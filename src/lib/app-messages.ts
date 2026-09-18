/**
 * 后端结构化消息（`@msg:` 协议）的前端渲染。
 *
 * Rust 侧不再拼自然语言，只返回**消息码 + 参数**（见 `src-tauri/src/message.rs`）。
 * 这里把消息码解析成当前语言的文案；三种语言的文案都在本目录的分片文件里，
 * 因此新增语言不需要改后端，后端也不会出现任何面向用户的硬编码文本。
 *
 * 未知消息码会**原样显示 code**（可用 `APP_MESSAGES` 检索），
 * 这样漏配文案时能立刻发现，而不是显示成空白。
 */

import { APP_MESSAGES_CORE } from "./app-messages-core";
import { APP_MESSAGES_FILES } from "./app-messages-files";
import { APP_MESSAGES_EXTRA } from "./app-messages-extra";
import { APP_MESSAGES_GIT } from "./app-messages-git";
import { APP_MESSAGES_RUNTIME } from "./app-messages-runtime";
import { APP_MESSAGES_DIALOGS } from "./app-messages-dialogs";
import { APP_MESSAGES_PROVIDERS } from "./app-messages-providers";
import { APP_MESSAGES_CHECKLIST } from "./app-messages-checklist";
import { interpolate } from "./i18n-core";
import type { Locale } from "./locale";

/** 消息线格式前缀，与 Rust 侧 `message::PREFIX` 保持一致。 */
export const APP_MESSAGE_PREFIX = "@msg:";

/** 一条后端消息在三种语言下的文案，`{name}` 为参数占位符。 */
export interface AppMessageText {
  "zh-CN": string;
  "zh-TW": string;
  en: string;
}

/**
 * 后端消息目录（由分片合并而成）。键是稳定消息码，值是三种语言的文案。
 *
 * 命名约定：`<模块>.<对象>.<情形>`，全小写，用 `.` 分段。
 * 分片按领域拆分，便于并行维护：
 *  - `app-messages-runtime.ts`：运行时与 DSH 服务
 *  - `app-messages-git.ts`：Git 操作
 *  - `app-messages-files.ts`：文件、恢复副本、运行时与扩展
 *  - `app-messages-core.ts`：应用生命周期、设置与其余模块
 *  - `app-messages-dialogs.ts`：原生对话框文案
 */
export const APP_MESSAGES: Record<string, AppMessageText> = {
  ...APP_MESSAGES_CORE,
  ...APP_MESSAGES_RUNTIME,
  ...APP_MESSAGES_EXTRA,
  ...APP_MESSAGES_GIT,
  ...APP_MESSAGES_FILES,
  ...APP_MESSAGES_DIALOGS,
  ...APP_MESSAGES_PROVIDERS,
  ...APP_MESSAGES_CHECKLIST,
};

export interface ParsedAppMessage {
  code: string;
  params: Record<string, string>;
}

/** 判断一个字符串是否是后端结构化消息。 */
export function isAppMessage(raw: string): boolean {
  return typeof raw === "string" && raw.startsWith(APP_MESSAGE_PREFIX);
}

/**
 * 解析 `@msg:<code>?k=v&k=v`。
 *
 * 返回 `null` 表示不是结构化消息（应原样显示，例如普通文本或第三方错误）。
 * 参数值做百分号解码；畸形转义不会抛异常，退化为原样保留。
 */
export function parseAppMessage(raw: string): ParsedAppMessage | null {
  if (!isAppMessage(raw)) return null;
  const rest = raw.slice(APP_MESSAGE_PREFIX.length);
  const queryStart = rest.indexOf("?");
  if (queryStart < 0) return { code: rest, params: {} };

  const code = rest.slice(0, queryStart);
  const params: Record<string, string> = {};
  for (const pair of rest.slice(queryStart + 1).split("&")) {
    if (!pair) continue;
    const equals = pair.indexOf("=");
    if (equals <= 0) continue;
    const key = pair.slice(0, equals);
    const value = pair.slice(equals + 1);
    try {
      params[key] = decodeURIComponent(value);
    } catch {
      params[key] = value;
    }
  }
  return { code, params };
}

/**
 * 把后端消息渲染成指定语言的文案。
 *
 * 非结构化消息原样返回；未知消息码返回 code 本身（便于排查漏配）。
 */
export function resolveAppMessage(raw: string, locale: Locale): string {
  const parsed = parseAppMessage(raw);
  if (!parsed) return raw;
  const entry = APP_MESSAGES[parsed.code];
  if (!entry) return parsed.code;
  const template = entry[locale] ?? entry["zh-CN"];
  return interpolate(template, parsed.params);
}

/**
 * 取某消息码在当前语言下的原始文案（保留 `{name}` 占位符）。
 *
 * 供「文案由前端传给后端」的场景使用（原生对话框）：主题、诊断导出与
 * Git 确认框都属于这一类。未知消息码原样返回 code，便于发现漏配。
 */
export function appMessageText(code: string, locale: Locale): string {
  const entry = APP_MESSAGES[code];
  if (!entry) return code;
  return entry[locale] ?? entry["zh-CN"];
}

/** 传给后端的确认对话框文案；字段名与 Rust `ConfirmDialog` 的 serde 字段一一对应。 */
export interface AppConfirmDialogPayload {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  terms: Record<string, string>;
}

/**
 * 按消息码前缀组装确认对话框文案。
 *
 * 目录里的键是 `<code>.title`、`<code>.message`、`<code>.confirm`、`<code>.cancel`
 * 与 `<code>.term.<name>`；`message` 与词汇模板里的 `{name}` 占位符**原样保留**，
 * 由后端在确认时刻用实测值渲染（见 `src-tauri/src/dialog_text.rs`）。
 */
export function appConfirmDialog(
  code: string,
  locale: Locale,
  terms: readonly string[] = [],
): AppConfirmDialogPayload {
  const entries = terms.map((name) => [name, appMessageText(`${code}.term.${name}`, locale)] as const);
  return {
    title: appMessageText(`${code}.title`, locale),
    message: appMessageText(`${code}.message`, locale),
    confirmLabel: appMessageText(`${code}.confirm`, locale),
    cancelLabel: appMessageText(`${code}.cancel`, locale),
    terms: Object.fromEntries(entries),
  };
}

/** 目录里已登记的消息码，供测试与覆盖率统计使用。 */
export function appMessageCodes(): string[] {
  return Object.keys(APP_MESSAGES);
}
