import { describe, expect, it } from "vitest";
import { applyRpcEvent, assistantMessageEmpty, canSubmitPrompt, emptyConversation, loadHistory, messageRenderable, readableRpcError } from "./rpc-state";

describe("RPC conversation reducer", () => {
  it("replaces cumulative messages instead of appending duplicate tokens", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 1, content: [] } } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_update", message: { role: "assistant", timestamp: 1, content: [{ type: "text", text: "hello" }] } } });
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "message_end", message: { role: "assistant", timestamp: 1, content: [{ type: "text", text: "hello world" }] } } });
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].content).toEqual([{ type: "text", text: "hello world" }]);
  });
  it("loads authoritative history and ignores events preceding its cursor", () => {
    let state = loadHistory(emptyConversation(), [{ role: "user", content: "hello", timestamp: 1 }], 10);
    state = applyRpcEvent(state, { sequence: 9, payload: { type: "message_start", message: { role: "user", content: "hello", timestamp: 1 } } });
    expect(state.messages).toHaveLength(1);
    expect(state.sequence).toBe(10);
  });
  it("replaces cumulative tool output and waits for agent_settled", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "agent_start" } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "tool_execution_start", toolCallId: "t", toolName: "read", args: { path: "a.ts" } } });
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "tool_execution_update", toolCallId: "t", partialResult: { content: [{ type: "text", text: "a" }] } } });
    state = applyRpcEvent(state, { sequence: 4, payload: { type: "tool_execution_update", toolCallId: "t", partialResult: { content: [{ type: "text", text: "ab" }] } } });
    state = applyRpcEvent(state, { sequence: 5, payload: { type: "agent_end" } });
    expect(state.busy).toBe(true);
    expect(state.tools.t.result).toEqual({ content: [{ type: "text", text: "ab" }] });
    state = applyRpcEvent(state, { sequence: 6, payload: { type: "agent_settled" } });
    expect(state.busy).toBe(false);
  });
  it("stamps heartbeat timestamps on events and tool updates for activity display", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "tool_execution_start", toolCallId: "t", toolName: "bash", args: { command: "sleep 600" } } });
    expect(state.lastEventAt).toBeGreaterThan(0);
    const firstStamp = state.tools.t.lastUpdateAt ?? 0;
    expect(firstStamp).toBeGreaterThan(0);
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "tool_execution_update", toolCallId: "t", partialResult: { content: [{ type: "text", text: "out" }] } } });
    // 本地时钟分辨率有限：两次打点时间可能相同，但绝不会倒退。
    expect(state.tools.t.lastUpdateAt).toBeGreaterThanOrEqual(firstStamp);
    // 重放的历史事件（sequence 更小）提前返回，不会误刷新心跳。
    const before = state.lastEventAt;
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "tool_execution_update", toolCallId: "t", partialResult: { content: [{ type: "text", text: "replay" }] } } });
    expect(state.lastEventAt).toBe(before);
  });
  it("reports exits and rejects replayed events", () => {
    let state = applyRpcEvent(emptyConversation(), { sequence: 4, payload: { type: "rpc_exit", code: 1 } });
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "agent_start" } });
    expect(state.closed).toBe(true);
    expect(state.busy).toBe(false);
  });
  it("releases duplicated live tool output after its final message", () => {
    let state = applyRpcEvent(emptyConversation(), { sequence: 1, payload: {
      type: "tool_execution_end", toolCallId: "tool", result: { content: "output" },
    } });
    state = applyRpcEvent(state, { sequence: 2, payload: {
      type: "message_end", message: { role: "toolResult", toolCallId: "tool", content: "output" },
    } });
    expect(state.tools.tool).toBeUndefined();
    expect(state.messages[0].content).toBe("output");
  });
  it("clears queued messages and stops live tool indicators on exit", () => {
    let state = emptyConversation();
    state.queue = ["queued"];
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "tool_execution_start", toolCallId: "tool" } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "rpc_exit" } });
    expect(state.queue).toEqual([]);
    expect(state.queueSteering).toEqual([]);
    expect(state.queueFollowUp).toEqual([]);
    expect(state.tools.tool.running).toBe(false);
  });
  it("splits queued prompts by delivery mode for the pending list", () => {
    let state = applyRpcEvent(emptyConversation(), { sequence: 1, payload: {
      type: "queue_update", steering: ["引导一", "引导二"], followUp: ["跟进一"],
    } });
    expect(state.queueSteering).toEqual(["引导一", "引导二"]);
    expect(state.queueFollowUp).toEqual(["跟进一"]);
    expect(state.queue).toEqual(["引导一", "引导二", "跟进一"]);
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "queue_update", steering: [], followUp: [] } });
    expect(state.queue).toEqual([]);
    // 兼容非字符串条目：直接过滤，不抛错。
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "queue_update", steering: ["ok", 42], followUp: [null, "ok2"] } });
    expect(state.queueSteering).toEqual(["ok"]);
    expect(state.queueFollowUp).toEqual(["ok2"]);
  });
});

describe("assistant message deltas (Pi 0.85+)", () => {
  it("accumulates text deltas into the streaming assistant message", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 7, content: [{ type: "text", text: "你" }] } } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_update", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "好" } } });
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "message_update", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "世界" } } });
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].content).toEqual([{ type: "text", text: "你好世界" }]);
  });

  it("keeps the timestamp so message_end still replaces instead of duplicating", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 7, content: [{ type: "text", text: "a" }] } } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_update", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "b" } } });
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "message_end", message: { role: "assistant", timestamp: 7, content: [{ type: "text", text: "ab" }] } } });
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].content).toEqual([{ type: "text", text: "ab" }]);
  });

  it("does not duplicate the first chunk when message_start pre-seeds it", () => {
    // Pi 会把首个分片先放进 message_start，随后 text_start + text_delta 再发一次，
    // 因此 text_start 必须重置该片段，否则首字会重复（实测出现 \"这这\"）。
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 5, content: [{ type: "text", text: "这" }] } } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_update", assistantMessageEvent: { type: "text_start", contentIndex: 0 } } });
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "message_update", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "这" } } });
    state = applyRpcEvent(state, { sequence: 4, payload: { type: "message_update", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "是" } } });
    expect(state.messages[0].content).toEqual([{ type: "text", text: "这是" }]);
  });

  it("routes thinking deltas to a thinking part and ignores start/end markers", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 9, content: [] } } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_update", assistantMessageEvent: { type: "thinking_start", contentIndex: 0 } } });
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "message_update", assistantMessageEvent: { type: "thinking_delta", contentIndex: 0, delta: "推理" } } });
    state = applyRpcEvent(state, { sequence: 4, payload: { type: "message_update", assistantMessageEvent: { type: "text_delta", contentIndex: 1, delta: "答案" } } });
    state = applyRpcEvent(state, { sequence: 5, payload: { type: "message_update", assistantMessageEvent: { type: "text_end", contentIndex: 1 } } });
    expect(state.messages[0].content).toEqual([
      { type: "thinking", thinking: "推理" },
      { type: "text", text: "答案" },
    ]);
  });

  it("does not mutate the previous conversation when applying deltas", () => {
    const started = applyRpcEvent(emptyConversation(), { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 3, content: [{ type: "text", text: "a" }] } } });
    const snapshot = JSON.stringify(started.messages);
    applyRpcEvent(started, { sequence: 2, payload: { type: "message_update", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "b" } } });
    expect(JSON.stringify(started.messages)).toBe(snapshot);
  });

  it("still accepts full messages from older Pi versions", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 1, content: [] } } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_update", message: { role: "assistant", timestamp: 1, content: [{ type: "text", text: "legacy" }] } } });
    expect(state.messages[0].content).toEqual([{ type: "text", text: "legacy" }]);
  });
});

describe("rpc error readability", () => {
  it("extracts the provider message from a status-prefixed JSON error", () => {
    const raw = '429: {"message":"You\'ve reached your weekly usage limit. Continue tomorrow.","type":"rate_limit_error","code":"RATE_LIMITED"}';
    expect(readableRpcError(raw)).toBe(
      "You've reached your weekly usage limit. Continue tomorrow.（rate_limit_error · HTTP 429）",
    );
  });

  it("handles a bare JSON error and leaves plain text untouched", () => {
    expect(readableRpcError('{"message":"quota exceeded","type":"quota_error"}')).toBe("quota exceeded（quota_error）");
    expect(readableRpcError("connection refused")).toBe("connection refused");
    expect(readableRpcError("")).toBe("");
    // JSON 里没有 message 字段时原样返回，不丢信息。
    expect(readableRpcError('429: {"code":"X"}')).toBe('429: {"code":"X"}');
    // @msg: 消息码原样透传（由 tm 渲染）。
    expect(readableRpcError("@msg:rpc.outcome_unknown?error=x")).toBe("@msg:rpc.outcome_unknown?error=x");
  });
});

describe("failed prompt cleanup", () => {
  it("drops the empty assistant turn left behind by a failed request", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "message_start", message: { role: "user", timestamp: 1, content: [{ type: "text", text: "hello" }] } } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_start", message: { role: "assistant", timestamp: 2, content: [] } } });
    expect(state.messages).toHaveLength(2);
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "rpc_error", error: '429: {"message":"limit reached","type":"rate_limit_error"}' } });
    expect(state.messages).toHaveLength(1); // 空 assistant 轮次被清掉
    expect(state.error).toBe("limit reached（rate_limit_error · HTTP 429）");
  });

  it("clears the stale error once a new turn starts", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "rpc_error", error: "fetch failed" } });
    expect(state.error).toBe("fetch failed");
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "agent_start" } });
    expect(state.error).toBe("");
    expect(state.phase).toBe("thinking");
  });

  it("clears the stale error once assistant streaming resumes", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "rpc_error", error: "fetch failed" } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_update", assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "hi" } } });
    expect(state.error).toBe("");
  });

  it("keeps the error while no recovery activity arrives", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "rpc_error", error: "fetch failed" } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "agent_settled" } });
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "queue_update", steering: [], followUp: [] } });
    expect(state.error).toBe("fetch failed");
  });

  it("keeps assistant turns that already have content", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 1, content: [{ type: "text", text: "partial" }] } } });
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "rpc_error", error: "boom" } });
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].content).toEqual([{ type: "text", text: "partial" }]);
    expect(state.error).toBe("boom");
  });
});

describe("empty assistant turns", () => {
  it("treats placeholder turns with no visible content as empty", () => {
    expect(assistantMessageEmpty({ role: "assistant", content: [] })).toBe(true);
    expect(assistantMessageEmpty({ role: "assistant", content: "" })).toBe(true);
    expect(assistantMessageEmpty({ role: "assistant", content: null })).toBe(true);
    expect(assistantMessageEmpty({ role: "assistant", content: [{ type: "text", text: "  " }] })).toBe(true);
    expect(assistantMessageEmpty({ role: "assistant", content: [{ type: "thinking", thinking: "" }] })).toBe(true);
    expect(assistantMessageEmpty({ role: "assistant", content: [{ type: "text", text: "hi" }] })).toBe(false);
    expect(assistantMessageEmpty({ role: "assistant", content: [{ type: "image", source: "data:image/png;base64,x" }] })).toBe(false);
    expect(assistantMessageEmpty({ role: "assistant", content: [{ type: "toolCall", name: "read" }] })).toBe(false);
    expect(assistantMessageEmpty({ role: "user", content: "" })).toBe(false);
  });

  it("render filter keeps every non-assistant message and filled assistant turns", () => {
    expect(messageRenderable({ role: "user", content: "" })).toBe(true);
    expect(messageRenderable({ role: "toolResult", content: "" })).toBe(true);
    expect(messageRenderable({ role: "assistant", content: [{ type: "text", text: "answer" }] })).toBe(true);
    expect(messageRenderable({ role: "assistant", content: [] })).toBe(false);
  });

  it("hides empty assistant turns left in the session history by failed prompts", () => {
    // 重试失败会在会话文件里留下多个空 assistant 占位；重连后按历史加载，
    // 渲染层过滤把它们全部隐藏（错误文本由横幅展示，不依赖这些空轮次）。
    const state = loadHistory(emptyConversation(), [
      { role: "user", content: [{ type: "text", text: "hello" }], timestamp: 1 },
      { role: "assistant", content: [], timestamp: 2 },
      { role: "assistant", content: [{ type: "text", text: "  " }], timestamp: 3 },
      { role: "user", content: [{ type: "text", text: "hello" }], timestamp: 4 },
      { role: "assistant", content: null, timestamp: 5 },
      { role: "assistant", content: [{ type: "text", text: "answer" }], timestamp: 6 },
    ], 10);
    const visible = state.messages.filter(messageRenderable);
    expect(state.messages).toHaveLength(6);
    expect(visible).toHaveLength(3);
    expect(visible.map((message) => message.role)).toEqual(["user", "user", "assistant"]);
  });
});
describe("context compaction", () => {
  it("marks compaction in progress and appends the summary as a compactionSummary message", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "compaction_start", reason: "auto" } });
    expect(state.compacting).toBe(true);
    state = applyRpcEvent(state, {
      sequence: 2,
      payload: { type: "compaction_end", reason: "auto", aborted: false, willRetry: false,
        result: { summary: "摘要内容", tokensBefore: 55_000, estimatedTokensAfter: 4_000 } },
    });
    expect(state.compacting).toBe(false);
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].role).toBe("compactionSummary");
    expect(state.messages[0].summary).toBe("摘要内容");
    expect(state.messages[0].tokensBefore).toBe(55_000);
    expect(state.messages[0].tokensAfter).toBe(4_000);
  });

  it("does not duplicate the summary when it already exists in the history", () => {
    let state = emptyConversation();
    state = loadHistory(emptyConversation(), [{ role: "compactionSummary", summary: "摘要", timestamp: 1 }], 1);
    state = applyRpcEvent(state, {
      sequence: 2,
      payload: { type: "compaction_end", reason: "manual", aborted: false, willRetry: false, result: { summary: "摘要", tokensBefore: 100 } },
    });
    expect(state.messages).toHaveLength(1);
    expect(state.compacting).toBe(false);
  });

  it("surfaces auto-compaction failures but leaves manual ones to the compact RPC rejection", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, {
      sequence: 1,
      payload: { type: "compaction_end", reason: "auto", aborted: false, willRetry: false, errorMessage: "Compaction failed: boom" },
    });
    expect(state.compacting).toBe(false);
    expect(state.error).toBe("Compaction failed: boom");
    expect(state.messages).toHaveLength(0);
    state = applyRpcEvent(state, {
      sequence: 2,
      payload: { type: "compaction_end", reason: "manual", aborted: false, willRetry: false, errorMessage: "Compaction failed: boom" },
    });
    expect(state.error).toBe("Compaction failed: boom");
  });
});

describe("execution visibility and concurrency errors", () => {
  it("tracks thinking, generation, tool execution and waiting phases", () => {
    let state = emptyConversation();
    state = applyRpcEvent(state, { sequence: 1, payload: { type: "agent_start" } });
    expect(state.phase).toBe("thinking");
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "agent_end" } });
    expect(state.phase).toBe("generating");
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "tool_execution_start", toolCallId: "x", toolName: "read" } });
    expect(state.phase).toBe("tool");
    state = applyRpcEvent(state, { sequence: 4, payload: { type: "agent_settled" } });
    expect(state.phase).toBe("waiting");
  });

  it("turns account concurrency errors into one actionable message", () => {
    const raw = '429: {"message":"Concurrency limit exceeded for account, please retry later","type":"rate_limit_error"}';
    expect(readableRpcError(raw)).toBe("账户并发请求已达到上限，请稍后重试");
    expect(readableRpcError("Concurrency limit exceeded for account, please retry later")).toBe("账户并发请求已达到上限，请稍后重试");
  });
});
  it("blocks a second prompt while the first submission is in flight", () => {
    expect(canSubmitPrompt(false, "hello")).toBe(true);
    expect(canSubmitPrompt(true, "hello")).toBe(false);
    expect(canSubmitPrompt(false, "   ")).toBe(false);
    // 图片：纯图片可发送；读取未完成时先等待，避免附件不完整导致丢图。
    expect(canSubmitPrompt(false, "", 1, 0)).toBe(true);
    expect(canSubmitPrompt(false, "", 0, 1)).toBe(false);
    expect(canSubmitPrompt(false, "hello", 0, 1)).toBe(false);
    expect(canSubmitPrompt(true, "", 1, 0)).toBe(false);
  });
