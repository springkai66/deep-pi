export type ColorMode = "system" | "light" | "dark";
export type UiFont = "system" | "segoe" | "yahei" | "inter";
export type CodeFont = "cascadia" | "consolas" | "jetbrains";
export type CloseBehavior = "ask" | "minimize" | "exit";

export interface AppSettings {
  schemaVersion: 1;
  maxConcurrentTasks: number;
  lastProject: string | null;
  colorMode: ColorMode;
  appFont: UiFont;
  textFont: UiFont;
  codeFont: CodeFont;
  closeBehavior: CloseBehavior;
  skippedUpdates: Record<string, string>;
  snoozedUpdates: Record<string, number>;
}

export const DEFAULT_APP_SETTINGS: AppSettings = {
  schemaVersion: 1,
  maxConcurrentTasks: 3,
  lastProject: null,
  colorMode: "system",
  appFont: "system",
  textFont: "system",
  codeFont: "cascadia",
  closeBehavior: "ask",
  skippedUpdates: {},
  snoozedUpdates: {},
};

export const UI_FONT_OPTIONS: Array<{ value: UiFont; label: string }> = [
  { value: "system", label: "系统默认" },
  { value: "segoe", label: "Segoe UI" },
  { value: "yahei", label: "Microsoft YaHei UI" },
  { value: "inter", label: "Inter" },
];

export const CODE_FONT_OPTIONS: Array<{ value: CodeFont; label: string }> = [
  { value: "cascadia", label: "Cascadia Mono" },
  { value: "consolas", label: "Consolas" },
  { value: "jetbrains", label: "JetBrains Mono" },
];

const FONT_FAMILIES: Record<UiFont | CodeFont, string> = {
  system: "system-ui, \"Segoe UI\", \"Microsoft YaHei UI\", sans-serif",
  segoe: "\"Segoe UI\", sans-serif",
  yahei: "\"Microsoft YaHei UI\", \"Segoe UI\", sans-serif",
  inter: "Inter, \"Segoe UI\", sans-serif",
  cascadia: "\"Cascadia Mono\", Consolas, monospace",
  consolas: "Consolas, \"Cascadia Mono\", monospace",
  jetbrains: "\"JetBrains Mono\", Consolas, monospace",
};

export function cssFontFamily(font: UiFont | CodeFont): string {
  return FONT_FAMILIES[font];
}

export function isLightColorMode(mode: ColorMode): boolean {
  return mode === "light" || (mode === "system" && typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: light)").matches);
}
