import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { applyRpcEvent, emptyConversation, loadHistory } from "./rpc-state";
import { prependRpcHistory } from "./rpc-history";
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

  it("keeps an assistant failure visible after reloading history without a live error banner", () => {
    const restored = loadHistory(emptyConversation(), [
      { role: "assistant", content: [], errorMessage: "Request failed" },
    ], 1);
    expect(restored.error).toBe("");
    expect(restored.messages.filter((message) => showConversationMessage(message, "concise"))).toHaveLength(1);
  });

  it("does not inspect base64 image data just to decide whether a message is visible", () => {
    let payloadReads = 0;
    const image = { type: "image", mimeType: "image/png", get data() { payloadReads++; return "a".repeat(6 * 1024 * 1024); } };
    const restored = loadHistory(emptyConversation(), [{ role: "assistant", content: [image] }], 1);
    expect(showConversationMessage(restored.messages[0], "concise")).toBe(true);
    expect(payloadReads).toBe(0);
  });

  it("reprojects live tools through success, failure and mode switches", () => {
    let state = loadHistory(emptyConversation(), [
      { role: "assistant", content: [{ type: "thinking", thinking: "older private thought" }] },
      { role: "user", content: "Question" },
    ], 1);
    const event = (sequence: number, payload: Record<string, unknown>) => { state = applyRpcEvent(state, { sequence, payload }); };
    const visible = (mode: "concise" | "verbose") => state.messages.filter((message) => showConversationMessage(message, mode));

    event(2, { type: "tool_execution_start", toolCallId: "ok", toolName: "bash", args: { command: "private command" } });
    expect(state.tools.ok.running).toBe(true);
    expect(visible("concise").map((message) => message.role)).toEqual(["user"]);
    event(3, { type: "tool_execution_end", toolCallId: "ok", result: { content: "private output" } });
    expect(state.tools.ok.running).toBe(false);
    expect(state.tools.ok.isError).toBe(false);
    event(4, { type: "message_end", message: { role: "toolResult", toolCallId: "ok", content: "private output" } });
    expect(state.tools.ok).toBeUndefined();
    expect(visible("concise").map((message) => message.role)).toEqual(["user"]);
    expect(visible("verbose").map((message) => message.role)).toEqual(["assistant", "user", "toolResult"]);

    event(5, { type: "tool_execution_start", toolCallId: "fail", toolName: "bash", args: { command: "private failing command" } });
    event(6, { type: "tool_execution_end", toolCallId: "fail", isError: true, result: { content: "private failure" } });
    expect(state.tools.fail.isError).toBe(true);
    event(7, { type: "message_end", message: { role: "toolResult", toolCallId: "fail", isError: true, content: "private failure" } });
    expect(visible("concise").map((message) => message.role)).toEqual(["user", "toolResult"]);
    expect(visible("verbose")).toHaveLength(4);
    expect(visible("concise")).toHaveLength(2);
  });

  it("keeps process details available in full mode and reprojects after switching", () => {
    const loaded = loadHistory(emptyConversation(), messages, 1);
    const displayed = (detail: "concise" | "verbose") => loaded.messages.filter((message) => showConversationMessage(message, detail));
    expect(displayed("verbose")).toHaveLength(6);
    expect(displayed("concise")).toHaveLength(4);
    expect(displayed("verbose")).toHaveLength(6);
  });

  it("filters a prepended history page by the currently selected mode", () => {
    const firstPage = loadHistory(emptyConversation(), messages.slice(2), 2);
    const older = prependRpcHistory(firstPage, {
      snapshotId: "snapshot", eventSequence: 2, start: 0, end: 2, total: 6,
      messages: loadHistory(emptyConversation(), messages.slice(0, 2), 2).messages,
    }, 2);
    expect(older.messages.filter((message) => showConversationMessage(message, "concise")).map((m) => m.role))
      .toEqual(["user", "assistant", "toolResult", "compactionSummary"]);
    expect(older.messages.filter((message) => showConversationMessage(message, "verbose"))).toHaveLength(6);
  });
});

describe("ChatPane display wiring", () => {
  const pane = readFileSync("src/lib/ChatPane.svelte", "utf8");

  it("applies the selected mode to live, finished and paged history entries", () => {
    expect(pane).toContain('.filter((entry) => showConversationMessage(entry.message, chatDetailLevel))');
    expect(pane).toContain('conversation = prependRpcHistory(conversation, page, historyOffset)');
    expect(pane).toContain('historyLimit = conversation.messages.length');
    expect(pane).toContain('{#each visibleEntries as entry, offset (historyOffset + entry.index)}');
    expect(pane).toContain('detail={chatDetailLevel} resolvedToolIds={toolResults}');
    expect(pane).toContain('<MessageFallback {message} detail={chatDetailLevel} />');
  });

  it("hides successful completed tools and output in concise mode, but preserves failure status", () => {
    expect(pane).toContain('{#each runningTools as tool (tool.id)}');
    expect(pane).toContain('{#each finishedLiveTools as tool (tool.id)}');
    expect(pane).toContain('{#if chatDetailLevel === "verbose"}');
    expect(pane).toContain('{:else if tool.isError}');
    expect(pane).toContain('{#if liveOutputTail(tool)}');
    expect(pane).toContain('{#if fileChangeDiffs.get(tool.id)}');
    expect(pane).toContain('role="alert"><Terminal size={14} /> {tool.name} · {t("失败")}');
    expect(pane).toContain('function displayedToolName(tool: RpcTool)');
    expect(pane).toContain('{displayedToolName(tool)}');
  });
});
