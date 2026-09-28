/**
 * 桌宠状态推导（纯函数）：瞬时事件（任务完成/失败）优先于任务列表派生。
 */

import type { Task } from "./task";

export type PetState = "idle" | "working" | "celebrate" | "sad";

/** 按任务列表派生常态：有运行中/等待输入的任务 → 工作，否则待机。 */
export function petStateFromTasks(tasks: Pick<Task, "status">[]): PetState {
  return tasks.some((task) => task.status === "running" || task.status === "waiting")
    ? "working"
    : "idle";
}

/** pty-exit / rpc-task-exit 事件的公共字段（status 仅 rpc 携带）。 */
export interface PetExitEvent {
  taskId: string;
  runId?: string | null;
  status?: "completed" | "failed" | "cancelled";
}

/**
 * 任务退出事件 → 桌宠瞬时状态（庆祝/沮丧）。
 * 取消、过期运行（runId 不匹配）、未知任务都返回 null——保持当前状态。
 * 打开桌宠时的历史完成任务没有退出事件，不会触发庆祝。
 */
export function petStateFromExit(event: PetExitEvent, tasks: Task[]): PetState | null {
  if (event.status !== "completed" && event.status !== "failed") return null;
  const task = tasks.find((candidate) => candidate.id === event.taskId);
  if (!task) return null;
  if (event.runId != null && task.runId !== event.runId) return null;
  return event.status === "completed" ? "celebrate" : "sad";
}

/** 瞬时状态的展示时长（毫秒），之后回落到任务列表派生的常态。 */
export const PET_TRANSIENT_MS = 3500;
