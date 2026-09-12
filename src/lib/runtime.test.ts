import { describe, expect, it } from "vitest";
import { SNOOZE_DURATION_MS, updateActionsVisible, updateSuppressed, type RuntimeUpdate } from "./runtime";

const update = (patch: Partial<RuntimeUpdate> = {}): RuntimeUpdate => ({
  id: "pi", name: "Pi", currentVersion: "0.84.4", latestVersion: "0.85.1",
  updateAvailable: true, installable: true, canRollback: true, stale: false, error: null, note: null,
  ...patch,
});

describe("runtime update suppression", () => {
  it("suppresses only the exact skipped version, so a newer release re-surfaces", () => {
    const skipped = { pi: "0.85.1" };
    expect(updateSuppressed(update(), "pi", skipped, {})).toEqual({ skipped: true, snoozed: false, suppressed: true });
    // 上游发布更新的版本后必须重新提示，否则用户会被永久静音。
    expect(updateSuppressed(update({ latestVersion: "0.86.0" }), "pi", skipped, {}).skipped).toBe(false);
    // 跳到的是别的组件时不影响本组件。
    expect(updateSuppressed(update(), "dsh", skipped, {}).skipped).toBe(false);
  });

  it("treats snooze as time-bounded and expires it", () => {
    const now = 1_000_000;
    const snoozed = { pi: now + SNOOZE_DURATION_MS };
    expect(updateSuppressed(update(), "pi", {}, snoozed, now).snoozed).toBe(true);
    expect(updateSuppressed(update(), "pi", {}, snoozed, now + SNOOZE_DURATION_MS + 1).snoozed).toBe(false);
  });

  it("does not suppress anything without a known latest version", () => {
    const unknown = update({ latestVersion: null, updateAvailable: false });
    expect(updateSuppressed(unknown, "pi", { pi: "" }, { pi: 9e15 })).toEqual({ skipped: false, snoozed: false, suppressed: false });
    expect(updateSuppressed(undefined, "pi", { pi: "" }, { pi: 9e15 }).suppressed).toBe(false);
  });

  it("reports whether the update/install/repair entry should be visible", () => {
    expect(updateActionsVisible(update(), false, false)).toBe(true);
    expect(updateActionsVisible(update(), true, false)).toBe(false);
    expect(updateActionsVisible(update(), false, true)).toBe(false);
    expect(updateActionsVisible(update({ latestVersion: null }), false, false)).toBe(false);
    expect(updateActionsVisible(update({ installable: false }), false, false)).toBe(false);
    expect(updateActionsVisible(update({ note: "上游已固定" }), false, false)).toBe(true);
    expect(updateActionsVisible(undefined, false, false)).toBe(false);
  });
});
