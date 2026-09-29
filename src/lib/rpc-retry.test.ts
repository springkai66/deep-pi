import { describe, expect, it, vi } from "vitest";
import { applyRpcEvent, emptyConversation } from "./rpc-state";

describe("Pi automatic transport retry lifecycle", () => {
  const failedAttempt = () => {
    let state = applyRpcEvent(emptyConversation(), { sequence: 1, payload: { type: "agent_start" } });
    return applyRpcEvent(state, { sequence: 2, payload: {
      type: "message_end", message: { role: "assistant", timestamp: 1, content: [], stopReason: "error", errorMessage: "terminated" },
    } });
  };

  it("shows an ongoing retry instead of a terminal failure and keeps the run busy", () => {
    const state = applyRpcEvent(failedAttempt(), { sequence: 3, payload: {
      type: "auto_retry_start", attempt: 1, maxAttempts: 3, delayMs: 2000, errorMessage: "terminated",
    } });
    expect(state.error).toBe("");
    expect(state.phase).toBe("retrying");
    expect(state.busy).toBe(true);
    expect(state.closed).toBe(false);
    expect(state.retry).toEqual({ attempt: 1, maxAttempts: 3, delayMs: 2000, error: "terminated", rawError: "terminated", startedAt: expect.any(Number) });
  });

  it("clears the prior attempt error even when the successful response has no text deltas", () => {
    let state = applyRpcEvent(failedAttempt(), { sequence: 3, payload: {
      type: "auto_retry_start", attempt: 1, maxAttempts: 3, delayMs: 1, errorMessage: "terminated",
    } });
    state = applyRpcEvent(state, { sequence: 4, payload: {
      type: "message_start", message: { role: "assistant", timestamp: 2, content: [] },
    } });
    expect(state.retry).toBeUndefined();
    state = applyRpcEvent(state, { sequence: 5, payload: { type: "auto_retry_end", success: true, attempt: 1 } });
    expect(state.error).toBe("");
    expect(state.busy).toBe(true);
    state = applyRpcEvent(state, { sequence: 6, payload: { type: "agent_settled" } });
    expect(state.busy).toBe(false);
    expect(state.error).toBe("");
  });

  it("preserves the final failure when retry is exhausted and clears retry state on exit", () => {
    let state = applyRpcEvent(failedAttempt(), { sequence: 3, payload: {
      type: "auto_retry_start", attempt: 3, maxAttempts: 3, delayMs: 8000, errorMessage: "terminated",
    } });
    state = applyRpcEvent(state, { sequence: 4, payload: {
      type: "auto_retry_end", success: false, attempt: 3, finalError: "WebSocket error",
    } });
    expect(state.error).toBe("WebSocket error");
    expect(state.errorRaw).toBe("WebSocket error");
    expect(state.phase).toBe("failed");
    expect(state.retry).toBeUndefined();
    expect(state.closed).toBe(false);
    state = applyRpcEvent(state, { sequence: 5, payload: { type: "agent_settled" } });
    expect(state.error).toBe("WebSocket error");
    state = applyRpcEvent(state, { sequence: 6, payload: { type: "auto_retry_start", attempt: 1, maxAttempts: 3, delayMs: 1 } });
    state = applyRpcEvent(state, { sequence: 7, payload: { type: "rpc_exit" } });
    expect(state.errorRaw).toBeUndefined();
    expect(state.retry).toBeUndefined();
    expect(state.busy).toBe(false);
    expect(state.closed).toBe(true);
  });
});

it("does not restart the retry countdown when unrelated RPC events arrive", () => {
  const clock = vi.spyOn(Date, "now").mockReturnValue(1000);
  try {
    let state = applyRpcEvent(emptyConversation(), { sequence: 1, payload: { type: "auto_retry_start", attempt: 1, maxAttempts: 3, delayMs: 2000 } });
    expect(state.retry?.startedAt).toBe(1000);
    clock.mockReturnValue(2500);
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "queue_update", steering: ["queued"] } });
    expect(state.lastEventAt).toBe(2500);
    expect(state.retry?.startedAt).toBe(1000);
  } finally { clock.mockRestore(); }
});
