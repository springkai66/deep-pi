/**
 * 任务完成的系统通知。
 *
 * 后端只广播结构化事件（`pty-exit` / `rpc-task-exit`），不产出文案
 * （与 message.rs 的约定一致）；通知标题/正文在这里按当前语言渲染，
 * 经 Tauri 通知插件发送——应用切到后台（最小化/失焦）时照样弹出。
 *
 * 可靠性规则：
 * - 以「任务列表的持久化状态」为准判定是否值得通知：取消、归档等
 *   非完成/失败状态一律静默（进程被杀的退出事件不会误报为失败）；
 * - runId 不匹配的过期事件（旧运行回调）不通知；
 * - 同一任务的同一运行只通知一次（跨 pty/rpc 双通道去重）；
 * - 通知发送失败只记日志，不影响任务流程。
 */

import type { Task, TaskStatus } from "./task";

/** pty-exit / rpc-task-exit 事件的公共字段（status 仅 rpc 携带）。 */
export interface TaskExitEvent {
  taskId: string;
  runId?: string | null;
  status?: TaskStatus;
  /** pty-exit 附加字段；通知判定以持久化状态为准，仅透传。 */
  exitCode?: number | null;
  error?: string | null;
}

/** 一条待发送的通知内容。 */
export interface TaskExitNotice {
  taskId: string;
  runId: string | null;
  title: string;
  status: TaskStatus;
}

const NOTIFY_STATUSES: ReadonlySet<string> = new Set(["completed", "failed"]);

/**
 * 判定一次任务退出是否应通知，并给出通知内容。
 * 事件未携带 status（pty-exit）时以任务列表里的持久化状态为准。
 * （同一运行的跨通道去重由调用方的同步抢占负责，见 createTaskExitNotifier。）
 */
export function resolveTaskExitNotice(event: TaskExitEvent, tasks: Task[]): TaskExitNotice | null {
  const task = tasks.find((candidate) => candidate.id === event.taskId);
  if (!task) return null;
  if (event.runId != null && task.runId !== event.runId) return null;
  const status = event.status ?? task.status;
  if (!NOTIFY_STATUSES.has(status)) return null;
  return { taskId: task.id, runId: task.runId ?? null, title: task.title, status };
}

/** 通知文案：状态行按当前语言渲染，标题用任务标题（用户内容，不翻译）。 */
export function taskExitNoticeBody(status: TaskStatus, translate: (text: string) => string): string {
  return status === "completed"
    ? translate("任务已完成")
    : translate("任务失败");
}

export interface TaskExitNotifierDeps {
  /** 拉取任务列表（持久化状态的事实来源）。 */
  fetchTasks: () => Promise<Task[]>;
  /** 设置开关：任务完成通知。 */
  isEnabled: () => boolean;
  /** 当前语言的状态文案。 */
  bodyFor: (status: TaskStatus) => string;
  /** 系统通知发送（内部处理权限，失败只记日志）。 */
  send: (input: { title: string; body: string }) => Promise<void>;
}

export interface TaskExitNotifier {
  handlePtyExit(event: TaskExitEvent): Promise<void>;
  handleRpcExit(event: TaskExitEvent): Promise<void>;
}

/**
 * 组装任务退出通知器：主窗口挂上 pty-exit / rpc-task-exit 两个监听，
 * 共享同一份去重表与发送管道。
 */
export function createTaskExitNotifier(deps: TaskExitNotifierDeps): TaskExitNotifier {
  const lastNotifiedRun = new Map<string, string | null>();

  async function handle(event: TaskExitEvent): Promise<void> {
    if (!deps.isEnabled()) return;
    // 同步抢占：并发到达的 pty/rpc 退出事件（同一任务同一运行）只可能
    // 有一个先到者完成去重登记，后到者直接放弃。
    if (event.runId != null) {
      if (lastNotifiedRun.get(event.taskId) === event.runId) return;
      lastNotifiedRun.set(event.taskId, event.runId);
    }
    let tasks: Task[];
    try {
      tasks = await deps.fetchTasks();
    } catch {
      // 任务列表不可用时保持静默：通知只是锦上添花。
      return;
    }
    const notice = resolveTaskExitNotice(event, tasks);
    if (!notice) return;
    try {
      await deps.send({ title: notice.title, body: deps.bodyFor(notice.status) });
    } catch {
      // 系统通知失败（权限/通道）不影响任务流程。
    }
  }

  return {
    handlePtyExit: (event) => handle(event),
    handleRpcExit: (event) => handle(event),
  };
}
