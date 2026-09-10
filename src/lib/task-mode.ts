import type { Task } from "./task";

export type InteractionMode = "tui" | "rpc";
interface ModePorts {
  confirm(task: Task, target: InteractionMode): Promise<boolean>;
  stop(task: Task): Promise<void>;
  restart(task: Task, target: InteractionMode): Promise<Task>;
  busyChanged?(id: string, busy: boolean): void;
}

export function createTaskModeSwitcher(ports: ModePorts) {
  const busy = new Set<string>();
  return {
    isBusy: (id: string) => busy.has(id),
    async switch(task: Task, target: InteractionMode): Promise<Task | null> {
      if (busy.has(task.id) || (task.interactionMode ?? "tui") === target) return null;
      if (task.agent !== "pi" || !task.projectId || task.archivedAt !== null) {
        throw new Error("请先选择未归档的 Pi 项目任务");
      }
      const snapshot = { ...task };
      const assertCurrent = () => {
        if (task.runId !== snapshot.runId || task.interactionMode !== snapshot.interactionMode
          || task.archivedAt !== snapshot.archivedAt || task.sessionId !== snapshot.sessionId) {
          throw new Error("任务状态已发生变化，请重新选择切换操作");
        }
      };
      busy.add(task.id);
      ports.busyChanged?.(task.id, true);
      try {
        if (!await ports.confirm(snapshot, target)) return null;
        assertCurrent();
        if (!snapshot.runId && (task.status === "running" || task.status === "waiting")) {
          throw new Error("活动任务缺少运行标识，请刷新任务列表");
        }
        if (snapshot.runId) {
          const wasActive = task.status === "running" || task.status === "waiting";
          await ports.stop(snapshot);
          assertCurrent();
          if (wasActive) task.status = "cancelled";
        }
        return await ports.restart(snapshot, target);
      } finally {
        busy.delete(task.id);
        ports.busyChanged?.(task.id, false);
      }
    },
  };
}
