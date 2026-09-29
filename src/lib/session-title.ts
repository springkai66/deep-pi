import type { RpcMessage } from "./rpc-state";

/** Bounded conversation context for the title model; keep the opening request and recent turns. */
export function sessionTitleContext(messages: RpcMessage[]): { role: "user" | "assistant"; text: string }[] {
  const turns = messages.flatMap((message) => {
    if (message.role !== "user" && message.role !== "assistant") return [];
    const text = typeof message.content === "string" ? message.content : Array.isArray(message.content)
      ? message.content.filter((part): part is { type: "text"; text: string } =>
        part?.type === "text" && typeof part.text === "string").map((part) => part.text).join("\n") : "";
    return text.trim() ? [{ role: message.role as "user" | "assistant", text: text.trim().slice(0, 700) }] : [];
  });
  return turns.length > 16 ? [turns[0], ...turns.slice(-15)] : turns;
}
