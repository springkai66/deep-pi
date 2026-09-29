import { t } from "./i18n.svelte";
import { contentText, type Conversation, type RpcMessage } from "./rpc-state";

export interface StopRecord {
  id: string;
  anchor: number;
  timestamp: number;
}

const storageKey = (taskId: string) => `deeppi:chat-stops:${taskId}`;
const aborted = (text: string) => text.trim().toLowerCase() === "request aborted";

export function readStopRecords(storage: Storage, taskId: string): StopRecord[] {
  try {
    const value: unknown = JSON.parse(storage.getItem(storageKey(taskId)) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.filter((item): item is StopRecord => item !== null && typeof item === "object"
      && typeof item.id === "string" && typeof item.anchor === "number" && Number.isFinite(item.anchor)
      && typeof item.timestamp === "number" && Number.isFinite(item.timestamp));
  } catch {
    return [];
  }
}

export function saveStopRecord(storage: Storage, taskId: string, record: StopRecord): StopRecord[] {
  const records = [...readStopRecords(storage, taskId), record];
  storage.setItem(storageKey(taskId), JSON.stringify(records));
  return records;
}

export function lastUserAnchor(messages: RpcMessage[]): number | null {
  for (let index = messages.length - 1; index >= 0; index--) {
    const message = messages[index];
    if (message.role === "user" && typeof message.timestamp === "number") return message.timestamp;
  }
  return null;
}

export function suppressStoppedAbort(state: Conversation, records: StopRecord[], pendingAnchor: number | null): Conversation {
  const anchor = lastUserAnchor(state.messages);
  if (anchor === null || (anchor !== pendingAnchor && !records.some((item) => item.anchor === anchor))) return state;
  return aborted(state.error) ? { ...state, error: "", errorRaw: undefined, phase: state.busy ? state.phase : "waiting" } : state;
}

function visibleStoppedAssistant(message: RpcMessage): RpcMessage | null {
  if (message.role !== "assistant") return message;
  const content = message.content;
  const cleaned = typeof content === "string"
    ? (aborted(content) ? "" : content)
    : Array.isArray(content) ? content.filter((part) => !(part?.type === "text" && typeof part.text === "string" && aborted(part.text))) : content;
  const cleanedMessage = { ...message, content: cleaned, errorMessage: aborted(message.errorMessage ?? "") ? undefined : message.errorMessage };
  if (cleanedMessage.errorMessage) return cleanedMessage;
  return contentText(cleaned).trim() || (Array.isArray(cleaned) && cleaned.some((part) => part?.type === "toolCall" || part?.type === "image"))
    ? cleanedMessage : null;
}

/** Display-only messages: stop actions never enter Pi's model context. */
export function withStopRecords(messages: RpcMessage[], records: StopRecord[]): RpcMessage[] {
  if (!records.length) return messages;
  const byAnchor = new Map<number, StopRecord[]>();
  for (const record of records) byAnchor.set(record.anchor, [...byAnchor.get(record.anchor) ?? [], record]);
  const result: RpcMessage[] = [];
  // A history page can start in the middle of the stopped turn, without its user message.
  const firstTimestamp = messages[0]?.timestamp;
  let active: StopRecord[] = typeof firstTimestamp === "number"
    ? records.filter((record) => record.anchor < firstTimestamp && firstTimestamp <= record.timestamp)
    : [];
  const flush = () => {
    for (const record of active) result.push({ role: "user", content: t("停止任务"), timestamp: record.timestamp });
    active = [];
  };
  for (const message of messages) {
    if (message.role === "user") {
      flush();
      active = typeof message.timestamp === "number" ? byAnchor.get(message.timestamp) ?? [] : [];
    }
    const displayed = active.length ? visibleStoppedAssistant(message) : message;
    if (displayed) result.push(displayed);
  }
  flush();
  return result;
}
