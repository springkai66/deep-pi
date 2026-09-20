import { contentText, type RpcMessage } from "./rpc-state";

/** 自动标题上限：足够容纳首条提示词的完整首行，悬停提示不再显示省略结尾。 */
export const SESSION_TITLE_LIMIT = 200;

/** 用首条用户消息的首个非空行作为会话标题；空会话返回 null。 */
export function deriveSessionTitle(messages: RpcMessage[]): string | null {
  const first = messages.find((message) => message.role === "user");
  if (!first) return null;
  const line = contentText(first.content).split("\n").map((part) => part.trim()).find((part) => part.length > 0);
  if (!line) return null;
  return line.length > SESSION_TITLE_LIMIT ? `${line.slice(0, SESSION_TITLE_LIMIT)}…` : line;
}

/**
 * 旧版本把自动标题截断到 24 字并追加 `…`；若当前标题是这种截断标题、
 * 且新推导的标题延续其内容，则允许升级为更完整的标题（只做一次，由调用方控制）。
 */
export function isTruncatedTitleUpgrade(current: string, next: string): boolean {
  const trimmed = current.trim();
  if (!(trimmed.endsWith("…") || trimmed.endsWith("..."))) return false;
  const base = trimmed.replace(/(?:…|\.\.\.)$/, "").trim();
  return base.length > 0 && next.length > trimmed.length && next.startsWith(base);
}
