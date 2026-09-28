/**
 * 「任务看板」浮窗与主窗口之间的动作协议。
 *
 * 看板浮窗不持有终端窗格、模式切换器、确认对话框等主窗口状态，
 * 因此卡片上的操作（打开/停止/重开/归档/恢复/删除/新建）不直接执行，
 * 而是通过 Tauri 事件发给主窗口，由主窗口复用既有的业务函数处理；
 * 看板随后靠 `task-status` / `rpc-task-exit` 事件与兜底重载保持同步。
 */
export const BOARD_ACTION_EVENT = "board-action";

export type BoardAction =
  | "open"
  | "stop"
  | "restart"
  | "archive"
  | "restore"
  | "delete"
  | "new-task";

export interface BoardActionRequest {
  action: BoardAction;
  taskId?: string;
  projectId?: string;
}
