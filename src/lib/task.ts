export type TaskStatus =
  | "running"
  | "waiting"
  | "completed"
  | "failed"
  | "cancelled";

export interface Task {
  id: string;
  runId?: string | null;
  title: string;
  agent: "pi" | "dsh";
  status: TaskStatus;
  projectId: string | null;
  projectPath: string;
  sessionId: string;
  sessionFile: string | null;
  executionTarget: "local";
  createdAt: number;
  startedAt: number | null;
  completedAt: number | null;
  archivedAt: number | null;
}

export const statusLabels = {
  running: "运行中",
  waiting: "等待输入",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
} satisfies Record<TaskStatus, string>;
