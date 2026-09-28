import { messageRenderable, record, type RpcMessage } from "./rpc-state";
import type { ChatDetailLevel } from "./settings";

/** The same visibility policy applies to live messages and pages loaded from history. */
export function showConversationMessage(message: RpcMessage, detail: ChatDetailLevel): boolean {
  if (message.errorMessage) return true;
  if (detail === "verbose") return messageRenderable(message);
  if (message.role === "user" || message.role === "compactionSummary") return true;
  if (message.role === "toolResult") return record(message).isError === true;
  if (message.role !== "assistant") return false;
  if (typeof message.content === "string") return message.content.trim().length > 0;
  if (!Array.isArray(message.content)) return false;
  // Visibility needs only the block type, not image decoding or a scan of its base64 payload.
  return message.content.some((value) => {
    const part = record(value);
    return part.type === "image" || (part.type === "text" && typeof part.text === "string" && part.text.trim().length > 0);
  });
}

/** Excludes thinking, tool calls and unknown blocks from concise copy and fallbacks. */
export function visibleMessageText(message: RpcMessage): string {
  if (message.role !== "user" && message.role !== "assistant") return "";
  if (typeof message.content === "string") return message.content;
  if (!Array.isArray(message.content)) return "";
  return message.content.flatMap((value) => {
    const part = record(value);
    return part.type === "text" && typeof part.text === "string" ? [part.text] : [];
  }).join("\n");
}
