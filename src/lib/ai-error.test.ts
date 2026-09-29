import { describe, expect, it } from "vitest";
import { render } from "svelte/server";
import AiError from "./AiError.svelte";
import { describeAiError } from "./ai-error";
import { setLocale } from "./i18n.svelte";

describe("AI conversation error presentation", () => {
  it("prefers a provider code to an HTTP status, then uses the HTTP status", () => {
    expect(describeAiError('429: {"error":{"code":"RATE_LIMITED","message":"rate limit exceeded"}}'))
      .toEqual({ code: "RATE_LIMITED", description: "请求过于频繁，请稍后重试" });
    expect(describeAiError('401: {"message":"invalid token"}'))
      .toEqual({ code: "HTTP 401", description: "身份验证失败，请检查模型凭证" });
    expect(describeAiError("http status: 503")).toEqual({ code: "HTTP 503", description: "模型服务暂时不可用，请稍后重试" });
    expect(describeAiError('Error: 429: {"code":"RATE_LIMITED","message":"private request detail"}'))
      .toEqual({ code: "RATE_LIMITED", description: "请求过于频繁，请稍后重试" });
    expect(describeAiError('{"status":403,"error":{"message":"forbidden"}}'))
      .toEqual({ code: "HTTP 403", description: "当前账户无权访问该模型或服务" });
  });

  it.each([
    ["quota exceeded", "AI_QUOTA", "模型额度不足或已达到使用上限，请检查账户额度"],
    ["permission denied", "AI_PERMISSION", "当前账户无权访问该模型或服务"],
    ["rate limit exceeded", "AI_RATE_LIMIT", "请求过于频繁，请稍后重试"],
    ["request timed out", "AI_TIMEOUT", "模型请求超时，请稍后重试"],
    ["connection refused", "AI_NETWORK", "网络连接失败，请检查网络后重试"],
    ["service unavailable", "AI_SERVICE", "模型服务暂时不可用，请稍后重试"],
    ["unrecognized private detail", "AI_REQUEST_FAILED", "请求失败，请展开详情查看原因"],
  ])("classifies %s without showing provider prose", (raw, code, description) => {
    expect(describeAiError(raw)).toEqual({ code, description });
  });

  it("does not put an untrusted code or raw text into the visible summary", () => {
    const raw = '429: {"code":"secret value <script>","message":"private token <script>"}';
    const body = render(AiError, { props: { raw } }).body;
    expect(body).toContain("HTTP 429 · 请求过于频繁，请稍后重试");
    expect(body).toMatch(/<details[^>]*>\s*<summary[^>]*>错误详情<\/summary>\s*<pre[^>]*>/);
    expect(body).not.toContain("<details open");
    expect(body).toContain("&lt;script>");
    expect(body).not.toContain("<script>");
    expect(body.slice(0, body.indexOf("<details"))).not.toContain("private token");
  });

  it("translates the brief description and detail label without translating the raw error", () => {
    setLocale("en");
    try {
      const body = render(AiError, { props: { raw: "connection refused with account-private detail" } }).body;
      expect(body).toContain("AI_NETWORK · Network connection failed. Check your connection and retry.");
      expect(body).toMatch(/<summary[^>]*>Error details<\/summary>/);
      expect(body).toContain("connection refused with account-private detail");
    } finally { setLocale("zh-CN"); }
  });
});
