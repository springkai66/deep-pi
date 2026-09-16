/**
 * DeepPi 主题包（Theme Pack）引擎。
 *
 * 主题以 JSON 文件分发（`.deeppi-theme.json`），可导出当前主题、导入第三方主题。
 * 主题定义设计令牌（颜色 + 字体），由 applyThemePack 写入 CSS 变量，
 * 界面样式全部通过这些变量驱动，因此换主题不需要改任何组件。
 */
import { t } from "./i18n.svelte";

export type ThemeColorScheme = "dark" | "light";

/** 与 app.css 中 :root 令牌一一对应的颜色集合。 */
export interface ThemeColors {
  pageBg: string;
  surface: string;
  surfaceAlt: string;
  surfaceRaised: string;
  surfaceHover: string;
  border: string;
  borderStrong: string;
  text: string;
  textStrong: string;
  textMuted: string;
  textSubtle: string;
  accent: string;
  accentInk: string;
  /** 任务状态色：等待 / 运行中 / 失败 / 已完成。 */
  statusWaiting: string;
  statusRunning: string;
  statusFailed: string;
  statusDone: string;
}

export interface ThemeTypography {
  appFont?: string;
  sessionFont?: string;
  codeFont?: string;
  appFontSize?: number;
  sessionFontSize?: number;
}

export interface ThemePack {
  $schema?: string;
  id: string;
  name: string;
  author?: string;
  version?: string;
  description?: string;
  colorScheme: ThemeColorScheme;
  colors: ThemeColors;
  /** 同一主题的浅色变体；缺失时浅色模式复用 colors。 */
  light?: Partial<ThemeColors>;
  typography?: ThemeTypography;
}

export const THEME_SCHEMA_URL = "https://deeppi.dev/schemas/theme.v1.json";

/** 颜色令牌 → CSS 变量名。 */
export const THEME_TOKENS: Record<keyof ThemeColors, string> = {
  pageBg: "--page-bg",
  surface: "--surface",
  surfaceAlt: "--surface-alt",
  surfaceRaised: "--surface-raised",
  surfaceHover: "--surface-hover",
  border: "--border",
  borderStrong: "--border-strong",
  text: "--text",
  textStrong: "--text-strong",
  textMuted: "--text-muted",
  textSubtle: "--text-subtle",
  accent: "--accent",
  accentInk: "--accent-ink",
  statusWaiting: "--status-waiting",
  statusRunning: "--status-running",
  statusFailed: "--status-failed",
  statusDone: "--status-done",
};

export const THEME_COLOR_KEYS = Object.keys(THEME_TOKENS) as Array<keyof ThemeColors>;

/** 方案 A「Command Flow」精密流式主题：深空黑 + 翠绿强调色。 */
export const COMMAND_FLOW_THEME: ThemePack = {
  $schema: THEME_SCHEMA_URL,
  id: "command-flow",
  name: "Command Flow（精密流式）",
  author: "DeepPi",
  version: "1.0.0",
  description: "深空黑底 + 翠绿强调色的精密仪器风格，配等宽代码字体。",
  colorScheme: "dark",
  colors: {
    pageBg: "#0c0e0d",
    surface: "#141815",
    surfaceAlt: "#111613",
    surfaceRaised: "#1a201c",
    surfaceHover: "#202822",
    border: "rgba(255, 255, 255, 0.08)",
    borderStrong: "rgba(255, 255, 255, 0.16)",
    text: "#e6ede8",
    textStrong: "#f4f8f5",
    textMuted: "#829186",
    textSubtle: "#6d7a70",
    accent: "#52e19d",
    accentInk: "#0c0e0d",
    statusWaiting: "#e5c07b",
    statusRunning: "#52e19d",
    statusFailed: "#f87171",
    statusDone: "#60a5fa",
  },
  light: {
    pageBg: "#f6f8f7",
    surface: "#ffffff",
    surfaceAlt: "#eef2f0",
    surfaceRaised: "#e6ece8",
    surfaceHover: "#e2eae5",
    border: "#dfe6e1",
    borderStrong: "#c2cdc6",
    text: "#26302a",
    textStrong: "#141a16",
    textMuted: "#57635b",
    textSubtle: "#67736b",
    accent: "#1f8a58",
    accentInk: "#ffffff",
    statusWaiting: "#a9741a",
    statusRunning: "#1f8a58",
    statusFailed: "#b23b34",
    statusDone: "#2563b8",
  },
  typography: {
    appFont: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Microsoft YaHei", sans-serif`,
    sessionFont: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Microsoft YaHei", sans-serif`,
    codeFont: `"Cascadia Code", "JetBrains Mono", Consolas, monospace`,
    appFontSize: 13,
    sessionFontSize: 13,
  },
};

export const BUILT_IN_THEMES: ThemePack[] = [COMMAND_FLOW_THEME];

export const DEFAULT_THEME_ID = COMMAND_FLOW_THEME.id;

export function findBuiltInTheme(id: string): ThemePack | undefined {
  return BUILT_IN_THEMES.find((theme) => theme.id === id);
}

/** 在「内置 + 已导入」中按 id 查找主题；找不到时回落默认主题。 */
export function resolveTheme(id: string, customThemes: ThemePack[] = []): ThemePack {
  return (
    findBuiltInTheme(id) ??
    customThemes.find((theme) => theme.id === id) ??
    COMMAND_FLOW_THEME
  );
}

/** 解析某配色模式下的最终颜色：浅色模式优先取 light 变体，缺失项回落到主色板。 */
export function resolveThemeColors(theme: ThemePack, scheme: ThemeColorScheme): ThemeColors {
  if (scheme === "light" && theme.light) {
    return { ...theme.colors, ...omitUndefined(theme.light) };
  }
  return theme.colors;
}

function omitUndefined(source: Partial<ThemeColors>): Partial<ThemeColors> {
  const result: Partial<ThemeColors> = {};
  for (const key of THEME_COLOR_KEYS) {
    const value = source[key];
    if (typeof value === "string" && value.trim()) result[key] = value.trim();
  }
  return result;
}

export interface ThemeApplication {
  /** CSS 变量名 → 值。 */
  variables: Record<string, string>;
}

/**
 * 把主题 + 配色模式编译成 CSS 变量表。
 * 字体设置由调用方（applyAppearance）叠加用户自定义后写入，这里只给出主题默认值。
 */
export function themeCssVariables(theme: ThemePack, scheme: ThemeColorScheme): Record<string, string> {
  const colors = resolveThemeColors(theme, scheme);
  const variables: Record<string, string> = {};
  for (const key of THEME_COLOR_KEYS) {
    const value = colors[key];
    if (typeof value === "string" && value.trim()) variables[THEME_TOKENS[key]] = value.trim();
  }
  return variables;
}

/** 允许的 CSS 值字符集：拒绝分号、花括号、反斜杠转义等可能破坏声明的内容。 */
const SAFE_COLOR = /^[#a-zA-Z0-9\s(),.%/'"-]+$/;
const SAFE_ID = /^[a-z0-9][a-z0-9-_]{0,63}$/i;

function isSafeColor(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > 64) return false;
  return SAFE_COLOR.test(trimmed) && !trimmed.includes(";") && !trimmed.includes("{") && !trimmed.includes("}");
}

function isSafeFontStack(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > 240) return false;
  return !/[{;}<>]/.test(trimmed);
}

function clampFontSize(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isFinite(value)) return undefined;
  const rounded = Math.round(value);
  if (rounded < 9 || rounded > 32) return undefined;
  return rounded;
}

export type ThemeParseResult =
  | { ok: true; theme: ThemePack }
  | { ok: false; error: string };

/**
 * 校验并规范化外部主题文件内容。
 * 只接受已知令牌，非法值整包拒绝，避免把不可信 CSS 写进界面。
 */
export function parseThemePack(raw: unknown): ThemeParseResult {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, error: t("主题文件不是一个 JSON 对象") };
  }
  const source = raw as Record<string, unknown>;
  const id = typeof source.id === "string" ? source.id.trim() : "";
  if (!SAFE_ID.test(id)) {
    return { ok: false, error: t("主题 id 缺失或非法（只允许字母、数字、短横线和下划线）") };
  }
  const name = typeof source.name === "string" ? source.name.trim().slice(0, 80) : "";
  if (!name) return { ok: false, error: t("主题缺少 name 字段") };

  const scheme = source.colorScheme === "light" ? "light" : "dark";
  const rawColors = source.colors;
  if (!rawColors || typeof rawColors !== "object" || Array.isArray(rawColors)) {
    return { ok: false, error: t("主题缺少 colors 字段") };
  }
  const colorSource = rawColors as Record<string, unknown>;
  const colors = {} as ThemeColors;
  for (const key of THEME_COLOR_KEYS) {
    const value = colorSource[key];
    if (!isSafeColor(value)) {
      return { ok: false, error: t("颜色 {key} 缺失或不是合法 CSS 颜色值", { key }) };
    }
    colors[key] = value.trim();
  }

  let light: Partial<ThemeColors> | undefined;
  if (source.light && typeof source.light === "object" && !Array.isArray(source.light)) {
    const lightSource = source.light as Record<string, unknown>;
    const parsed: Partial<ThemeColors> = {};
    for (const key of THEME_COLOR_KEYS) {
      if (lightSource[key] === undefined) continue;
      if (!isSafeColor(lightSource[key])) {
        return { ok: false, error: t("浅色变体 {key} 不是合法 CSS 颜色值", { key }) };
      }
      parsed[key] = (lightSource[key] as string).trim();
    }
    if (Object.keys(parsed).length) light = parsed;
  }

  let typography: ThemeTypography | undefined;
  if (source.typography && typeof source.typography === "object" && !Array.isArray(source.typography)) {
    const typeSource = source.typography as Record<string, unknown>;
    const parsed: ThemeTypography = {};
    if (isSafeFontStack(typeSource.appFont)) parsed.appFont = typeSource.appFont;
    if (isSafeFontStack(typeSource.sessionFont)) parsed.sessionFont = typeSource.sessionFont;
    if (isSafeFontStack(typeSource.codeFont)) parsed.codeFont = typeSource.codeFont;
    const appSize = clampFontSize(typeSource.appFontSize);
    if (appSize) parsed.appFontSize = appSize;
    const sessionSize = clampFontSize(typeSource.sessionFontSize);
    if (sessionSize) parsed.sessionFontSize = sessionSize;
    if (Object.keys(parsed).length) typography = parsed;
  }

  const theme: ThemePack = {
    $schema: THEME_SCHEMA_URL,
    id,
    name,
    colorScheme: scheme,
    colors,
  };
  if (typeof source.author === "string" && source.author.trim()) theme.author = source.author.trim().slice(0, 60);
  if (typeof source.version === "string" && source.version.trim()) theme.version = source.version.trim().slice(0, 24);
  if (typeof source.description === "string" && source.description.trim()) {
    theme.description = source.description.trim().slice(0, 200);
  }
  if (light) theme.light = light;
  if (typography) theme.typography = typography;
  return { ok: true, theme };
}

/** 从文件文本解析主题。 */
export function parseThemeFile(text: string): ThemeParseResult {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return { ok: false, error: t("主题文件不是合法 JSON") };
  }
  return parseThemePack(raw);
}

/** 序列化为可导出的主题文件内容（稳定字段顺序，便于人工编辑与 diff）。 */
export function serializeTheme(theme: ThemePack): string {
  const ordered: Record<string, unknown> = {
    $schema: THEME_SCHEMA_URL,
    id: theme.id,
    name: theme.name,
  };
  if (theme.author) ordered.author = theme.author;
  if (theme.version) ordered.version = theme.version;
  if (theme.description) ordered.description = theme.description;
  ordered.colorScheme = theme.colorScheme;
  ordered.typography = theme.typography ?? {};
  ordered.colors = theme.colors;
  if (theme.light) ordered.light = theme.light;
  return `${JSON.stringify(ordered, null, 2)}\n`;
}

/** 导出文件名：`<id>.deeppi-theme.json`。 */
export function themeFileName(theme: ThemePack): string {
  const safe = theme.id.replace(/[^a-z0-9-_]/gi, "");
  return `${safe || "theme"}.deeppi-theme.json`;
}
