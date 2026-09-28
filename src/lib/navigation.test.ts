import { describe, expect, it } from "vitest";
import { matchesSearch, shortcutCommand } from "./navigation";

describe("sidebar search", () => {
  it("matches names, Chinese and ordered subsequences without case sensitivity", () => {
    expect(matchesSearch("Update Runtime", " ur ")).toBe(true);
    expect(matchesSearch("修复模型设置", "模型")).toBe(true);
    expect(matchesSearch("Update Runtime", "zz")).toBe(false);
    expect(matchesSearch("anything", "")).toBe(true);
    expect(matchesSearch("ab", "ba")).toBe(false);
  });
});

describe("host shortcuts", () => {
  const key = (value: string, extra = {}) => ({
    key: value, ctrlKey: true, metaKey: false, altKey: false,
    shiftKey: false, isComposing: false, repeat: false, ...extra,
  });
  it("routes implemented navigation commands", () => {
    expect(shortcutCommand(key("P", { shiftKey: true }))).toBe("tasks");
    expect(shortcutCommand(key("p"))).toBe("files");
    expect(shortcutCommand(key(","))).toBe("settings");
    expect(shortcutCommand(key("b"))).toBe("sidebar");
    expect(shortcutCommand(key("l"))).toBe("composer");
  });
  it("does not capture IME, repeated toggles or unsupported commands", () => {
    expect(shortcutCommand(key("b", { isComposing: true }))).toBeNull();
    expect(shortcutCommand(key("b", { repeat: true }))).toBeNull();
    expect(shortcutCommand(key("b", { altKey: true }))).toBeNull();
    expect(shortcutCommand(key("b", { shiftKey: true }))).toBeNull();
  });
});
