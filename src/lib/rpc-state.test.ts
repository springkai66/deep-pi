import { describe, expect, it } from "vitest";
import { applyRpcEvent, emptyConversation, loadHistory } from "./rpc-state";

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
    expect(state.tools.tool.running).toBe(false);
  });
});
