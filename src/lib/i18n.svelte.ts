/**
 * 国际化响应式外壳。
 *
 * 只负责「当前语言」这一份状态；翻译、简繁转换、插值等纯逻辑都在 `i18n-core.ts`，
 * 因此单元测试可以直接覆盖逻辑，不必拉起 runes 运行时。
 *
 * 调用点统一写：`import { t } from "$lib/i18n.svelte";`
 */

import { resolveAppMessage } from "./app-messages";
import { translate } from "./i18n-core";
import { DEFAULT_LOCALE, isLocale, type Locale } from "./locale";

export { DEFAULT_LOCALE, LOCALES, LOCALE_LABELS, isLocale, type Locale } from "./locale";
export {
  englishMessageCount,
  interpolate,
  plural,
  toTraditional,
  traditionalMessage,
  traditionalPairCount,
  translate,
} from "./i18n-core";

/** 当前语言。模块级 `$state`，组件模板与派生读取时会自动建立依赖。 */
let activeLocale = $state<Locale>(DEFAULT_LOCALE);

export function setLocale(next: unknown) {
  activeLocale = isLocale(next) ? next : DEFAULT_LOCALE;
}

export function getLocale(): Locale {
  return activeLocale;
}

/** 取当前语言下的文案；未翻译时回落简体中文原文。 */
export function t(zh: string, params?: Record<string, string | number>): string {
  return translate(activeLocale, zh, params);
}

/** 渲染后端结构化消息（`@msg:` 协议）；非消息文本原样返回。 */
export function tm(raw: string): string {
  return resolveAppMessage(raw, activeLocale);
}
