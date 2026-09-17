/**
 * 官方账号 OAuth 登录（pi SDK 桥）的前端客户端。
 *
 * Rust 侧在登录过程中通过全局事件 `pi-auth-event` 下发授权链接、进度与输入
 * 提示（见 src-tauri/src/pi_auth.rs 与 resources/pi-auth-bridge.mjs）；组件
 * 负责状态机，这里只保留命令调用所需的类型与纯函数，便于无 jsdom 的单测。
 */
import { invoke as nativeInvoke } from "@tauri-apps/api/core";

/** pi 内置支持官方 OAuth 登录的 provider（来自 pi SDK 的运行时枚举）。 */
export interface PiAuthProviderInfo {
  id: string;
  name: string;
  oauth: boolean;
  oauthName: string | null;
  isSubscription: boolean;
}

/** auth.json 中一条凭据的元数据（不含令牌）。 */
export interface PiAuthCredentialSummary {
  provider: string;
  authType: string;
  expires: number | null;
}

/** pi_auth_status 命令的返回。 */
export interface PiAuthStatusResponse {
  credentials: PiAuthCredentialSummary[];
  activeLogin: string | null;
  activeLoginProvider: string | null;
}

/** pi_auth_start_login 的返回（登录会话句柄）。 */
export interface PiAuthLoginAck {
  loginId: string;
  provider: string;
}

export interface PiAuthSelectOption {
  id: string;
  label: string;
  description?: string | null;
}

/** 登录过程中需要用户输入的提示。 */
export interface PiAuthPromptEvent {
  event: "prompt";
  login: string;
  promptId: string;
  kind: "text" | "secret" | "select" | "manual_code";
  message: string;
  placeholder: string | null;
  options: PiAuthSelectOption[] | null;
}

/** 登录过程中的通知（授权链接 / 设备码 / 进度 / 说明）。 */
export interface PiAuthNotifyEvent {
  event: "notify";
  login: string;
  type: "info" | "auth_url" | "device_code" | "progress";
  url?: string;
  instructions?: string;
  message?: string;
  userCode?: string;
  verificationUri?: string;
  intervalSeconds?: number;
  expiresInSeconds?: number;
}

/** 某个提示被解除等待（已提交或流程自行放弃）。 */
export interface PiAuthPromptClosedEvent {
  event: "prompt_closed";
  login: string;
  promptId: string;
}

/** 登录结束（成功或失败）。 */
export interface PiAuthDoneEvent {
  event: "done";
  login: string;
  ok: boolean;
  provider: string;
  credentialType?: string;
  expires?: number | null;
  error?: string;
}

export type PiAuthEvent =
  | PiAuthNotifyEvent
  | PiAuthPromptEvent
  | PiAuthPromptClosedEvent
  | PiAuthDoneEvent;

/** 全局事件名（Rust 侧同名常量）。 */
export const AUTH_EVENT_NAME = "pi-auth-event";

/** `pi auth start_login` 类命令的请求体（serde camelCase）。 */
export interface PiAuthLoginRequest {
  providerId: string;
}

/** `pi auth respond` 的请求体。 */
export interface PiAuthRespondRequest {
  loginId: string;
  promptId: string;
  value: string;
}

/** 令牌有效期的结构化拆分（用于本地化拼接）。 */
export interface ValidityParts {
  days: number;
  hours: number;
  minutes: number;
  expired: boolean;
}

/** OAuth 令牌剩余有效期：无 expiry（API Key）或已过期返回 expired 标记。 */
export function validityParts(
  expires: number | null | undefined,
  now: number = Date.now(),
): ValidityParts {
  if (typeof expires !== "number" || !Number.isFinite(expires) || expires <= now) {
    return { days: 0, hours: 0, minutes: 0, expired: true };
  }
  const remainingMs = expires - now;
  const days = Math.floor(remainingMs / 86_400_000);
  const hours = Math.floor((remainingMs % 86_400_000) / 3_600_000);
  const minutes = Math.floor((remainingMs % 3_600_000) / 60_000);
  return { days, hours, minutes, expired: false };
}

/**
 * 把剩余有效期拼成展示文本；translate 由调用方注入（组件传 i18n 的 t），
 * 保持本模块不依赖 svelte runes，测试可直接传恒等函数观察选择的模板。
 */
export function formatValidityText(
  parts: ValidityParts,
  translate: (key: string, params?: Record<string, string | number>) => string,
): string {
  if (parts.expired) return translate("已登录（令牌已过期，使用时会自动刷新）");
  if (parts.days >= 1) {
    return translate("已登录 · 有效期约 {days} 天 {hours} 小时", {
      days: parts.days,
      hours: parts.hours,
    });
  }
  if (parts.hours >= 1) {
    return translate("已登录 · 有效期约 {hours} 小时 {minutes} 分", {
      hours: parts.hours,
      minutes: parts.minutes,
    });
  }
  if (parts.minutes >= 1) {
    return translate("已登录 · 有效期约 {minutes} 分钟", { minutes: parts.minutes });
  }
  return translate("已登录（令牌即将过期，使用时会自动刷新）");
}

/** 手动回贴输入的预处理：接受完整回调 URL 或裸授权码，去掉首尾空白。 */
export function normalizePastedCode(input: string): string {
  return input.trim();
}
