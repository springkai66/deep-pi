import { describe, expect, it } from "vitest";
import { render } from "svelte/server";
import MessageMarkdown from "./MessageMarkdown.svelte";
import ChatMessage from "./ChatMessage.svelte";
import MessageFallback from "./MessageFallback.svelte";

describe("message components server rendering", () => {
  it("renders nested code controls and table markup without executing message HTML", () => {
    const { body } = render(MessageMarkdown, { props: { content:
      '# Hello\n\n<img src=x onerror=evil>\n\n- nested\n\n  ```js\n  <script>evil</script>\n  ```\n\n| A | B |\n| - | - |\n| 1 | 2 |',
      onOpenLink: () => {},
    } });
    expect(body).toContain("<h1");
    expect(body).toContain("<table");
    expect(body).toContain('aria-label="复制代码"');
    expect(body).toContain("&lt;script>");
    expect(body).not.toContain("<script>");
    expect(body).not.toContain("<img");
  });
  it("renders assistant thinking separately and keeps user text literal", () => {
    const assistant = render(ChatMessage, { props: { message: { role: "assistant", content: [
      { type: "thinking", thinking: "check" }, { type: "text", text: "**answer**" },
    ] }, detail: "verbose", onOpenLink: () => {} } }).body;
    expect(assistant).toContain("<summary");
    expect(assistant).toContain("<strong");
    expect(assistant).toContain(">check<");
    const user = render(ChatMessage, { props: { message: { role: "user", content: "**literal**" }, onOpenLink: () => {} } }).body;
    expect(user).toContain("**literal**");
    expect(user).not.toContain("<strong");
  });
});

describe("Pi conversation display modes", () => {
  const content = [
    { type: "thinking", thinking: "private reasoning" },
    { type: "toolCall", id: "tool", name: "read", arguments: { path: "secret.txt" } },
    { type: "text", text: "Visible answer" },
    { type: "image", mimeType: "image/png", data: "aGVsbG8=" },
    { type: "future", value: "internal detail" },
  ];
  const show = (detail: "concise" | "verbose") => render(ChatMessage, { props: {
    message: { role: "assistant", content }, detail, onOpenLink: () => {},
  } }).body;

  it("keeps answers and images visible without process or raw-source access in concise mode", () => {
    const body = show("concise");
    expect(body).toContain("Visible answer");
    expect(body).toContain('alt="会话图片"');
    expect(body).not.toContain("private reasoning");
    expect(body).not.toContain("secret.txt");
    expect(body).not.toContain("internal detail");
    expect(body).not.toContain('aria-label="原始文本"');
  });

  it("shows process and raw-source access in full mode", () => {
    const body = show("verbose");
    expect(body).toContain("Visible answer");
    expect(body).toContain("private reasoning");
    expect(body).toContain("secret.txt");
    expect(body).toContain("internal detail");
    expect(body).toContain('aria-label="原始文本"');
  });

  it("keeps invalid images visibly reported in concise mode", () => {
    const body = render(ChatMessage, { props: { message: { role: "user", content: [
      { type: "image", mimeType: "image/png", data: "invalid" },
    ] }, detail: "concise", onOpenLink: () => {} } }).body;
    expect(body).toContain("图片类型或大小不受支持。");
  });
  it("does not offer an empty-text copy action for image-only concise messages", () => {
    const body = render(ChatMessage, { props: { message: { role: "assistant", content: [
      { type: "image", mimeType: "image/png", data: "aGVsbG8=" },
    ] }, detail: "concise", onOpenLink: () => {} } }).body;
    expect(body).toContain('alt="会话图片"');
    expect(body).toMatch(/<button[^>]*disabled[^>]*aria-label="复制消息"|<button[^>]*aria-label="复制消息"[^>]*disabled/);
  });

  it("shows an assistant error restored from history without a live banner", () => {
    const body = render(ChatMessage, { props: {
      message: { role: "assistant", content: [], errorMessage: "Request failed" },
      detail: "concise", onOpenLink: () => {},
    } }).body;
    expect(body).toContain("Request failed");
  });

  it("shows a restored tool-result error in full mode even without output text", () => {
    const body = render(ChatMessage, { props: {
      message: { role: "toolResult", content: [], errorMessage: "Tool failed" },
      detail: "verbose", onOpenLink: () => {},
    } }).body;
    expect(body).toContain("Tool failed");
  });

  it("keeps images and assistant errors visible while the message renderer loads or fails", () => {
    const message = { role: "assistant", content: [
      { type: "thinking", thinking: "private reasoning" },
      { type: "image", mimeType: "image/png", data: "aGVsbG8=" },
      { type: "toolCall", name: "read", arguments: { path: "secret.txt" } },
    ], errorMessage: "Request failed" };
    const concise = render(MessageFallback, { props: { message, detail: "concise" } }).body;
    expect(concise).toContain('alt="会话图片"');
    expect(concise).toContain("Request failed");
    expect(concise).not.toContain("private reasoning");
    expect(concise).not.toContain("secret.txt");
    const verbose = render(MessageFallback, { props: { message, detail: "verbose" } }).body;
    expect(verbose).toContain("private reasoning");
    expect(verbose).toContain("secret.txt");
  });
});
