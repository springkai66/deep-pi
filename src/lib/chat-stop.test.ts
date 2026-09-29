import { describe, expect, it } from "vitest";
import { applyRpcEvent, emptyConversation, loadHistory } from "./rpc-state";
import { lastUserAnchor, readStopRecords, saveStopRecord, suppressStoppedAbort, withStopRecords, type StopRecord } from "./chat-stop";

function memoryStorage(): Storage {
  const entries = new Map<string, string>();
  return {
    getItem: (key) => entries.get(key) ?? null,
    setItem: (key, value) => { entries.set(key, value); },
  } as Storage;
}

const stop: StopRecord = { id: "stop-1", anchor: 100, timestamp: 200 };
const user = { role: "user", timestamp: 100, content: "原始任务" };
const partial = { role: "assistant", timestamp: 110, content: [{ type: "text", text: "有效回复" }] };
const aborted = { role: "assistant", timestamp: 120, content: [{ type: "text", text: "Request aborted" }], errorMessage: "Request aborted" };

describe("user-initiated stop in a Pi conversation", () => {
  it("persists a user-facing stop across a fresh history load without sending it to Pi", () => {
    const storage = memoryStorage();
    expect(readStopRecords(storage, "task-1")).toEqual([]);
    saveStopRecord(storage, "task-1", stop);
    expect(readStopRecords(storage, "task-2")).toEqual([]);
    const history = loadHistory(emptyConversation(), [user, partial, aborted], 5);
    const displayed = withStopRecords(history.messages, readStopRecords(storage, "task-1"));
    expect(displayed.map((message) => [message.role, message.content])).toEqual([
      ["user", "原始任务"], ["assistant", partial.content], ["user", "停止任务"],
    ]);
    expect(history.messages).toHaveLength(3);
    expect(history.messages[2].errorMessage).toBe("Request aborted");
  });

  it("hides only the abort block and preserves other assistant content and real failures", () => {
    const mixed = { ...aborted, content: [{ type: "text", text: "有效回复" }, { type: "text", text: "Request aborted" }] };
    const failure = { role: "assistant", timestamp: 140, content: [], errorMessage: "rate limit" };
    const result = withStopRecords([user, mixed, failure], [stop]);
    expect(result[1].content).toEqual([{ type: "text", text: "有效回复" }]);
    expect(result[1].errorMessage).toBeUndefined();
    expect(result[2].errorMessage).toBe("rate limit");
    expect(result[3].content).toBe("停止任务");
    expect(withStopRecords([user, aborted], [])).toEqual([user, aborted]);
  });

  it("suppresses a matching in-flight abort error, but preserves command failures and future turn errors", () => {
    let state = loadHistory(emptyConversation(), [user, partial], 1);
    expect(lastUserAnchor(state.messages)).toBe(100);
    state = applyRpcEvent(state, { sequence: 2, payload: { type: "message_end", message: aborted } });
    expect(state.error).toBe("Request aborted");
    expect(suppressStoppedAbort(state, [], 100)).toMatchObject({ error: "", errorRaw: undefined });
    expect(suppressStoppedAbort(state, [], null).error).toBe("Request aborted");
    state = suppressStoppedAbort(state, [stop], null);
    state = applyRpcEvent(state, { sequence: 3, payload: { type: "agent_settled" } });
    expect(state.busy).toBe(false);
    state = applyRpcEvent(state, { sequence: 4, payload: { type: "message_end", message: { role: "user", timestamp: 300, content: "继续" } } });
    state = applyRpcEvent(state, { sequence: 5, payload: { type: "message_end", message: { role: "assistant", timestamp: 310, content: [], errorMessage: "rate limit" } } });
    expect(suppressStoppedAbort(state, [stop], null).error).toBe("rate limit");
    expect(withStopRecords(state.messages, [stop]).map((message) => message.content)).toEqual([
      "原始任务", partial.content, "停止任务", "继续", [],
    ]);
  });

  it("does not repeat stop markers when an older history page is prepended", () => {
    const older = { role: "user", content: "旧任务", timestamp: 10 };
    const recent = [user, partial, aborted, { role: "user", content: "新任务", timestamp: 300 }];
    expect(withStopRecords(recent, [stop]).filter((message) => message.content === "停止任务")).toHaveLength(1);
    expect(withStopRecords([older, ...recent], [stop]).filter((message) => message.content === "停止任务")).toHaveLength(1);
    const pageWithoutUser = withStopRecords([partial, aborted, recent[3]], [stop]);
    expect(pageWithoutUser.map((message) => message.content)).toEqual([partial.content, "停止任务", "新任务"]);
    expect(withStopRecords([user, ...[partial, aborted, recent[3]]], [stop]).filter((message) => message.content === "停止任务")).toHaveLength(1);
  });
});
