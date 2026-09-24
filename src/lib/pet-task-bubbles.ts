/**
 * 桌宠任务环的控制器：悬停生命周期、任务快照、原生窗口显隐、
 * 单任务动作与失败提示，全部收在这一处，页面只做渲染。
 *
 * 两个窗口共用本控制器，但用法不同：
 * - 桌宠本体窗口只关心 `enter()` / `close()`——「悬停即打开环」；
 * - 浮层窗口负责「鼠标是否还在环上」的宽限期与退场动画完成后的真正隐藏。
 */

import type { Task } from "./task";
import { canOpenPetTask, canStopPetTask, visiblePetTasks, type PetTaskAction } from "./pet-task-actions";
import { ringOverflowCount } from "./pet-task-ring";

export interface PetTaskBubblesState {
  /** 环是否显示（退场动画期间仍为 true，见 leaving）。 */
  open: boolean;
  /** 正在依次消失；视图据此播放退场，播完调用 finishClose()。 */
  leaving: boolean;
  loading: boolean;
  /** 刷新结果为空且无错误：显示「暂无进行中任务」并计时自动收起。 */
  empty: boolean;
  tasks: Task[];
  /** 已下发、尚未确认的任务 id。 */
  pending: string[];
  /** 按任务 id 记录的动作失败原因（其中 id → 时间戳 的部分仅供内部计时）。 */
  actionErrors: Record<string, string>;
  /** 环上某任务进入结束态的时刻；用于「结束后停留几秒再消失」。 */
  endedAt: Record<string, number>;
  /** 列表读取失败（环级提示，与单任务动作失败区分开）。 */
  error: string;
}

interface Ports {
  load(): Promise<Task[]>;
  /** open=true 显示浮层，false 隐藏；force 用于窗口销毁前的兜底隐藏。 */
  resize(open: boolean, force?: boolean): Promise<void>;
  action(action: PetTaskAction, task: Task): Promise<void>;
  changed(state: PetTaskBubblesState): void;
  loaded?(tasks: Task[]): void;
}

export interface PetTaskBubblesOptions {
  /** 鼠标离开后、开始退场前的宽限期：留出走到气泡上的时间。 */
  leaveMs?: number;
  /** 任务快照轮询间隔。 */
  refreshMs?: number;
  /** 结束后气泡在环上停留的时长，然后按退场规则消失。 */
  endedHoldMs?: number;
  /** 空环（无进行中任务）自动收起前的停留时长。 */
  emptyCloseMs?: number;
}

/** 结束状态（含被停止）在环上停留多久。 */
const ENDED_HOLD_MS = 2500;

/** 悬停生命周期、任务快照与原生显隐都收在这里。 */
export function createPetTaskBubbles(ports: Ports, options: PetTaskBubblesOptions = {}) {
  const leaveMs = options.leaveMs ?? 400;
  const refreshMs = options.refreshMs ?? 1500;
  const endedHoldMs = options.endedHoldMs ?? ENDED_HOLD_MS;
  const emptyCloseMs = options.emptyCloseMs ?? 1500;
  let state: PetTaskBubblesState = {
    open: false,
    leaving: false,
    loading: false,
    empty: false,
    tasks: [],
    pending: [],
    actionErrors: {},
    endedAt: {},
    error: "",
  };
  let desiredOpen = false;
  /** 指针是否悬在桌宠本体上（enter/leave 事件维护；空环收起时据此决定是否设闩）。 */
  let petHovered = false;
  /** 防循环闩：空环自动收起时指针仍在本体上 → 指针离开前不再重开，否则收起→重开闪烁。 */
  let emptyLatch = false;
  /** 指针在环上（浮层内）：即使在退场中也要把环拉回来。 */
  let inside = false;
  let disposed = false;
  let generation = 0;
  let readVersion = 0;
  let resizeQueue: Promise<void> = Promise.resolve();
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  let pollTimer: ReturnType<typeof setTimeout> | undefined;
  let heldFadeTimer: ReturnType<typeof setTimeout> | undefined;
  let emptyTimer: ReturnType<typeof setTimeout> | undefined;
  /** 最近一次发布给视图的元素数，退场动画时长按它算。 */
  let slotCount = 0;
  const retained = new Set<string>();
  const pending = new Set<string>();
  const actionErrors = new Map<string, string>();

  function publish(patch: Partial<PetTaskBubblesState>) {
    if (disposed) return;
    state = {
      ...state,
      ...patch,
      pending: [...pending],
      actionErrors: Object.fromEntries(actionErrors),
    };
    // 退场动画时长按「下一次渲染会有几个元素」算，含溢出角标。
    slotCount = state.tasks.length + (ringOverflowCount(state.tasks.length) > 0 ? 1 : 0);
    ports.changed(state);
  }

  /** 动作失败会就地展示错误：此时不能偷偷把环收掉，否则用户永远看不到失败原因。 */
  function hasActionError() {
    return [...actionErrors.values()].some((message) => message.length > 0);
  }

  function clearTimers() {
    clearTimeout(leaveTimer);
    clearTimeout(pollTimer);
    clearTimeout(heldFadeTimer);
    clearTimeout(emptyTimer);
  }

  function resize(open: boolean, force = false) {
    const job = resizeQueue.then(() => ports.resize(open, force));
    resizeQueue = job.catch(() => {});
    return job;
  }

  function poll() {
    clearTimeout(pollTimer);
    if (desiredOpen && !disposed) pollTimer = setTimeout(() => { void refresh(); }, refreshMs);
  }

  /**
   * 结束的任务在环上停留 endedHoldMs 后，再按退场规则逐条淡出。
   * 计时只在首次观测到结束态时启动，刷新不会把它推后。
   */
  function scheduleHeldFade(tasks: Task[]) {
    clearTimeout(heldFadeTimer);
    const now = Date.now();
    const ended = tasks.filter((task) => isEndedTask(task));
    if (!ended.length) return;
    // 用「观测到结束的时刻」而不是后端的 completedAt：后者可能是几分钟前，
    // 打开环时不该让一个早就结束的任务立刻消失。
    const firstSeen = Math.min(...ended.map((task) => state.endedAt[task.id] ?? now));
    heldFadeTimer = setTimeout(() => { void close(ended.length); }, Math.max(0, endedHoldMs - (now - firstSeen)));
  }

  /** 空环停留 emptyCloseMs 后自动收起；指针仍在本体上则设防循环闩。 */
  function scheduleEmptyClose() {
    clearTimeout(emptyTimer);
    emptyTimer = setTimeout(() => {
      emptyTimer = undefined;
      if (petHovered) emptyLatch = true;
      void close();
    }, emptyCloseMs);
  }

  async function refresh() {
    if (!desiredOpen || !state.open || disposed || state.leaving) return;
    const cycle = generation;
    const read = ++readVersion;
    clearTimeout(pollTimer);
    try {
      const tasks = await ports.load();
      if (disposed || cycle !== generation || read !== readVersion) return;
      ports.loaded?.(tasks);
      // 刚结束的任务要留在环上把结果讲完：先把它们登记进保留集，
      // 再据此过滤，否则从 running 变 completed 的那一帧气泡会直接消失。
      const now = Date.now();
      for (const task of tasks) {
        if (task.archivedAt == null && isEndedTask(task)) retained.add(task.id);
      }
      const visible = visiblePetTasks(tasks, retained);
      const nextEndedAt: Record<string, number> = {};
      for (const task of visible) {
        if (isEndedTask(task)) nextEndedAt[task.id] = state.endedAt[task.id] ?? now;
      }
      publish({ tasks: visible, loading: false, endedAt: nextEndedAt, empty: visible.length === 0 });
      // 空环给一段「暂无进行中任务」的停留后自动收起；有任务则取消该计时。
      if (visible.length === 0) scheduleEmptyClose(); else clearTimeout(emptyTimer);
      scheduleHeldFade(visible);
    } catch {
      if (disposed || cycle !== generation || read !== readVersion) return;
      clearTimeout(emptyTimer);
      publish({ loading: false, error: "无法读取任务，请重试", empty: false });
    } finally {
      if (cycle === generation && read === readVersion) poll();
    }
  }

  /** 真正把浮层窗口藏起来；退场动画播完由视图调用。 */
  async function finishClose() {
    if (!state.open) return;
    // 退场途中指针又回到环上：撤销退场，保持显示（重新开启会打断动画）。
    if (inside && desiredOpen) {
      publish({ leaving: false });
      return;
    }
    clearTimeout(leaveTimer);
    clearTimeout(heldFadeTimer);
    publish({ open: false, leaving: false, loading: false, empty: false });
    try {
      await resize(false);
    } catch {
      publish({ error: "桌宠窗口调整失败，请重试" });
    }
  }

  /**
   * 开始退场：先播动画（leaving），由视图在 ringExitMs 之后调用 finishClose()。
   * 环上没有元素时无可播内容，直接收窗口。
   */
  async function close(slots?: number) {
    if (!desiredOpen && !state.open) return;
    const count = slots ?? slotCount;
    desiredOpen = false;
    generation++;
    readVersion++;
    clearTimers();
    if (count > 0 && state.open) {
      publish({ leaving: true, loading: false });
      return;
    }
    await finishClose();
  }

  function leave() {
    petHovered = false;
    // 指针离开本体：闩解除，下次悬停可重开。
    emptyLatch = false;
    inside = false;
    clearTimeout(leaveTimer);
    if (pending.size === 0 && !hasActionError()) {
      leaveTimer = setTimeout(() => { void close(); }, leaveMs);
    }
  }

  function keepOpen() {
    inside = true;
    // 退场途中指针回到环上：这不只是「保持」，而是撤销关闭。
    if (state.leaving) desiredOpen = true;
    clearTimeout(leaveTimer);
  }

  async function enter() {
    if (disposed) return;
    // 防循环闩：空环自动收起后指针未离开本体，不重开（否则收起→重开闪烁循环）。
    if (emptyLatch) return;
    // 已经在显示（或正在退场被拉回）：只需续上宽限期，不重播打开流程。
    if (desiredOpen) { keepOpen(); return; }
    petHovered = true;
    desiredOpen = true;
    inside = true;
    const cycle = ++generation;
    publish({ leaving: false, loading: true, error: "", endedAt: {}, empty: false });
    try {
      await resize(true);
      if (disposed || cycle !== generation) return;
      publish({ open: true });
      await refresh();
    } catch {
      if (disposed || cycle !== generation) return;
      desiredOpen = false;
      publish({ open: false, leaving: false, loading: false, error: "桌宠窗口调整失败，请重试" });
      // Even partial native resize failures must be undone before the next open.
      await resize(false).catch(() => {});
    }
  }

  async function act(action: PetTaskAction, task: Task) {
    if (disposed || pending.has(task.id)) return;
    if (action === "stop" && !canStopPetTask(task)) return;
    if (action === "open" && !canOpenPetTask(task)) return;
    pending.add(task.id);
    // 被停止的任务要留在环上把结果讲完；打开只是跳转，不需要保留项。
    if (action === "stop") retained.add(task.id);
    ++readVersion; // A pre-action snapshot must not overwrite the action result.
    clearTimeout(leaveTimer);
    actionErrors.delete(task.id);
    publish({ error: "" });
    try {
      await ports.action(action, task);
    } catch (cause) {
      actionErrors.set(task.id, cause instanceof Error ? cause.message : String(cause));
      publish({});
    } finally {
      // Refresh before unlocking, including after an unconfirmed/timeout result.
      await refresh();
      pending.delete(task.id);
      publish({});
      // 打开成功即由主窗口接管：环退场。失败则留在环上把原因讲完；
      // 停止动作沿用原规则——指针已经离开就补一次退场。
      if (!hasActionError() && (action === "open" || !inside)) void close();
    }
  }

  return {
    enter, keepOpen, leave, close, finishClose, refresh, act,
    async retry() {
      publish({ error: "", loading: true, empty: false });
      if (state.open) await refresh(); else await enter();
    },
    dispose() {
      const needsClose = desiredOpen || state.open;
      disposed = true;
      desiredOpen = false;
      generation++;
      readVersion++;
      clearTimers();
      if (needsClose) void resize(false, true).catch(() => {});
    },
  };
}

/** 结束态：任务已不在运行/等待，气泡只剩「最后看一眼」的价值。 */
export function isEndedTask(task: Pick<Task, "status">): boolean {
  return task.status !== "running" && task.status !== "waiting";
}
