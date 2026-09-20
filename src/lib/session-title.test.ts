import { describe, expect, it } from "vitest";
import { deriveSessionTitle, isTruncatedTitleUpgrade, SESSION_TITLE_LIMIT } from "./session-title";
import type { RpcMessage } from "./rpc-state";

const message = (role: string, content: unknown): RpcMessage => ({ role, content } as RpcMessage);

describe("session title derivation", () => {
  it("uses the first non-empty line of the first user message", () => {
    expect(deriveSessionTitle([
      message("user", "\n\n  修复启动流程  \n后续内容"),
      message("assistant", "好的"),
    ])).toBe("修复启动流程");
  });

  it("keeps long first lines under the limit without cutting the common case", () => {
    const long = "长".repeat(SESSION_TITLE_LIMIT + 50);
    const title = deriveSessionTitle([message("user", long)]);
    expect(title).toBe(`${"长".repeat(SESSION_TITLE_LIMIT)}…`);
    const short = "有些改动没有进最新的release包，输入框下方显示的AI当前执行状态和用时并没有移动";
    expect(deriveSessionTitle([message("user", short)])).toBe(short);
  });

  it("returns null for empty or reply-only sessions", () => {
    expect(deriveSessionTitle([])).toBeNull();
    expect(deriveSessionTitle([message("assistant", "hi")])).toBeNull();
    expect(deriveSessionTitle([message("user", "   ")])).toBeNull();
  });
});

describe("truncated auto title upgrades", () => {
  it("upgrades a legacy ellipsized title when the new title continues it", () => {
    const legacy = "有些改动没有进最新的release包，输入框下方…";
    const next = "有些改动没有进最新的release包，输入框下方显示的AI当前执行状态和用时并没有移动";
    expect(isTruncatedTitleUpgrade(legacy, next)).toBe(true);
    expect(isTruncatedTitleUpgrade(legacy.replace("…", "..."), next)).toBe(true);
  });

  it("does not touch manual titles or unrelated renamed titles", () => {
    expect(isTruncatedTitleUpgrade("手工命名的标题", "新的自动标题")).toBe(false);
    expect(isTruncatedTitleUpgrade("some title...", "something else entirely")).toBe(false);
    expect(isTruncatedTitleUpgrade("截断…", "截断")).toBe(false);
  });
});
