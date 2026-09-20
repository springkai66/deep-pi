import { DEFAULT_LOCALE, type Locale } from "./locale";
import { DEFAULT_THEME_ID, type ThemePack } from "./theme";

export type ColorMode = "system" | "light" | "dark";
/** 主题 id：内置（command-flow）或导入主题的标识。 */
export type Theme = string;
/** 代码字体：空串 = 跟随主题；内置预设 cascadia / consolas / jetbrains；其余为本机字体族名。 */
export type CodeFont = string;
export type CloseBehavior = "ask" | "minimize" | "exit";
export type TerminalShell = "powershell" | "pwsh" | "bash" | "cmd";
/** AI 对话内容的显示详细程度：简洁（只看正文）/ 标准（过程内容折叠）/ 详细（过程内容默认展开）。 */
export type ChatDetailLevel = "concise" | "standard" | "verbose";

export const CHAT_DETAIL_LEVELS: Array<{ value: ChatDetailLevel; label: string }> = [
  { value: "concise", label: "简洁" },
  { value: "standard", label: "标准" },
  { value: "verbose", label: "详细" },
];
export interface ExternalEditor {
  kind: "vscode" | "notepadPlusPlus";
  executable: string;
}

export interface AppSettings {
  schemaVersion: 1;
  maxConcurrentTasks: number;
  lastProject: string | null;
  colorMode: ColorMode;
  theme: Theme;
  /** 应用程序字体（本机字体族名）；空串表示使用默认字体栈。 */
  appFontName: string;
  appFontSize: number;
  /** 会话窗口字体（对话/终端）；空串表示使用默认字体栈。 */
  sessionFontName: string;
  sessionFontSize: number;
  codeFont: CodeFont;
  closeBehavior: CloseBehavior;
  skippedUpdates: Record<string, string>;
  snoozedUpdates: Record<string, number>;
  externalEditor: ExternalEditor | null;
  /** 用户导入的主题包（内置主题之外）。 */
  customThemes: ThemePack[];
  /** 界面语言。 */
  language: Locale;
  /** 新建命令终端使用的 Shell；Pi TUI 终端不受此设置影响。 */
  terminalShell: TerminalShell;
  piEnvironment: "managed";
  /** AI 对话内容的显示详细程度（思考/工具调用等过程内容的展示方式）。 */
  chatDetailLevel: ChatDetailLevel;
}

export const DEFAULT_APP_SETTINGS: AppSettings = {
  schemaVersion: 1,
  maxConcurrentTasks: 3,
  lastProject: null,
  colorMode: "system",
  theme: DEFAULT_THEME_ID,
  appFontName: "",
  appFontSize: 13,
  sessionFontName: "",
  sessionFontSize: 13,
  codeFont: "cascadia",
  closeBehavior: "ask",
  skippedUpdates: {},
  snoozedUpdates: {},
  language: DEFAULT_LOCALE,
  externalEditor: null,
  terminalShell: "powershell",
  piEnvironment: "managed",
  chatDetailLevel: "standard",
  customThemes: [],
};

export const FONT_SIZE_RANGE = { min: 9, max: 32 } as const;

/** 代码字体：设置页选项 = 空串（跟随主题）+ 内置预设 + 本机系统字体。 */
export const CODE_FONT_OPTIONS: Array<{ value: CodeFont; label: string }> = [
  { value: "cascadia", label: "Cascadia Mono" },
  { value: "consolas", label: "Consolas" },
  { value: "jetbrains", label: "JetBrains Mono" },
];

const DEFAULT_APP_STACK = `system-ui, "Segoe UI", "Microsoft YaHei UI", sans-serif`;
const DEFAULT_SESSION_STACK = `system-ui, "Segoe UI", "Microsoft YaHei UI", sans-serif`;
const MONOSPACE_CODE_FALLBACK = `"Cascadia Mono", "Cascadia Code", Consolas, "JetBrains Mono", "Segoe UI Mono", monospace`;
const CODE_FONT_FAMILIES: Record<CodeFont, string> = {
  cascadia: MONOSPACE_CODE_FALLBACK,
  consolas: `Consolas, "Cascadia Mono", "Cascadia Code", "JetBrains Mono", "Segoe UI Mono", monospace`,
  jetbrains: `"JetBrains Mono", "Cascadia Mono", "Cascadia Code", Consolas, "Segoe UI Mono", monospace`,
};

function ensureMonospaceFallback(stack: string): string {
  const normalized = stack.trim();
  if (!normalized) return MONOSPACE_CODE_FALLBACK;
  return /(?:^|,)\s*(?:ui-)?monospace\s*(?:,|$)/i.test(normalized)
    ? normalized
    : `${normalized}, ${MONOSPACE_CODE_FALLBACK}`;
}

function quoteFamily(name: string): string {
  const escaped = name.replace(/["\\]/g, "").trim();
  return escaped ? `"${escaped}"` : "";
}

/// 把用户选择的字体族名转成 CSS font-family 栈；空名回落默认。
export function fontFamilyStack(name: string, fallback: string): string {
  const family = quoteFamily(name);
  return family ? `${family}, ${fallback}` : fallback;
}

/** 应用字体：用户显式指定优先，其次主题字体，最后默认字体栈。 */
export function cssAppFontFamily(name: string, themeFont?: string): string {
  return fontFamilyStack(name, themeFont?.trim() || DEFAULT_APP_STACK);
}

export function cssSessionFontFamily(name: string, themeFont?: string): string {
  return fontFamilyStack(name, themeFont?.trim() || DEFAULT_SESSION_STACK);
}

/** 原生 TUI 与命令终端优先使用等宽代码字体；未显式选择会话字体时不回落到比例字体。 */
export function cssTerminalFontFamily(name: string, codeFont: CodeFont, themeFont?: string): string {
  return name.trim() ? fontFamilyStack(name, cssCodeFontFamily(codeFont, themeFont)) : cssCodeFontFamily(codeFont, themeFont);
}

/** 代码字体：空串跟随主题（主题可覆盖 cascadia 档）；内置预设沿用固定字体栈；
 *  其余值视为本机字体族名，末尾保证等宽回落。 */
export function cssCodeFontFamily(font: CodeFont, themeFont?: string): string {
  const chosen = font.trim();
  if (!chosen || chosen === "cascadia") return ensureMonospaceFallback(themeFont?.trim() || CODE_FONT_FAMILIES.cascadia);
  if (chosen === "consolas" || chosen === "jetbrains") return CODE_FONT_FAMILIES[chosen];
  return fontFamilyStack(chosen, MONOSPACE_CODE_FALLBACK);
}

/// 字体预览栈：指定族名 + 应用默认回落（应用程序/会话字体选项预览用）。
export function appFontPreviewStack(name: string): string {
  return fontFamilyStack(name, DEFAULT_APP_STACK);
}

/// 字体预览栈：指定族名 + 等宽回落（代码字体选项预览用）。
export function codeFontPreviewStack(name: string): string {
  return fontFamilyStack(name, MONOSPACE_CODE_FALLBACK);
}

export function isLightColorMode(mode: ColorMode): boolean {
  return mode === "light" || (mode === "system" && typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: light)").matches);
}
