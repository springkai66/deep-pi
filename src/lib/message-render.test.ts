import { describe, expect, it } from "vitest";
import { render } from "svelte/server";
import MessageMarkdown from "./MessageMarkdown.svelte";
import ChatMessage from "./ChatMessage.svelte";

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
    ] }, onOpenLink: () => {} } }).body;
    expect(assistant).toContain("<summary");
    expect(assistant).toContain("<strong");
    expect(assistant).not.toContain(">check<");
    const user = render(ChatMessage, { props: { message: { role: "user", content: "**literal**" }, onOpenLink: () => {} } }).body;
    expect(user).toContain("**literal**");
    expect(user).not.toContain("<strong");
  });
});
