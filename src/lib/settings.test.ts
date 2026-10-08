import { describe, expect, it } from "vitest";
import { CHAT_DETAIL_LEVELS, DEFAULT_APP_SETTINGS, cssCodeFontFamily, cssTerminalFontFamily } from "./settings";
import { DEFAULT_THEME_TRANSPARENCY, TRANSPARENT_THEME, themeTransparencyRange } from "./theme";

describe("managed workspace defaults", () => {
  it("uses only the DeepPi environment for new tasks", () => {
    expect(DEFAULT_APP_SETTINGS.piEnvironment).toBe("managed");
  });

  it("defaults the global proxy to the Windows system setting", () => {
    expect(DEFAULT_APP_SETTINGS.proxyMode).toBe("system");
    expect(DEFAULT_APP_SETTINGS.proxyUrl).toBe("");
    expect(DEFAULT_APP_SETTINGS.proxyNoProxy).toBe("");
  });

  it("keeps the desktop pet hidden in fallback settings", () => {
    expect(DEFAULT_APP_SETTINGS.petEnabled).toBe(false);
  });
});
describe("Pi conversation display choices", () => {
  it("offers only concise and full, defaulting new settings to concise", () => {
    expect(CHAT_DETAIL_LEVELS).toEqual([
      { value: "concise", label: "简洁模式" },
      { value: "verbose", label: "完整模式" },
    ]);
    expect(DEFAULT_APP_SETTINGS.chatDetailLevel).toBe("concise");
  });
});
describe("theme transparency setting", () => {
  it("defaults to 50% and uses the configured theme range", () => {
    expect(DEFAULT_APP_SETTINGS.themeTransparency).toBe(DEFAULT_THEME_TRANSPARENCY.default);
    expect(themeTransparencyRange(TRANSPARENT_THEME)).toEqual(DEFAULT_THEME_TRANSPARENCY);
  });
});
describe("theme typography defaults", () => {
  it("leaves font overrides unset so each theme can provide its typography", () => {
    expect(DEFAULT_APP_SETTINGS.appFontName).toBe("");
    expect(DEFAULT_APP_SETTINGS.sessionFontName).toBe("");
    expect(DEFAULT_APP_SETTINGS.codeFont).toBe("");
    expect(DEFAULT_APP_SETTINGS.appFontSize).toBeNull();
    expect(DEFAULT_APP_SETTINGS.sessionFontSize).toBeNull();
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
