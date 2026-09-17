/**
 * 外观应用（主题包 / 颜色模式 / 语言 / 字体）。
 *
 * 主窗口与「任务看板」浮窗是两个独立的 Webview，各自加载一份页面；
 * 两边共用这一个函数把设置写入 document 根节点的 CSS 变量与 data 属性，
 * 保证两个窗口的主题、语言、字体观感一致。
 */
import { setLocale } from "./i18n.svelte";
import {
  cssAppFontFamily,
  cssCodeFontFamily,
  cssSessionFontFamily,
  isLightColorMode,
  type AppSettings,
} from "./settings";
import { resolveTheme, themeCssVariables } from "./theme";

/** 把设置里的外观项写入 document 根节点；不读取也不修改业务状态。 */
export function applyAppearance(next: AppSettings): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const scheme = isLightColorMode(next.colorMode) ? "light" : "dark";
  root.dataset.colorMode = next.colorMode;
  root.dataset.colorScheme = scheme;
  root.dataset.theme = next.theme;
  setLocale(next.language);
  // 同步 <html lang>：影响无障碍朗读、字体选择与 :lang() 选择器。
  root.lang = next.language;
  // 主题包 → 颜色令牌；字体在主题默认值之上叠加用户自定义（用户设置优先）。
  const theme = resolveTheme(next.theme, next.customThemes ?? []);
  for (const [name, value] of Object.entries(themeCssVariables(theme, scheme))) {
    root.style.setProperty(name, value);
  }
  const appFont = cssAppFontFamily(next.appFontName, theme.typography?.appFont);
  root.style.setProperty("--app-font", appFont);
  root.style.setProperty("--text-font", appFont);
  root.style.setProperty("--session-font", cssSessionFontFamily(next.sessionFontName, theme.typography?.sessionFont));
  root.style.setProperty("--code-font", cssCodeFontFamily(next.codeFont, theme.typography?.codeFont));
  root.style.setProperty("--app-font-size", `${next.appFontSize}px`);
  root.style.setProperty("--session-font-size", `${next.sessionFontSize}px`);
}
