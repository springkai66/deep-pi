import type { Task } from "./task";

export const PET_TASK_ACTION_EVENT = "pet-task-action";
export const PET_TASK_RESULT_EVENT = "pet-task-result";
export type PetTaskAction = "stop" | "continue";
export interface PetTaskRequest {
  requestId: string;
  taskId: string;
  runId: string | null;
  action: PetTaskAction;
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

export function canContinuePetTask(task: Task): boolean {
  return task.agent === "pi" && task.archivedAt == null && !!task.projectId
    && (task.status === "waiting" || ((task.status === "cancelled" || task.status === "failed") && !task.runId));
}

export function visiblePetTasks(tasks: Task[], retained: ReadonlySet<string>): Task[] {
  return tasks.filter((task) => task.archivedAt == null && (
    task.status === "running" || task.status === "waiting"
    || (retained.has(task.id) && (task.status === "cancelled" || task.status === "failed"))
  ));
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
  updated(task: Task): void;
}

export function isPetTaskRequest(value: unknown): value is PetTaskRequest {
  if (!value || typeof value !== "object") return false;
  const request = value as Partial<PetTaskRequest>;
  return typeof request.requestId === "string" && request.requestId.length > 0
    && typeof request.taskId === "string" && request.taskId.length > 0
    && (request.runId === null || typeof request.runId === "string")
    && (request.action === "stop" || request.action === "continue");
}

/** Execute only in the main webview; existing native run-id guards remain intact. */
export function createPetTaskActionHandler(ports: ActionPorts) {
  return async (request: PetTaskRequest): Promise<PetTaskResult> => {
    const result = { requestId: request.requestId, taskId: request.taskId };
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
        if (!canContinuePetTask(task)) throw new Error("任务状态已改变，请刷新后重试");
        await ports.prepareContinue();
        // Navigation may await window operations; recheck before starting a run.
        task = await current();
        if (!canContinuePetTask(task)) throw new Error("任务状态已改变，请刷新后重试");
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
