import type { HostCommand } from "./shortcuts";

/// 原生加速键事件允许的三个 host 作用域命令。
///
/// 与 Rust 侧 `DSH_SHORTCUTS` 保持一致；其他作用域命令（侧栏、对话输入、
/// 保存、差异导航）在 DSH 焦点下没有可用上下文，不参与原生路由。
const DSH_HOST_COMMANDS = ["tasks", "files", "settings"] as const;

export function dshShortcutCommand(payload: unknown): HostCommand | null {
  return typeof payload === "string"
    && (DSH_HOST_COMMANDS as readonly string[]).includes(payload)
    ? (payload as HostCommand)
    : null;
}
