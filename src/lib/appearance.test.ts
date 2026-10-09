import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import { applyAppearance } from "./appearance";
import { DEFAULT_APP_SETTINGS, cssAppFontFamily, cssCodeFontFamily, cssSessionFontFamily } from "./settings";
import { COMMAND_FLOW_THEME, TRANSPARENT_THEME, themeCssVariables } from "./theme";

function appearanceRoot() {
  const values: Record<string, string> = {};
  const root = {
    dataset: {} as Record<string, string>,
    lang: "",
    style: { setProperty: (name: string, value: string) => { values[name] = value; } },
  };
  return { root, values };
}

describe("theme appearance application", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("applies the chosen transparency and color scheme to root tokens", () => {
    const { root, values } = appearanceRoot();
    vi.stubGlobal("document", { documentElement: root });

    applyAppearance({
      ...DEFAULT_APP_SETTINGS,
      theme: TRANSPARENT_THEME.id,
      colorMode: "light",
      themeTransparency: 35,
    });

    expect(root.dataset.theme).toBe(TRANSPARENT_THEME.id);
    expect(root.dataset.surfaceMaterial).toBe("glass");
    expect(root.dataset.windowMaterial).toBe("none");
    expect(root.dataset.colorScheme).toBe("light");
    expect(values["--theme-transparency"]).toBe("35%");
    expect(values["--surface"]).toBe(themeCssVariables(TRANSPARENT_THEME, "light", 35)["--surface"]);
  });

  it("uses the operating-system light preference when color mode is system", () => {
    const { root, values } = appearanceRoot();
    vi.stubGlobal("window", { matchMedia: () => ({ matches: true }) });
    vi.stubGlobal("document", { documentElement: root });

    applyAppearance({ ...DEFAULT_APP_SETTINGS, theme: TRANSPARENT_THEME.id, colorMode: "system" });

    expect(root.dataset.colorMode).toBe("system");
    expect(root.dataset.colorScheme).toBe("light");
    expect(values["--surface"]).toBe(themeCssVariables(TRANSPARENT_THEME, "light", DEFAULT_APP_SETTINGS.themeTransparency)["--surface"]);
  });

  it("switches app, session and code fonts with the theme unless explicitly overridden", () => {
    const { root, values } = appearanceRoot();
    vi.stubGlobal("document", { documentElement: root });

    applyAppearance(DEFAULT_APP_SETTINGS);
    const commandFlowFonts = {
      app: values["--app-font"],
      session: values["--session-font"],
      code: values["--code-font"],
    };
    applyAppearance({ ...DEFAULT_APP_SETTINGS, theme: TRANSPARENT_THEME.id });

    expect(values["--app-font"]).toBe(cssAppFontFamily("", TRANSPARENT_THEME.typography?.appFont));
    expect(values["--session-font"]).toBe(cssSessionFontFamily("", TRANSPARENT_THEME.typography?.sessionFont));
    expect(values["--code-font"]).toBe(cssCodeFontFamily("", TRANSPARENT_THEME.typography?.codeFont));
    expect(values["--app-font"]).not.toBe(commandFlowFonts.app);
    expect(values["--session-font"]).not.toBe(commandFlowFonts.session);
    expect(values["--code-font"]).not.toBe(commandFlowFonts.code);

    applyAppearance({
      ...DEFAULT_APP_SETTINGS,
      theme: TRANSPARENT_THEME.id,
      appFontName: "Arial",
      sessionFontName: "Georgia",
      codeFont: "consolas",
    });
    expect(values["--app-font"]).toContain('"Arial"');
    expect(values["--session-font"]).toContain('"Georgia"');
    expect(values["--code-font"]).toContain("Consolas");
  });

  it("applies imported theme font stacks and sizes as defaults", () => {
    const { root, values } = appearanceRoot();
    vi.stubGlobal("document", { documentElement: root });
    const customTheme = {
      ...COMMAND_FLOW_THEME,
      id: "typography-test",
      typography: {
        appFont: '"Inter", sans-serif',
        sessionFont: '"Atkinson Hyperlegible", sans-serif',
        codeFont: '"Fira Code", monospace',
        appFontSize: 15,
        sessionFontSize: 17,
      },
    };

    applyAppearance({
      ...DEFAULT_APP_SETTINGS,
      theme: customTheme.id,
      customThemes: [customTheme],
    });

    expect(values["--app-font"]).toBe(customTheme.typography.appFont);
    expect(values["--session-font"]).toBe(customTheme.typography.sessionFont);
    expect(values["--code-font"]).toBe(customTheme.typography.codeFont);
    expect(values["--app-font-size"]).toBe("15px");
    expect(values["--session-font-size"]).toBe("17px");

    applyAppearance({
      ...DEFAULT_APP_SETTINGS,
      theme: customTheme.id,
      customThemes: [customTheme],
      appFontSize: 18,
      sessionFontSize: 19,
    });
    expect(values["--app-font-size"]).toBe("18px");
    expect(values["--session-font-size"]).toBe("19px");
  });

  it("keeps settings surroundings clear and conversation surfaces translucent and legible", () => {
    const appCss = readFileSync(new URL("../app.css", import.meta.url), "utf8");
    const pageStyles = readFileSync(new URL("../routes/+page.svelte", import.meta.url), "utf8");
    expect(pageStyles).toMatch(/\.settings-overlay\s*\{[^}]*background:\s*transparent;/s);
    expect(appCss).toMatch(/:root\[data-surface-material="glass"\]\.*/);
    expect(appCss).toMatch(/:root\[data-window-material="transparent"\]\s+body\s*\{\s*background:\s*transparent\s*!important;/);
    expect(appCss).toMatch(/:root\[data-surface-material="glass"\] \.settings-overlay\s*\{[^}]*background:\s*transparent/s);
    expect(appCss).toMatch(/:root\[data-surface-material="glass"\] \.dialog-backdrop[\s\S]*?--theme-transparency/);
    expect(appCss).toMatch(/:root\[data-surface-material="glass"\] \.app-dialog\s*\{[^}]*background:\s*var\(--surface\)/s);
    expect(appCss).toMatch(/:root\[data-surface-material="glass"\] \.chat-pane\s*\{[^}]*background:\s*var\(--surface\)/s);
    expect(appCss).toMatch(/:root\[data-surface-material="glass"\] \.chat-pane \.message-content\s*\{[^}]*color:\s*var\(--text-strong\)[^}]*font-weight:\s*500/s);
    expect(appCss).toContain("--theme-reading-surface-opacity");
    expect(appCss).toMatch(/\.chat-pane article:not\(\.user-message\):not\(\.tool-message\) \.message-content\s*\{[^}]*background:\s*var\(--surface\)/s);
    expect(appCss).toMatch(/\.app-shell\s*\{[^}]*text-shadow:/s);
    expect(appCss).not.toContain(':root[data-theme="frosted-glass"]');
    expect(appCss).not.toContain("--glass-transparency");
  });

  it("restores opaque palette values when switching back to a regular theme", () => {
    const { root, values } = appearanceRoot();
    vi.stubGlobal("document", { documentElement: root });

    applyAppearance({ ...DEFAULT_APP_SETTINGS, theme: TRANSPARENT_THEME.id, themeTransparency: 45 });
    applyAppearance(DEFAULT_APP_SETTINGS);

    expect(root.dataset.surfaceMaterial).toBe("solid");
    expect(values["--surface"]).toBe(themeCssVariables(COMMAND_FLOW_THEME, "dark")["--surface"]);
  });
});
