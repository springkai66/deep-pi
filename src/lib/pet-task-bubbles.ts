import type { Task } from "./task";
import { canContinuePetTask, canStopPetTask, visiblePetTasks, type PetTaskAction } from "./pet-task-actions";

export interface PetTaskBubblesState {
  open: boolean;
  loading: boolean;
  tasks: Task[];
  pending: string[];
  error: string;
}
interface Ports {
  load(): Promise<Task[]>;
  resize(open: boolean, force?: boolean): Promise<void>;
  action(action: PetTaskAction, task: Task): Promise<void>;
  changed(state: PetTaskBubblesState): void;
  loaded?(tasks: Task[]): void;
}

/** Hover lifetime, fresh snapshots and native size transitions in one place. */
export function createPetTaskBubbles(ports: Ports, leaveMs = 400, refreshMs = 1500) {
  let state: PetTaskBubblesState = { open: false, loading: false, tasks: [], pending: [], error: "" };
  let desiredOpen = false;
  let inside = false;
  let disposed = false;
  let generation = 0;
  let readVersion = 0;
  let resizeQueue: Promise<void> = Promise.resolve();
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  let pollTimer: ReturnType<typeof setTimeout> | undefined;
  const retained = new Set<string>();
  const pending = new Set<string>();

  function publish(patch: Partial<PetTaskBubblesState>) {
    if (disposed) return;
    state = { ...state, ...patch, pending: [...pending] };
    ports.changed(state);
  }
  function clearTimers() {
    clearTimeout(leaveTimer);
    clearTimeout(pollTimer);
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
  async function refresh() {
    if (!desiredOpen || !state.open || disposed) return;
    const cycle = generation;
    const read = ++readVersion;
    clearTimeout(pollTimer);
    try {
      const tasks = await ports.load();
      if (disposed || cycle !== generation || read !== readVersion) return;
      ports.loaded?.(tasks);
      publish({ tasks: visiblePetTasks(tasks, retained), loading: false });
    } catch {
      if (disposed || cycle !== generation || read !== readVersion) return;
      publish({ loading: false, error: "无法读取任务，请重试" });
    } finally {
      if (cycle === generation && read === readVersion) poll();
    }
  }
  async function close(force = false) {
    if (!desiredOpen && !state.open) return;
    desiredOpen = false;
    generation++;
    readVersion++;
    clearTimers();
    publish({ open: false, loading: false });
    try { await resize(false, force); } catch {
      publish({ error: "桌宠窗口调整失败，请重试" });
    }
  }
  function leave() {
    inside = false;
    clearTimeout(leaveTimer);
    if (pending.size === 0) leaveTimer = setTimeout(() => { void close(); }, leaveMs);
  }
  function keepOpen() {
    inside = true;
    clearTimeout(leaveTimer);
  }
  async function enter() {
    if (disposed) return;
    keepOpen();
    if (desiredOpen) return;
    desiredOpen = true;
    const cycle = ++generation;
    publish({ loading: true, error: "", tasks: [] });
    try {
      await resize(true);
      if (disposed || cycle !== generation) return;
      publish({ open: true });
      await refresh();
    } catch {
      if (disposed || cycle !== generation) return;
      desiredOpen = false;
      publish({ open: false, loading: false, error: "桌宠窗口调整失败，请重试" });
      // Even partial native resize failures must be undone before the next open.
      await resize(false).catch(() => {});
    }
  }
  async function act(action: PetTaskAction, task: Task) {
    if (disposed || pending.has(task.id)) return;
    if (!(action === "stop" ? canStopPetTask(task) : canContinuePetTask(task))) return;
    pending.add(task.id);
    retained.add(task.id); // Keep a stopped task's bubble available for Continue.
    ++readVersion; // A pre-action snapshot must not overwrite the action result.
    clearTimeout(leaveTimer);
    publish({ error: "" });
    try {
      await ports.action(action, task);
    } catch (cause) {
      publish({ error: cause instanceof Error ? cause.message : String(cause) });
    } finally {
      // Refresh before unlocking, including after an unconfirmed/timeout result.
      await refresh();
      pending.delete(task.id);
      publish({});
      if (!inside) leave();
    }
  }
  return {
    enter, keepOpen, leave, close, refresh, act,
    async retry() {
      publish({ error: "", loading: true });
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
