import { messageParts } from "./message-parts";
import { messageRenderable, record, type RpcMessage } from "./rpc-state";
import type { ChatDetailLevel } from "./settings";

/** The same visibility policy applies to live messages and pages loaded from history. */
export function showConversationMessage(message: RpcMessage, detail: ChatDetailLevel): boolean {
  if (detail === "verbose") return messageRenderable(message);
  if (message.role === "user" || message.role === "compactionSummary") return true;
  if (message.role === "toolResult") return record(message).isError === true || Boolean(message.errorMessage);
  if (message.role !== "assistant") return false;
  return messageParts(message.content).some((part) =>
    part.kind === "image" || (part.kind === "text" && part.content.trim().length > 0));
}

/** Excludes thinking, tool calls and unknown blocks from concise copy and fallbacks. */
export function visibleMessageText(message: RpcMessage): string {
  if (message.role !== "user" && message.role !== "assistant") return "";
  return messageParts(message.content).filter((part) => part.kind === "text").map((part) => part.content).join("\n");
}
