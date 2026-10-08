/**
 * 外观应用（主题包 / 颜色模式 / 语言 / 字体）。
 *
 * 主窗口与「任务看板」浮窗是两个独立的 Webview，各自加载一份页面；
 * 两边共用这一个函数把设置写入 document 根节点的 CSS 变量与 data 属性，
 * 保证两个窗口的主题、语言、字体观感一致。
 */
import { setLocale } from "./i18n.svelte";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  cssAppFontFamily,
  cssCodeFontFamily,
  cssSessionFontFamily,
  isLightColorMode,
  type AppSettings,
} from "./settings";
import { resolveTheme, themeCssVariables } from "./theme";
let requestedNativeWindowMaterial: string | undefined;
let appliedNativeWindowMaterial: string | undefined;
let nativeMaterialRequest = 0;

interface NativeMaterialStatus {
  material: "acrylic" | "transparent" | "none";
  fallback: "system_transparency_disabled" | null;
}

/** 把设置里的外观项写入 document 根节点；不读取也不修改业务状态。 */
export function applyAppearance(next: AppSettings): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  const scheme = isLightColorMode(next.colorMode) ? "light" : "dark";
  const theme = resolveTheme(next.theme, next.customThemes ?? []);
  const isMainWindows = isTauri() && getCurrentWindow().label === "main" &&
    typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);
  const windowMaterial = isMainWindows ? theme.effects?.windowMaterial ?? "none" : "none";
  root.dataset.colorMode = next.colorMode;
  root.dataset.colorScheme = scheme;
  root.dataset.theme = next.theme;
  root.dataset.surfaceMaterial = theme.effects?.surfaceMaterial ?? "solid";
  root.dataset.windowMaterial = isMainWindows && windowMaterial === requestedNativeWindowMaterial
    ? appliedNativeWindowMaterial ?? windowMaterial : windowMaterial;
  setLocale(next.language);
  // 同步 <html lang>：影响无障碍朗读、字体选择与 :lang() 选择器。
  root.lang = next.language;
  if (isMainWindows && windowMaterial !== requestedNativeWindowMaterial) {
    requestedNativeWindowMaterial = windowMaterial;
    appliedNativeWindowMaterial = undefined;
    delete root.dataset.nativeMaterialFallback;
    const request = ++nativeMaterialRequest;
    void invoke<NativeMaterialStatus>("set_main_window_material", { material: windowMaterial })
      .then((status) => {
        if (request !== nativeMaterialRequest) return;
        appliedNativeWindowMaterial = status.material;
        root.dataset.windowMaterial = status.material;
        if (status.fallback) root.dataset.nativeMaterialFallback = status.fallback;
      })
      .catch(() => {
        if (request !== nativeMaterialRequest) return;
        requestedNativeWindowMaterial = undefined;
        root.dataset.nativeMaterialFallback = "native_material_failed";
      });
  }
  // 主题包令牌与效果参数；字体设置在主题默认值之上叠加用户自定义（用户设置优先）。
  for (const [name, value] of Object.entries(themeCssVariables(theme, scheme, next.themeTransparency))) {
    root.style.setProperty(name, value);
  }
  const appFont = cssAppFontFamily(next.appFontName, theme.typography?.appFont);
  root.style.setProperty("--app-font", appFont);
  root.style.setProperty("--text-font", appFont);
  root.style.setProperty("--session-font", cssSessionFontFamily(next.sessionFontName, theme.typography?.sessionFont));
  root.style.setProperty("--code-font", cssCodeFontFamily(next.codeFont, theme.typography?.codeFont));
  root.style.setProperty("--app-font-size", `${next.appFontSize ?? theme.typography?.appFontSize ?? 13}px`);
  root.style.setProperty("--session-font-size", `${next.sessionFontSize ?? theme.typography?.sessionFontSize ?? 13}px`);
}
