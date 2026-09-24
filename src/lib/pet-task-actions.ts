import type { Task } from "./task";

export const PET_TASK_ACTION_EVENT = "pet-task-action";
export const PET_TASK_RESULT_EVENT = "pet-task-result";
/** 桌宠气泡可发起的动作：结束 Pi 任务，或在主窗口打开该任务（Pi / DSH 都支持）。 */
export type PetTaskAction = "stop" | "open";
/** 主窗口执行的完整动作集：继续不在桌宠上暴露，但主窗口仍然需要它。 */
export type TaskAction = PetTaskAction | "continue";
export interface PetTaskRequest {
  requestId: string;
  taskId: string;
  runId: string | null;
  action: PetTaskAction;
}
export interface TaskActionRequest {
  requestId: string;
  taskId: string;
  runId: string | null;
  action: TaskAction;
}
export interface PetTaskResult {
  requestId: string;
  taskId: string;
  ok: boolean;
  error?: string;
}

export function canStopPetTask(task: Task): boolean {
  return task.agent === "pi" && task.archivedAt == null && !!task.runId
    && (task.status === "running" || task.status === "waiting");
}

/**
 * 打开（跳转）适用面：未归档的任务都能在主窗口定位到——Pi 任务进任务面板，
 * DSH 任务切到 DSH 视图。只读动作，不要求有活动运行。
 */
export function canOpenPetTask(task: Pick<Task, "archivedAt">): boolean {
  return task.archivedAt == null;
}

/** 继续的适用面：等输入的活动会话，或已停止但可重启的休眠任务。 */
export function canContinueTask(task: Task): boolean {
  return task.agent === "pi" && task.archivedAt == null && !!task.projectId
    && (task.status === "waiting"
      || ((task.status === "cancelled" || task.status === "failed") && !task.runId));
}

/**
 * 环上显示哪些任务：只显示「正在执行」的任务（status = running）。
 * Pi 空闲会话是 waiting、DSH 空闲会话同理，都不占环；已结束的任务也不再
 * 停留——环只讲当下正在跑什么。
 * agent 给定时只保留该工作流的任务（跟随主窗口当前所在的 Pi / DSH 工作区）。
 */
export function visiblePetTasks(tasks: Task[], agent?: Task["agent"] | null): Task[] {
  return tasks.filter((task) =>
    task.archivedAt == null
    && task.status === "running"
    && (!agent || task.agent === agent));
}

interface ActionPorts {
  loadTask(id: string): Promise<Task | undefined>;
  // Share these locks with main-window stop/restart and mode switching.
  isBusy(id: string): boolean;
  busyChanged(id: string, busy: boolean): void;
  stop(task: Task): Promise<void>;
  restart(task: Task): Promise<Task>;
  prepareContinue(): Promise<void>;
  open(task: Task): void;
  /** 把主窗口带到前台并定位到该任务（Pi 进任务面板，DSH 进 DSH 视图）。 */
  reveal(task: Task): Promise<void> | void;
  updated(task: Task): void;
}

/** 只接桌宠能发出的载荷：凡是带 continue 的旧形态请求一律视为无效。 */
export function isPetTaskRequest(value: unknown): value is PetTaskRequest {
  if (!value || typeof value !== "object") return false;
  const request = value as Partial<PetTaskRequest>;
  return typeof request.requestId === "string" && request.requestId.length > 0
    && typeof request.taskId === "string" && request.taskId.length > 0
    && (request.runId === null || typeof request.runId === "string")
    && (request.action === "stop" || request.action === "open");
}

/** Execute only in the main webview; existing native run-id guards remain intact. */
export function createPetTaskActionHandler(ports: ActionPorts) {
  return async (request: TaskActionRequest): Promise<PetTaskResult> => {
    const result = { requestId: request.requestId, taskId: request.taskId };
    // 打开是只读动作：不占任务锁、不改任务状态，前台化失败也要如实回报。
    if (request.action === "open") {
      try {
        const task = await ports.loadTask(request.taskId);
        if (!task || !canOpenPetTask(task)) throw new Error("任务不存在或已归档");
        await ports.reveal(task);
        return { ...result, ok: true };
      } catch (cause) {
        return { ...result, ok: false, error: cause instanceof Error ? cause.message : String(cause) };
      }
    }
    if (ports.isBusy(request.taskId)) return { ...result, ok: false, error: "任务正在处理，请稍后重试" };
    ports.busyChanged(request.taskId, true);
    try {
      const current = async () => {
        const task = await ports.loadTask(request.taskId);
        if (!task || task.archivedAt != null || (task.runId ?? null) !== request.runId) {
          throw new Error("任务状态已改变，请刷新后重试");
        }
        if (task.agent !== "pi") throw new Error("DSH 任务请在主窗口操作");
        return task;
      };
      let task = await current();
      if (request.action === "stop") {
        if (!canStopPetTask(task)) throw new Error("任务状态已改变，请刷新后重试");
        await ports.stop(task);
        const stopped = await ports.loadTask(task.id);
        if (stopped) ports.updated(stopped);
      } else {
        if (!canContinueTask(task)) throw new Error("任务状态已改变，请刷新后重试");
        await ports.prepareContinue();
        // Navigation may await window operations; recheck before starting a run.
        task = await current();
        if (!canContinueTask(task)) throw new Error("任务状态已改变，请刷新后重试");
        // A waiting live session already exists: never create a duplicate process.
        const resumed = task.runId ? task : await ports.restart(task);
        ports.updated(resumed);
        ports.open(resumed);
      }
      return { ...result, ok: true };
    } catch (cause) {
      return { ...result, ok: false, error: cause instanceof Error ? cause.message : String(cause) };
    } finally {
      ports.busyChanged(request.taskId, false);
    }
  };
}

/** Correlate action acknowledgements; never interpret an emitted event as success. */
export function createPetTaskActionClient(send: (request: PetTaskRequest) => Promise<void>, timeoutMs = 30_000) {
  const pending = new Map<string, {
    taskId: string;
    resolve(): void;
    reject(error: Error): void;
    timer: ReturnType<typeof setTimeout>;
  }>();
  return {
    request(action: PetTaskAction, task: Task): Promise<void> {
      const requestId = crypto.randomUUID();
      return new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(requestId);
          reject(new Error("任务操作结果未确认，请刷新核对后再试"));
        }, timeoutMs);
        pending.set(requestId, { taskId: task.id, resolve, reject, timer });
        void send({ requestId, action, taskId: task.id, runId: task.runId ?? null }).catch((cause) => {
          const operation = pending.get(requestId);
          if (!operation) return;
          clearTimeout(operation.timer);
          pending.delete(requestId);
          operation.reject(cause instanceof Error ? cause : new Error(String(cause)));
        });
      });
    },
    receive(result: PetTaskResult) {
      const operation = pending.get(result.requestId);
      if (!operation || operation.taskId !== result.taskId) return;
      clearTimeout(operation.timer);
      pending.delete(result.requestId);
      if (result.ok) operation.resolve();
      else operation.reject(new Error(result.error || "任务操作失败，请重试"));
    },
    dispose() {
      for (const operation of pending.values()) {
        clearTimeout(operation.timer);
        operation.reject(new Error("桌宠已关闭"));
      }
      pending.clear();
    },
  };
}
