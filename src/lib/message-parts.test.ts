import { describe, expect, it } from "vitest";
import { messageParts, messageSource } from "./message-parts";

describe("structured message display", () => {
  it("keeps text, thinking and tool calls in their original order", () => {
    const parts = messageParts([{ type: "thinking", thinking: "检查一下" }, { type: "text", text: "答案" },
      { type: "toolCall", name: "read", arguments: { path: "a.ts" } }]);
    expect(parts.map((part) => part.kind)).toEqual(["thinking", "text", "tool"]);
    expect(parts[2]).toMatchObject({ name: "read", content: '{\n  "path": "a.ts"\n}' });
    expect(messageSource([{ type: "text", text: " a\r\nb " }])).toBe(" a\r\nb ");
  });
  it("does not fetch remote image data or render SVG as an image", () => {
    for (const part of [
      { type: "image", data: "<svg onload=x/>", mimeType: "image/svg+xml" },
      { type: "image", data: "https://tracker.test/pixel", mimeType: "image/png" },
      { type: "image", data: "A".repeat(6_000_000), mimeType: "image/png" },
    ]) expect(messageParts([part])[0]).toMatchObject({ kind: "image", source: null });
    expect(messageParts([{ type: "image", data: "AAAA", mimeType: "image/png" }])[0])
      .toMatchObject({ source: "data:image/png;base64,AAAA" });
  });
  it("retains unsupported content in an explicit part instead of silently dropping it", () => {
    expect(messageParts([{ type: "future", value: "kept" }])[0]).toMatchObject({ kind: "unknown" });
    expect(messageSource([{ type: "future", value: "kept" }])).toContain("kept");
    expect(messageParts("plain")).toEqual([{ kind: "text", content: "plain" }]);
  });
});
