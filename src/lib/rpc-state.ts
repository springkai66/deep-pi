import { t } from "./i18n.svelte";
export interface RpcEvent {
  sequence: number;
  payload: Record<string, unknown>;
}

export interface RpcMessage {
  role: string;
  content: unknown;
  timestamp?: number;
  toolCallId?: string;
  toolName?: string;
  errorMessage?: string;
}

export interface RpcTool {
  id: string;
  name: string;
  args: unknown;
  result: unknown;
  running: boolean;
  isError: boolean;
}

export interface Conversation {
  sequence: number;
  messages: RpcMessage[];
  tools: Record<string, RpcTool>;
  busy: boolean;
  closed: boolean;
  error: string;
  queue: string[];
}

export function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

export function asMessage(value: unknown): RpcMessage | null {
  const candidate = record(value);
  if (typeof candidate.role !== "string") return null;
  // SAFETY: 这里只校验 role，其余字段由 Pi 的 RPC 协议约定；消费端（contentText、
  // 渲染与 toolCallId/timestamp 比对）在读取每个字段前都会再做运行时收窄，
  // 因此保留原始对象而不重建，避免丢掉 api/provider/model 等后续可能用到的字段。
  return candidate as unknown as RpcMessage;
}

export function contentText(content: unknown): string {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content.map((part) => {
    const node = record(part);
    if (node.type === "text" && typeof node.text === "string") return node.text;
    if (node.type === "thinking" && typeof node.thinking === "string") return node.thinking;
    if (node.type === "image") return t("[图片]");
    if (node.type === "toolCall") return `${String(node.name ?? t("工具"))}\n${JSON.stringify(node.arguments ?? {}, null, 2)}`;
    return "";
  }).filter(Boolean).join("\n");
}

interface AssistantUpdate {
  kind: "text" | "thinking";
  index: number;
  text: string;
  reset: boolean;
}

/**
 * Pi 0.85 起，流式增量在 `assistantMessageEvent` 里（`text_delta` / `thinking_delta`），
 * 而不是完整的 `message`；旧版本仍在 `message` 里给全量内容，两条路径都要兼容。
 *
 * 注意：`message_start` 会先把首个分片写进 content，随后 `text_start` 会从同一个
 * contentIndex 重新发送这一分片，所以 `*_start` 必须重置该片段，否则首字会重复。
 */
function asAssistantUpdate(value: unknown): AssistantUpdate | null {
  const event = record(value);
  const type = typeof event.type === "string" ? event.type : "";
  const rawIndex = event.contentIndex;
  const index = typeof rawIndex === "number" && Number.isInteger(rawIndex) && rawIndex >= 0 ? rawIndex : 0;
  const delta = typeof event.delta === "string" ? event.delta : "";
  if (type === "text_start") return { kind: "text", index, text: "", reset: true };
  if (type === "thinking_start") return { kind: "thinking", index, text: "", reset: true };
  if (type === "text_delta" && delta) return { kind: "text", index, text: delta, reset: false };
  if (type === "thinking_delta" && delta) return { kind: "thinking", index, text: delta, reset: false };
  return null;
}

/** 把增量拼到最后一条 assistant 消息的对应片段上；没有占位消息时等 `message_end` 补全。 */
function applyAssistantUpdate(messages: RpcMessage[], update: AssistantUpdate): RpcMessage[] {
  const last = messages.at(-1);
  if (!last || last.role !== "assistant") return messages;
  const content: unknown[] = Array.isArray(last.content) ? [...last.content] : [];
  const node = record(content[update.index]);
  const current = update.reset || typeof node[update.kind] !== "string" ? "" : node[update.kind] as string;
  content[update.index] = { ...node, type: update.kind, [update.kind]: current + update.text };
  return [...messages.slice(0, -1), { ...last, content }];
}


/**
 * 把 `429: {"message":"…","type":"rate_limit_error",…}` 这类原始错误收敛成可读文本：
 * 提取 JSON 的 message 字段，附上错误类型；解析失败原样返回。
 *
 * provider（或 Pi）给错误响应的形态是「HTTP 状态码: 响应体」，直接展示会是一整段
 * JSON；会话里至少出现 message/type/code 三个字段，只展示人能读懂的部分。
 */
export function readableRpcError(raw: string): string {
  const text = (raw ?? "").trim();
  if (!text) return text;
  // 两种形态：`429: {json…}` 与纯 `{"message":…}`（后者不带状态码）。
  const prefixed = /^(\d{3}):\s*(\{[\s\S]*\})$/.exec(text);
  const json = prefixed ? prefixed[2] : prefixed === null ? (/^\{[\s\S]*\}$/.exec(text)?.[0] ?? "") : "";
  const status = prefixed ? prefixed[1] : "";
  if (!json) return text;
  try {
    const parsed = JSON.parse(json) as Record<string, unknown>;
    const message = typeof parsed.message === "string" && parsed.message.trim() ? parsed.message.trim() : "";
    if (!message) return text;
    // 状态码与错误类型附在句尾，便于定位；正文才是人要读的部分。
    const bits = [
      typeof parsed.type === "string" && parsed.type ? parsed.type : "",
      status ? `HTTP ${status}` : "",
    ].filter(Boolean);
    return bits.length ? `${message}（${bits.join(" · ")}）` : message;
  } catch {
    return text; // 不是合法 JSON，按原样展示
  }
}

/** assistant 轮次是否没有任何可见内容（纯 text/thinking 且全空白）。 */
function assistantMessageEmpty(message: RpcMessage): boolean {
  if (message.role !== "assistant") return false;
  if (typeof message.content === "string") return message.content.trim().length === 0;
  if (!Array.isArray(message.content)) return false;
  return message.content.every((value) => {
    const part = record(value);
    if (part.type === "text") return !(typeof part.text === "string" && part.text.trim().length > 0);
    if (part.type === "thinking") return !(typeof part.thinking === "string" && part.thinking.trim().length > 0);
    // image / toolCall / 未知片段都视为有内容，避免误删真实输出。
    return false;
  });
}

/** 从尾部移除连续的空白 assistant 轮次（失败请求的占位残留）。 */
export function dropTrailingEmptyAssistant(messages: RpcMessage[]): RpcMessage[] {
  let end = messages.length;
  while (end > 0 && assistantMessageEmpty(messages[end - 1])) end -= 1;
  return end === messages.length ? messages : messages.slice(0, end);
}

export function emptyConversation(): Conversation {
  return { sequence: 0, messages: [], tools: {}, busy: false, closed: false, error: "", queue: [] };
}

export function loadHistory(state: Conversation, values: unknown[], sequence: number): Conversation {
  return { ...state, messages: values.map(asMessage).filter((message): message is RpcMessage => message !== null), sequence };
}

export function applyRpcEvent(previous: Conversation, event: RpcEvent): Conversation {
  if (event.sequence <= previous.sequence) return previous;
  const state = { ...previous, sequence: event.sequence };
  const payload = event.payload;
  const type = payload.type;
  if (type === "agent_start") state.busy = true;
  else if (type === "agent_settled") state.busy = false;
  else if (type === "rpc_exit") {
    state.closed = true;
    state.busy = false;
    state.queue = [];
    state.tools = Object.fromEntries(Object.entries(previous.tools).map(([id, tool]) =>
      [id, { ...tool, running: false, isError: tool.isError || tool.running }]));
  }
  else if (type === "rpc_error" || type === "extension_error") {
    state.error = typeof payload.error === "string"
      ? readableRpcError(payload.error)
      : t("RPC 运行失败");
    // 失败的请求只会留下一个没有任何内容的 assistant 占位轮次（Pi 在失败前已发
    // message_start）；把它从会话尾部清掉，否则界面会出现一整块空白的"黑屏"轮次。
    state.messages = dropTrailingEmptyAssistant(state.messages);
  } else if (type === "queue_update") {
    state.queue = [...(Array.isArray(payload.steering) ? payload.steering : []),
      ...(Array.isArray(payload.followUp) ? payload.followUp : [])].filter((item): item is string => typeof item === "string");
  } else if (type === "message_update" && payload.assistantMessageEvent) {
    const update = asAssistantUpdate(payload.assistantMessageEvent);
    if (update) state.messages = applyAssistantUpdate(previous.messages, update);
  } else if (type === "message_start" || type === "message_update" || type === "message_end") {
    const message = asMessage(payload.message);
    if (!message) return state;
    const last = previous.messages.at(-1);
    const same = last && last.role === message.role &&
      (message.timestamp !== undefined && last.timestamp === message.timestamp ||
        message.toolCallId !== undefined && last.toolCallId === message.toolCallId);
    state.messages = same ? [...previous.messages.slice(0, -1), message] : [...previous.messages, message];
    if (type === "message_end" && message.role === "toolResult" && message.toolCallId) {
      state.tools = { ...previous.tools };
      delete state.tools[message.toolCallId];
    }
    if (message.errorMessage) state.error = readableRpcError(message.errorMessage);
  } else if (type === "tool_execution_start" || type === "tool_execution_update" || type === "tool_execution_end") {
    const id = typeof payload.toolCallId === "string" ? payload.toolCallId : "";
    if (!id) return state;
    const existing = previous.tools[id] ?? { id, name: String(payload.toolName ?? t("工具")), args: payload.args, result: null, running: true, isError: false };
    state.tools = { ...previous.tools, [id]: {
      ...existing,
      result: type === "tool_execution_update" ? payload.partialResult : type === "tool_execution_end" ? payload.result : existing.result,
      running: type !== "tool_execution_end",
      isError: payload.isError === true,
    } };
  }
  return state;
}
