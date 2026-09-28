/**
 * 语言标识与展示名。
 *
 * 单独放在纯 `.ts` 模块里（而不是 `.svelte.ts`），这样 `settings.ts`、
 * 单元测试等非 Svelte 上下文也能安全引用语言类型，不必拉起 runes 运行时。
 */

export const LOCALES = ["zh-CN", "zh-TW", "en"] as const;
export type Locale = (typeof LOCALES)[number];

/** 语言选择器里显示的名字——用该语言自己的写法，切换前也能看懂。 */
export const LOCALE_LABELS: Record<Locale, string> = {
  "zh-CN": "简体中文",
  "zh-TW": "繁體中文",
  en: "English",
};

/** 语言的英文名，用于设置项说明等需要非本地化标识的场景。 */
export const LOCALE_ENGLISH_LABELS: Record<Locale, string> = {
  "zh-CN": "Simplified Chinese",
  "zh-TW": "Traditional Chinese",
  en: "English",
};

export const DEFAULT_LOCALE: Locale = "zh-CN";

export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && (LOCALES as readonly string[]).includes(value);
}
