import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const pane = readFileSync(new URL("./ChatPane.svelte", import.meta.url), "utf8");
const message = readFileSync(new URL("./ChatMessage.svelte", import.meta.url), "utf8");

describe("user transcript layout", () => {
  it("keeps user messages right aligned and distinct at narrow widths", () => {
    expect(pane).toContain('class:user-message={message.role === "user"}');
    expect(pane).toContain(".user-message { width: fit-content; max-width: min(78%, 700px); margin-right: 0; margin-left: auto; min-width: 0; }");
    expect(pane).toContain(".user-message .message-label { justify-content: flex-end; }");
    expect(pane).toContain("background: color-mix(in srgb, var(--accent) 12%, var(--surface))");
    expect(pane).toContain("@media (max-width: 560px) { .user-message { max-width: 92%; } }");
    expect(message).toContain(".user-text, .source { white-space: pre-wrap; overflow-wrap: anywhere;");
    expect(message).toContain("img { display: block; max-width: 100%;");
    expect(message).toContain('title={copied ? t("已复制") : t("复制消息")}');
  });
});
