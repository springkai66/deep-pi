import { describe, expect, it } from "vitest";
import { applyRpcEvent, emptyConversation, loadHistory } from "./rpc-state";
import { showConversationMessage, visibleMessageText } from "./chat-presentation";

const messages = [
  { role: "user", content: "Question" },
  { role: "assistant", content: [{ type: "thinking", thinking: "private" }] },
  { role: "assistant", content: [{ type: "text", text: "Answer" }, { type: "toolCall", name: "read", arguments: { path: "secret" } }] },
  { role: "toolResult", toolName: "read", content: "log output" },
  { role: "toolResult", toolName: "shell", content: "secret failure log", isError: true },
  { role: "compactionSummary", summary: "private context", content: "" },
];

describe("Pi conversation display for live and reloaded messages", () => {
  it("shows only answers, user content, compression status and actionable failures in concise mode", () => {
    const loaded = loadHistory(emptyConversation(), messages, 1);
    expect(loaded.messages.filter((message) => showConversationMessage(message, "concise")).map((m) => m.role))
      .toEqual(["user", "assistant", "toolResult", "compactionSummary"]);
    expect(visibleMessageText(loaded.messages[2])).toBe("Answer");
    expect(visibleMessageText(loaded.messages[4])).toBe("");
  });

  it("applies the same projection to streamed assistant updates and tool failures", () => {
    let live = emptyConversation();
    live = applyRpcEvent(live, { sequence: 1, payload: { type: "message_start", message: { role: "assistant", timestamp: 1, content: [{ type: "thinking", thinking: "private" }] } } });
    expect(live.messages.filter((message) => showConversationMessage(message, "concise"))).toHaveLength(0);
    live = applyRpcEvent(live, { sequence: 2, payload: { type: "message_update", message: { role: "assistant", timestamp: 1, content: [{ type: "thinking", thinking: "private" }, { type: "text", text: "Answer" }] } } });
    expect(live.messages.filter((message) => showConversationMessage(message, "concise"))).toHaveLength(1);
    expect(visibleMessageText(live.messages[0])).toBe("Answer");
    live = applyRpcEvent(live, { sequence: 3, payload: { type: "message_end", message: { role: "toolResult", toolCallId: "tool", isError: true, content: "private log" } } });
    expect(live.messages.filter((message) => showConversationMessage(message, "concise")).map((m) => m.role)).toEqual(["assistant", "toolResult"]);
    expect(visibleMessageText(live.messages[1])).toBe("");
  });

  it("keeps process details available in full mode and reprojects after switching", () => {
    const loaded = loadHistory(emptyConversation(), messages, 1);
    const displayed = (detail: "concise" | "verbose") => loaded.messages.filter((message) => showConversationMessage(message, detail));
    expect(displayed("verbose")).toHaveLength(6);
    expect(displayed("concise")).toHaveLength(4);
    expect(displayed("verbose")).toHaveLength(6);
  });

  it("filters a history page loaded after switching by the currently selected mode", () => {
    const firstPage = loadHistory(emptyConversation(), messages.slice(2), 2);
    const older = [...messages.slice(0, 2), ...firstPage.messages];
    expect(older.filter((message) => showConversationMessage(message, "concise")).map((m) => m.role))
      .toEqual(["user", "assistant", "toolResult", "compactionSummary"]);
  });
});
