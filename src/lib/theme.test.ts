import { describe, expect, it } from "vitest";
import {
  BUILT_IN_THEMES,
  COMMAND_FLOW_THEME,
  DEFAULT_THEME_ID,
  DEFAULT_THEME_TRANSPARENCY,
  FROSTED_GLASS_THEME,
  GLASS_THEME_ID,
  clampThemeTransparency,
  parseThemeFile,
  parseThemePack,
  resolveTheme,
  resolveThemeColors,
  serializeTheme,
  themeCssVariables,
  themeFileName,
  themeTransparencyRange,
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

describe("frosted glass built-in theme", () => {
  it("is available as a built-in with light and dark palettes", () => {
    expect(BUILT_IN_THEMES.map((theme) => theme.id)).toContain(GLASS_THEME_ID);
    expect(resolveTheme(GLASS_THEME_ID)).toBe(FROSTED_GLASS_THEME);
    expect(resolveThemeColors(FROSTED_GLASS_THEME, "light").pageBg).toBe(FROSTED_GLASS_THEME.light?.pageBg);
    expect(resolveThemeColors(FROSTED_GLASS_THEME, "dark").pageBg).toBe(FROSTED_GLASS_THEME.colors.pageBg);
  });

  it("limits transparency to 0–100%, defaults to 50%, and only applies it to glass surfaces", () => {
    expect(DEFAULT_THEME_TRANSPARENCY).toEqual({ min: 0, max: 100, default: 50 });
    expect(themeTransparencyRange(FROSTED_GLASS_THEME)).toEqual(DEFAULT_THEME_TRANSPARENCY);
    expect(clampThemeTransparency(-3)).toBe(0);
    expect(clampThemeTransparency(140)).toBe(100);
    expect(clampThemeTransparency(Number.NaN)).toBe(50);
    expect(themeCssVariables(FROSTED_GLASS_THEME, "dark", 20)["--surface"])
      .toBe(`color-mix(in srgb, ${FROSTED_GLASS_THEME.colors.surface} 80%, transparent)`);
    expect(themeCssVariables(FROSTED_GLASS_THEME, "dark", 50)["--theme-transparency"]).toBe("50%");
    expect(themeCssVariables(FROSTED_GLASS_THEME, "dark", 100)["--surface"])
      .toBe(`color-mix(in srgb, ${FROSTED_GLASS_THEME.colors.surface} 0%, transparent)`);
    expect(themeCssVariables(FROSTED_GLASS_THEME, "dark", 0)["--surface"])
      .toBe(FROSTED_GLASS_THEME.colors.surface);
    expect(themeCssVariables(COMMAND_FLOW_THEME, "dark", 80)["--surface"])
      .toBe(COMMAND_FLOW_THEME.colors.surface);
  });

  it("parses and compiles configurable blur, saturation, ambient positions and material", () => {
    const result = parseThemePack(validTheme({ effects: {
      surfaceMaterial: "glass",
      windowMaterial: "acrylic",
      transparency: { min: 10, max: 80, default: 35 },
      backdropBlur: 22,
      backdropSaturation: 1.7,
      ambient: { primaryColor: "#abc", primaryOpacity: 18, primaryX: 27, primaryY: 63,
        secondaryColor: "#def", secondaryOpacity: 9, secondaryX: 76, secondaryY: 81 },
    } }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.theme.effects?.backdropSaturation).toBe(1.7);
    expect(result.theme.effects?.ambient?.primaryX).toBe(27);
    expect(result.theme.effects?.ambient?.secondaryY).toBe(81);
    const variables = themeCssVariables(result.theme, "dark", 45);
    expect(variables["--theme-transparency"]).toBe("45%");
    expect(variables["--theme-backdrop-blur"]).toBe("22px");
    expect(variables["--theme-backdrop-saturation"]).toBe("1.7");
    expect(variables["--theme-glow-primary-x"]).toBe("27%");
    expect(variables["--theme-glow-secondary-y"]).toBe("81%");
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
    expect(parsed.theme.typography?.appFont).toBe(COMMAND_FLOW_THEME.typography?.appFont);
    expect(parsed.theme.typography?.sessionFont).toBe(COMMAND_FLOW_THEME.typography?.sessionFont);
    expect(parsed.theme.typography?.appFontSize).toBe(COMMAND_FLOW_THEME.typography?.appFontSize);
    expect(parsed.theme.typography?.sessionFontSize).toBe(COMMAND_FLOW_THEME.typography?.sessionFontSize);
  });
  it("round-trips the glass effects and typography configuration", () => {
    const parsed = parseThemePack(JSON.parse(serializeTheme(FROSTED_GLASS_THEME)));
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(parsed.theme.effects).toEqual(FROSTED_GLASS_THEME.effects);
    expect(parsed.theme.typography).toEqual(FROSTED_GLASS_THEME.typography);
  });

  it("builds a filesystem-safe export file name", () => {
    expect(themeFileName(COMMAND_FLOW_THEME)).toBe("command-flow.deeppi-theme.json");
    expect(themeFileName({ ...COMMAND_FLOW_THEME, id: "a/../b" })).toBe("ab.deeppi-theme.json");
  });
});
