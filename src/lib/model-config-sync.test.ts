import { describe, expect, it, vi } from "vitest";
import { notifyModelsChanged, onModelsChanged } from "./model-config-sync";

describe("model config change broadcast", () => {
  it("notifies every subscriber and stops after unsubscribe", () => {
    const first = vi.fn();
    const second = vi.fn();
    const off = onModelsChanged(first);
    onModelsChanged(second);

    notifyModelsChanged();
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);

    off();
    notifyModelsChanged();
    expect(first).toHaveBeenCalledTimes(1); // 已取消订阅
    expect(second).toHaveBeenCalledTimes(2);
  });

  it("isolates listener errors so other panels still get notified", () => {
    const bad = vi.fn(() => {
      throw new Error("listener bug");
    });
    const good = vi.fn();
    const offBad = onModelsChanged(bad);
    const offGood = onModelsChanged(good);

    expect(() => notifyModelsChanged()).not.toThrow();
    expect(bad).toHaveBeenCalledTimes(1);
    expect(good).toHaveBeenCalledTimes(1);

    offBad();
    offGood();
  });
});
