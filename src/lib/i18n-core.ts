/**
 * 国际化核心逻辑（纯函数，不含响应式状态）。
 *
 * 单独放在普通 `.ts` 模块里，好处：
 *  - 单元测试可以直接覆盖翻译、回落、插值与简繁转换，不需要 runes 运行时；
 *  - 响应式外壳（`i18n.svelte.ts`）只负责「当前语言」这一份状态。
 *
 * ## 设计：以简体中文原文作为消息键
 * 调用点写 `t("设置")` 而不是 `t("settings.title")`：
 * 未翻译时自动回落简体中文原文，不会出现空白或 key 泄漏，译文可以分批补齐。
 */

import { EN_MESSAGES } from "./i18n-en";
import { DEFAULT_LOCALE, type Locale } from "./locale";
import { S2T_PAIRS, ZH_TW_TERMS, ZH_TW_VOCAB } from "./i18n-zh-tw";

/* ------------------------------------------------------------------ *
 * 简 → 繁 转换
 * ------------------------------------------------------------------ */

/** 简→繁 字符查找表；由 S2T_PAIRS 的「简繁」对构建。 */
const S2T_MAP: Map<string, string> = (() => {
  const map = new Map<string, string>();
  for (const pair of S2T_PAIRS) {
    // 每项固定两个字符；异常数据直接跳过，避免把错误带进界面。
    if (pair.length === 2 && pair[0] !== pair[1] && !map.has(pair[0])) {
      map.set(pair[0], pair[1]);
    }
  }
  return map;
})();

/** 逐字符做简→繁转换；映射表没有的字符原样保留。 */
export function toTraditional(text: string): string {
  let result = "";
  for (const character of text) {
    result += S2T_MAP.get(character) ?? character;
  }
  return result;
}

/** 繁体译文：先查整串覆盖，再做字符转换，最后修正两岸软件用语。 */
export function traditionalMessage(zh: string): string {
  const exact = ZH_TW_VOCAB[zh];
  if (exact !== undefined) return exact;
  let text = toTraditional(zh);
  for (const [from, to] of ZH_TW_TERMS) {
    if (text.includes(from)) text = text.split(from).join(to);
  }
  return text;
}

/* ------------------------------------------------------------------ *
 * 翻译
 * ------------------------------------------------------------------ */

/** 把 `{name}` 占位符替换成参数值；缺失的参数保持占位符原样，便于发现漏传。 */
export function interpolate(text: string, params: Record<string, string | number>): string {
  return text.replace(/\{(\w+)\}/g, (match, key: string) =>
    Object.prototype.hasOwnProperty.call(params, key) ? String(params[key]) : match,
  );
}

/**
 * 取指定语言下的文案。
 *
 * @param locale 目标语言
 * @param zh     简体中文原文（同时作为消息键）
 * @param params 可选插值，形如 `{ count: 3 }` 会替换文案里的 `{count}`
 */
export function translate(
  locale: Locale,
  zh: string,
  params?: Record<string, string | number>,
): string {
  let text: string;
  switch (locale) {
    case "en":
      text = EN_MESSAGES[zh] ?? zh;
      break;
    case "zh-TW":
      text = traditionalMessage(zh);
      break;
    default:
      text = zh;
  }
  return params ? interpolate(text, params) : text;
}

/** 按语言与数量在若干候选中挑选（用于「单数/复数」这类差异）。 */
export function plural(locale: Locale, count: number, one: string, many: string): string {
  return translate(locale, count === 1 ? one : many, { count });
}

/** 语言缺失或非法时的兜底语言。 */
export const FALLBACK_LOCALE = DEFAULT_LOCALE;

/** 当前英文目录的条目数，用于覆盖率提示。 */
export function englishMessageCount(): number {
  return Object.keys(EN_MESSAGES).length;
}

/** 当前简繁字符映射条目数，用于覆盖率提示。 */
export function traditionalPairCount(): number {
  return S2T_MAP.size;
}
