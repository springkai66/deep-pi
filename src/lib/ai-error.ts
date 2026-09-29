import { t, tm } from "./i18n.svelte";

export interface AiErrorInfo {
  code: string;
  description: string;
}

export function describeAiError(raw: string): AiErrorInfo {
  const text = tm(raw).trim();
  const prefixed = /^(?:Error:\s*)?(\d{3}):\s*(\{[\s\S]*\})$/.exec(text);
  const json = prefixed?.[2] ?? (text.startsWith("{") ? text : "");
  let data: Record<string, unknown> = {};
  let responseStatus = "";
  if (json) {
    try {
      const parsed: unknown = JSON.parse(json);
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
        const outer = parsed as Record<string, unknown>;
        data = outer.error && typeof outer.error === "object" && !Array.isArray(outer.error)
          ? outer.error as Record<string, unknown> : outer;
        if (typeof outer.status === "number" && Number.isInteger(outer.status) && outer.status >= 100 && outer.status <= 599) responseStatus = String(outer.status);
      }
    } catch { /* Keep the HTTP status even if the body is malformed. */ }
  }
  const safeCode = (value: unknown) => typeof value === "string" && /^[a-z][\w.-]{0,63}$/i.test(value) ? value : "";
  const providerCode = safeCode(data.code) || safeCode(data.error_code);
  const status = prefixed?.[1] || responseStatus || /\b(?:HTTP\s+|status(?:\s+code)?\s*[:=]?\s*)([1-5]\d{2})\b/i.exec(text)?.[1] || /^(?:Error:\s*)?([1-5]\d{2}):/.exec(text)?.[1] || "";
  const details = [data.message, data.type, data.code, data.error_code, text]
    .filter((value): value is string => typeof value === "string").join(" ").toLowerCase();
  const fallbackCode = status ? `HTTP ${status}` : "AI_REQUEST_FAILED";
  const code = providerCode || fallbackCode;
  if (/concurrency\s+limit\s+exceeded/.test(details)) {
    return { code: providerCode || (status ? `HTTP ${status}` : "AI_RATE_LIMIT"), description: t("账户并发请求已达到上限，请稍后重试") };
  }
  if (/quota|insufficient[_\s-]*(?:balance|credits)|usage\s+limit|billing/.test(details)) {
    return { code: providerCode || (status ? `HTTP ${status}` : "AI_QUOTA"), description: t("模型额度不足或已达到使用上限，请检查账户额度") };
  }
  if (status === "429" || /rate[_\s-]*limit|too many requests/i.test(details)) {
    return { code: providerCode || (status ? `HTTP ${status}` : "AI_RATE_LIMIT"), description: t("请求过于频繁，请稍后重试") };
  }
  if (status === "401" || /invalid[_\s-]*(?:api[_\s-]*key|token)|unauthorized|authentication\s+failed|token\s+expired/.test(details)) {
    return { code: providerCode || (status ? `HTTP ${status}` : "AI_AUTH"), description: t("身份验证失败，请检查模型凭证") };
  }
  if (status === "403" || /permission\s+denied|forbidden|access\s+denied/.test(details)) {
    return { code: providerCode || (status ? `HTTP ${status}` : "AI_PERMISSION"), description: t("当前账户无权访问该模型或服务") };
  }
  if (status === "408" || status === "504" || /timed?\s*out|timeout|deadline\s+exceeded/.test(details)) {
    return { code: providerCode || (status ? `HTTP ${status}` : "AI_TIMEOUT"), description: t("模型请求超时，请稍后重试") };
  }
  if (/econnreset|econnrefused|enotfound|fetch failed|network error|websocket error|connection (?:refused|reset|closed)/.test(details)) {
    return { code: providerCode || (status ? `HTTP ${status}` : "AI_NETWORK"), description: t("网络连接失败，请检查网络后重试") };
  }
  if (status.startsWith("5") || /overloaded|service unavailable|internal server error/.test(details)) {
    return { code: providerCode || (status ? `HTTP ${status}` : "AI_SERVICE"), description: t("模型服务暂时不可用，请稍后重试") };
  }
  return { code, description: t("请求失败，请展开详情查看原因") };
}
