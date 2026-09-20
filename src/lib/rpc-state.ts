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
  /// 工具结果的执行起止时间（事件流内记的本地时钟），用于显示 Took；历史消息无此字段。
  toolStartedAt?: number;
  toolEndedAt?: number;
  /// 上下文压缩摘要消息（role === "compactionSummary"）：pi 压缩后重建上下文时插入。
  summary?: string;
  tokensBefore?: number;
  /// 压缩后估算 tokens：仅 live compaction_end 事件携带，历史重载的 pi 原生消息无此字段。
  tokensAfter?: number;
}

export interface RpcTool {
  id: string;
  name: string;
  args: unknown;
  result: unknown;
  running: boolean;
  isError: boolean;
  /// 执行起止时间：进行中的工具用 startedAt 显示 Elapsed 实时耗时。
  startedAt?: number;
  endedAt?: number;
  /// 最近一次输出事件的本地时间戳：tool_execution_update 本身就是心跳，
  /// 用于区分「输出仍在滚动」与「已 N 分钟无输出」。
  lastUpdateAt?: number;
}

export interface Conversation {
  sequence: number;
  messages: RpcMessage[];
  tools: Record<string, RpcTool>;
  busy: boolean;
  phase: "idle" | "thinking" | "generating" | "tool" | "waiting" | "failed";
  closed: boolean;
  error: string;
  queue: string[];
  /// 待发送提示词按投递模式分开：steering = 优先引导，followUp = 排队跟进；供输入框上方的待发送列表使用。
  queueSteering: string[];
  queueFollowUp: string[];
  /// 上下文压缩进行中（pi 的 compaction_start/compaction_end 事件或 get_state 的 isCompacting）。
  compacting: boolean;
  /// 最近一次收到任意 RPC 事件的本地时间戳：思考/生成阶段的事件流即心跳。
  lastEventAt?: number;
}

export function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

export function asMessage(value: unknown): RpcMessage | null {
  const candidate = record(value);
  if (typeof candidate.role !== "string") return null;
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

function applyAssistantUpdate(messages: RpcMessage[], update: AssistantUpdate): RpcMessage[] {
  const last = messages.at(-1);
  if (!last || last.role !== "assistant") return messages;
  const content: unknown[] = Array.isArray(last.content) ? [...last.content] : [];
  const node = record(content[update.index]);
  const current = update.reset || typeof node[update.kind] !== "string" ? "" : node[update.kind] as string;
  content[update.index] = { ...node, type: update.kind, [update.kind]: current + update.text };
  return [...messages.slice(0, -1), { ...last, content }];
}

export function readableRpcError(raw: string): string {
  const text = (raw ?? "").trim();
  if (!text) return text;
  const prefixed = /^(\d{3}):\s*(\{[\s\S]*\})$/.exec(text);
  const json = prefixed ? prefixed[2] : prefixed === null ? (/^\{[\s\S]*\}$/.exec(text)?.[0] ?? "") : "";
  const status = prefixed ? prefixed[1] : "";
  if (!json) return /concurrency\s+limit\s+exceeded/i.test(text) ? t("账户并发请求已达到上限，请稍后重试") : text;
  try {
    const parsed = JSON.parse(json) as Record<string, unknown>;
    const message = typeof parsed.message === "string" && parsed.message.trim() ? parsed.message.trim() : "";
    if (!message) return text;
    if (/concurrency\s+limit\s+exceeded/i.test(message)) return t("账户并发请求已达到上限，请稍后重试");
    const bits = [typeof parsed.type === "string" && parsed.type ? parsed.type : "", status ? `HTTP ${status}` : ""].filter(Boolean);
    return bits.length ? `${message}（${bits.join(" · ")}）` : message;
  } catch {
    return text;
  }
}
export function canSubmitPrompt(inFlight: boolean, draft: string, attachments = 0, pendingImageReads = 0): boolean {
  // 图片读取完成前不允许发送：否则 send() 拿到的是不完整的附件列表，图片会被静默丢弃。
  if (inFlight || pendingImageReads > 0) return false;
  return draft.trim().length > 0 || attachments > 0;
}

export function assistantMessageEmpty(message: RpcMessage): boolean {
  if (message.role !== "assistant") return false;
  if (typeof message.content === "string") return message.content.trim().length === 0;
  if (message.content == null) return true;
  if (!Array.isArray(message.content)) return false;
  return message.content.every((value) => {
    const part = record(value);
    if (part.type === "text") return !(typeof part.text === "string" && part.text.trim().length > 0);
    if (part.type === "thinking") return !(typeof part.thinking === "string" && part.thinking.trim().length > 0);
    return false;
  });
}

export function messageRenderable(message: RpcMessage): boolean {
  return message.role !== "assistant" || !assistantMessageEmpty(message);
}

export function dropTrailingEmptyAssistant(messages: RpcMessage[]): RpcMessage[] {
  let end = messages.length;
  while (end > 0 && assistantMessageEmpty(messages[end - 1])) end -= 1;
  return end === messages.length ? messages : messages.slice(0, end);
}

export function emptyConversation(): Conversation {
  return { sequence: 0, messages: [], tools: {}, busy: false, phase: "idle", closed: false, error: "", queue: [], queueSteering: [], queueFollowUp: [], compacting: false };
}

export function loadHistory(state: Conversation, values: unknown[], sequence: number): Conversation {
  return { ...state, messages: values.map(asMessage).filter((message): message is RpcMessage => message !== null), sequence };
}

export function applyRpcEvent(previous: Conversation, event: RpcEvent): Conversation {
  if (event.sequence <= previous.sequence) return previous;
  // 任意事件到达都刷新心跳；重放的历史事件（sequence 更小）在上面已提前返回，不会误刷新。
  const state = { ...previous, sequence: event.sequence, lastEventAt: Date.now() };
  const payload = event.payload;
  const type = payload.type;
  // 新回合开始 = 会话已从上一次失败中恢复，清掉遗留错误，避免错误卡片一直钉在会话流底部。
  if (type === "agent_start") { state.busy = true; state.phase = "thinking"; state.error = ""; }
  else if (type === "agent_end") { state.busy = true; state.phase = "generating"; }
  else if (type === "agent_settled") { state.busy = false; state.phase = "waiting"; }
  else if (type === "rpc_exit") {
    state.closed = true; state.busy = false; state.phase = "waiting"; state.queue = []; state.queueSteering = []; state.queueFollowUp = [];
    state.tools = Object.fromEntries(Object.entries(previous.tools).map(([id, tool]) => [id, { ...tool, running: false, isError: tool.isError || tool.running }]));
  } else if (type === "rpc_error" || type === "extension_error") {
    state.error = typeof payload.error === "string" ? readableRpcError(payload.error) : t("RPC 运行失败");
    state.phase = "failed"; state.busy = false; state.messages = dropTrailingEmptyAssistant(state.messages);
  } else if (type === "compaction_start") {
    // pi 压缩开始（手动 compact RPC 或上下文超出阈值自动触发）。
    state.compacting = true;
  } else if (type === "compaction_end") {
    state.compacting = false;
    // 成功结果里带 summary/tokensBefore：pi 重建上下文时不会为压缩摘要发 message_end 事件，
    // 这里补一条 compactionSummary 消息（与 pi createCompactionSummaryMessage 同构），让会话流展示压缩结果。
    const result = record(payload.result);
    const summary = typeof result.summary === "string" ? result.summary : "";
    if (summary && !previous.messages.some((message) => message.role === "compactionSummary" && message.summary === summary)) {
      state.messages = [...previous.messages, {
        role: "compactionSummary",
        summary,
        tokensBefore: typeof result.tokensBefore === "number" ? result.tokensBefore : undefined,
        tokensAfter: typeof result.estimatedTokensAfter === "number" ? result.estimatedTokensAfter : undefined,
        timestamp: Date.now(),
      } as RpcMessage];
    }
    // 自动压缩失败没有 RPC 调用方接收错误，手动失败走 compact RPC 的 rejection 提示，这里只补自动路径。
    const reason = payload.reason;
    if (reason !== "manual" && typeof payload.errorMessage === "string" && payload.errorMessage) {
      state.error = readableRpcError(payload.errorMessage);
    }
  } else if (type === "queue_update") {
    const steering = Array.isArray(payload.steering) ? payload.steering.filter((item): item is string => typeof item === "string") : [];
    const followUp = Array.isArray(payload.followUp) ? payload.followUp.filter((item): item is string => typeof item === "string") : [];
    state.queueSteering = steering;
    state.queueFollowUp = followUp;
    state.queue = [...steering, ...followUp];
  } else if (type === "message_update" && payload.assistantMessageEvent) {
    const update = asAssistantUpdate(payload.assistantMessageEvent);
    // 流式输出恢复同样说明会话已恢复，清掉上一轮遗留的错误。
    if (update) { state.error = ""; state.phase = update.kind === "thinking" ? "thinking" : "generating"; state.messages = applyAssistantUpdate(previous.messages, update); }
  } else if (type === "message_start" || type === "message_update" || type === "message_end") {
    const message = asMessage(payload.message);
    if (!message) return state;
    const last = previous.messages.at(-1);
    const same = last && last.role === message.role && (message.timestamp !== undefined && last.timestamp === message.timestamp || message.toolCallId !== undefined && last.toolCallId === message.toolCallId);
    state.messages = same ? [...previous.messages.slice(0, -1), message] : [...previous.messages, message];
    if (message.role === "assistant") state.phase = "generating";
    if (type === "message_end" && message.role === "toolResult" && message.toolCallId) {
      state.tools = { ...previous.tools };
      // 把工具执行的起止时间附到结果消息上，供 UI 显示 Took；随后清理运行态记录。
      const finishedTool = state.tools[message.toolCallId];
      if (finishedTool?.startedAt !== undefined) {
        message.toolStartedAt = finishedTool.startedAt;
        message.toolEndedAt = finishedTool.endedAt ?? Date.now();
      }
      delete state.tools[message.toolCallId];
    }
    if (message.errorMessage) { state.error = readableRpcError(message.errorMessage); state.phase = "failed"; }
  } else if (type === "tool_execution_start" || type === "tool_execution_update" || type === "tool_execution_end") {
    const id = typeof payload.toolCallId === "string" ? payload.toolCallId : "";
    if (!id) return state;
    const existing = previous.tools[id] ?? { id, name: String(payload.toolName ?? t("工具")), args: payload.args, result: null, running: true, isError: false };
    state.phase = "tool";
    // 记录执行起止时间（pi TUI 同款 Elapsed / Took 的数据源）；事件间隔即本地时钟差。
    const startedAt = existing.startedAt ?? Date.now();
    const endedAt = type === "tool_execution_end" ? Date.now() : existing.endedAt;
    state.tools = { ...previous.tools, [id]: { ...existing, result: type === "tool_execution_update" ? payload.partialResult : type === "tool_execution_end" ? payload.result : existing.result, running: type !== "tool_execution_end", isError: payload.isError === true, startedAt, endedAt, lastUpdateAt: Date.now() } };
  }
  return state;
}
