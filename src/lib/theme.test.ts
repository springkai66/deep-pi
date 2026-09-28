import { describe, expect, it } from "vitest";
import {
  COMMAND_FLOW_THEME,
  DEFAULT_THEME_ID,
  parseThemeFile,
  parseThemePack,
  resolveTheme,
  resolveThemeColors,
  serializeTheme,
  themeCssVariables,
  themeFileName,
  type ThemePack,
} from "./theme";

function validTheme(overrides: Record<string, unknown> = {}) {
  return {
    id: "my-theme",
    name: "我的主题",
    colorScheme: "dark",
    colors: { ...COMMAND_FLOW_THEME.colors },
    ...overrides,
  };
}

describe("theme pack parsing", () => {
  it("accepts a well-formed theme and keeps every token", () => {
    const result = parseThemePack(validTheme());
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.theme.id).toBe("my-theme");
    expect(result.theme.name).toBe("我的主题");
    expect(result.theme.colors.accent).toBe(COMMAND_FLOW_THEME.colors.accent);
  });

  it("rejects an id that could not be used as a stable identifier", () => {
    for (const id of ["", "has space", "bad/slash", "-leading-dash", "x".repeat(80), 12]) {
      const result = parseThemePack(validTheme({ id }));
      expect(result.ok, `id=${String(id)}`).toBe(false);
    }
  });

  it("rejects a theme without a name", () => {
    expect(parseThemePack(validTheme({ name: "  " })).ok).toBe(false);
  });

  it("rejects a theme that misses any required colour token", () => {
    const colors = { ...COMMAND_FLOW_THEME.colors } as Record<string, string>;
    delete colors.accent;
    expect(parseThemePack(validTheme({ colors })).ok).toBe(false);
  });

  it("rejects colour values that try to escape the declaration", () => {
    for (const accent of ["#fff; } body { display: none }", "red}/**/", "url(http://evil)", "a".repeat(80)]) {
      const result = parseThemePack(validTheme({ colors: { ...COMMAND_FLOW_THEME.colors, accent } }));
      expect(result.ok, `accent=${accent}`).toBe(false);
    }
  });

  it("drops unsafe typography entries instead of failing the whole pack", () => {
    const result = parseThemePack(
      validTheme({ typography: { appFont: "Inter, sans-serif", codeFont: "bad{font}", appFontSize: 999 } }),
    );
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.theme.typography?.appFont).toBe("Inter, sans-serif");
    expect(result.theme.typography?.codeFont).toBeUndefined();
    expect(result.theme.typography?.appFontSize).toBeUndefined();
  });

  it("reports a readable error for malformed JSON", () => {
    const result = parseThemeFile("{ not json");
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("JSON");
  });
});

describe("theme resolution", () => {
  it("prefers built-in themes over imported ones with the same id", () => {
    const impostor: ThemePack = { ...COMMAND_FLOW_THEME, name: "冒名主题" };
    expect(resolveTheme(DEFAULT_THEME_ID, [impostor]).name).toBe(COMMAND_FLOW_THEME.name);
  });

  it("falls back to the default theme for an unknown id", () => {
    expect(resolveTheme("does-not-exist", []).id).toBe(DEFAULT_THEME_ID);
  });

  it("falls back to the default theme for the removed legacy win11 id", () => {
    expect(resolveTheme("win11", [])).toBe(COMMAND_FLOW_THEME);
  });

  it("merges the light variant over the base palette", () => {
    const colors = resolveThemeColors(COMMAND_FLOW_THEME, "light");
    expect(colors.pageBg).toBe(COMMAND_FLOW_THEME.light?.pageBg);
    expect(colors.accent).toBe(COMMAND_FLOW_THEME.light?.accent);
  });

  it("reuses the base palette when a theme has no light variant", () => {
    const theme: ThemePack = { ...COMMAND_FLOW_THEME, light: undefined };
    expect(resolveThemeColors(theme, "light")).toBe(theme.colors);
  });
});

describe("theme css variables", () => {
  it("maps every token to its custom property", () => {
    const variables = themeCssVariables(COMMAND_FLOW_THEME, "dark");
    expect(variables["--page-bg"]).toBe(COMMAND_FLOW_THEME.colors.pageBg);
    expect(variables["--accent"]).toBe(COMMAND_FLOW_THEME.colors.accent);
    expect(variables["--accent-ink"]).toBe(COMMAND_FLOW_THEME.colors.accentInk);
    expect(Object.keys(variables)).toHaveLength(17);
  });
});

describe("theme serialization", () => {
  it("round-trips through serialize and parse", () => {
    const text = serializeTheme(COMMAND_FLOW_THEME);
    const parsed = parseThemePack(JSON.parse(text));
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(parsed.theme.colors).toEqual(COMMAND_FLOW_THEME.colors);
    expect(parsed.theme.typography?.codeFont).toBe(COMMAND_FLOW_THEME.typography?.codeFont);
  });

  it("builds a filesystem-safe export file name", () => {
    expect(themeFileName(COMMAND_FLOW_THEME)).toBe("command-flow.deeppi-theme.json");
    expect(themeFileName({ ...COMMAND_FLOW_THEME, id: "a/../b" })).toBe("ab.deeppi-theme.json");
  });
});
