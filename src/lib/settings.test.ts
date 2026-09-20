import { describe, expect, it } from "vitest";
import { DEFAULT_APP_SETTINGS, cssCodeFontFamily, cssTerminalFontFamily } from "./settings";

describe("managed workspace defaults", () => {
  it("uses only the DeepPi environment for new tasks", () => {
    expect(DEFAULT_APP_SETTINGS.piEnvironment).toBe("managed");
  });

  it("defaults the global proxy to follow the system environment", () => {
    expect(DEFAULT_APP_SETTINGS.proxyMode).toBe("system");
    expect(DEFAULT_APP_SETTINGS.proxyUrl).toBe("");
    expect(DEFAULT_APP_SETTINGS.proxyNoProxy).toBe("");
  });
});

describe("terminal font stacks", () => {
  it("keeps the native TUI on a monospace fallback stack", () => {
    const stack = cssTerminalFontFamily("", "cascadia");
    expect(stack).toContain("Cascadia Mono");
    expect(stack).toContain("monospace");
  });

  it("keeps an explicit session font first without losing code-font fallback", () => {
    const stack = cssTerminalFontFamily("Microsoft YaHei UI", "jetbrains");
    expect(stack).toMatch(/^"Microsoft YaHei UI", "JetBrains Mono"/);
  });

  it("adds a monospace fallback to a theme code-font stack", () => {
    expect(cssCodeFontFamily("cascadia", "Fira Code")).toContain("Cascadia Mono");
  });
});
