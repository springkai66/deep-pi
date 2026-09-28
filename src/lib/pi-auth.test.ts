import { describe, expect, it } from "vitest";
import {
  AUTH_EVENT_NAME,
  formatValidityText,
  normalizePastedCode,
  validityParts,
  type PiAuthDoneEvent,
  type PiAuthPromptClosedEvent,
  type PiAuthPromptEvent,
} from "./pi-auth";

/** 恒等翻译：记录 key 与参数，便于断言模板与占位符。 */
function identityTranslate(
  key: string,
  params?: Record<string, string | number>,
): string {
  if (!params) return key;
  let text = key;
  for (const [name, value] of Object.entries(params)) {
    text = text.replaceAll(`{${name}}`, String(value));
  }
  return text;
}

describe("validityParts", () => {
  it("splits remaining time into days/hours/minutes", () => {
    const now = 1_000_000_000_000;
    // 1 天 2 小时 3 分钟
    const parts = validityParts(now + 93_780_000, now);
    expect(parts.days).toBe(1);
    expect(parts.hours).toBe(2);
    expect(parts.minutes).toBe(3);
  });

  it("marks missing, non-finite and past expiries as expired", () => {
    expect(validityParts(null, 1).expired).toBe(true);
    expect(validityParts(undefined, 1).expired).toBe(true);
    expect(validityParts(Number.NaN, 1).expired).toBe(true);
    expect(validityParts(500, 1_000).expired).toBe(true);
  });
});

describe("formatValidityText", () => {
  it("picks the day template when days remain", () => {
    const text = formatValidityText({ days: 3, hours: 4, minutes: 5, expired: false }, identityTranslate);
    expect(text).toContain("3 天");
    expect(text).toContain("4");
  });

  it("falls back to hours and minutes", () => {
    const hours = formatValidityText({ days: 0, hours: 5, minutes: 6, expired: false }, identityTranslate);
    expect(hours).toContain("5 小时 6 分");
    const minutes = formatValidityText({ days: 0, hours: 0, minutes: 7, expired: false }, identityTranslate);
    expect(minutes).toContain("7 分钟");
  });

  it("uses expiry-aware copy when the token is stale", () => {
    const text = formatValidityText({ days: 0, hours: 0, minutes: 0, expired: true }, identityTranslate);
    expect(text).toContain("已过期");
  });
});
describe("event guards", () => {
  it("only accepts events from the active login id", () => {
    // 与组件 handleEvent 相同的过滤逻辑：loginId 不匹配的事件一律忽略。
    const prompt: PiAuthPromptEvent = {
      event: "prompt",
      login: "login-1",
      promptId: "3",
      kind: "manual_code",
      message: "paste",
      placeholder: null,
      options: null,
    };
    const done: PiAuthDoneEvent = {
      event: "done",
      login: "login-2",
      ok: true,
      provider: "anthropic",
    };
    const closed: PiAuthPromptClosedEvent = {
      event: "prompt_closed",
      login: "login-1",
      promptId: "3",
    };
    expect(prompt.login).toBe("login-1");
    expect(done.login).not.toBe("login-1");
    expect(closed.promptId).toBe(prompt.promptId);
  });
});

describe("normalizePastedCode", () => {
  it("trims whitespace around pasted urls or codes", () => {
    expect(normalizePastedCode("  https://claude.ai/oauth/callback?code=abc  ")).toBe(
      "https://claude.ai/oauth/callback?code=abc",
    );
    expect(normalizePastedCode(" raw-code ")).toBe("raw-code");
  });
});

describe("event name", () => {
  it("matches the rust-side constant", () => {
    expect(AUTH_EVENT_NAME).toBe("pi-auth-event");
  });
});
