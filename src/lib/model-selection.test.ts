import { describe, expect, it, vi } from "vitest";
import { createModelSelectionSaver } from "./model-selection";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function tick() { await Promise.resolve(); await Promise.resolve(); }

describe("createModelSelectionSaver", () => {
  it("serializes immutable snapshots and keeps the latest queued intent", async () => {
    const first = deferred<string>();
    const last = deferred<string>();
    const save = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(last.promise);
    const saved = vi.fn();
    const busy = vi.fn();
    const failed = vi.fn();
    const saver = createModelSelectionSaver({ save, saved, busy, failed });
    const value = { modelIds: ["a"] };
    saver.enqueue(value);
    value.modelIds.push("mutated");
    await tick();
    saver.enqueue({ modelIds: ["a", "b"] });
    saver.enqueue({ modelIds: ["b"] });
    first.resolve("old");
    await tick();
    expect(save.mock.calls.map(([argument]) => argument.modelIds)).toEqual([["a"], ["b"]]);
    expect(saved).toHaveBeenCalledWith({ modelIds: ["a"] }, "old", false);
    last.resolve("new");
    await saver.flush();
    expect(saved).toHaveBeenLastCalledWith({ modelIds: ["b"] }, "new", true);
    expect(failed).not.toHaveBeenCalled();
    expect(busy).toHaveBeenLastCalledWith(false);
  });

  it("reports only the latest failure and retries that intent", async () => {
    const old = deferred<void>();
    const save = vi.fn().mockReturnValueOnce(old.promise).mockRejectedValueOnce("offline").mockResolvedValueOnce(undefined);
    const failed = vi.fn();
    const saver = createModelSelectionSaver({ save, saved: vi.fn(), busy: vi.fn(), failed });
    saver.enqueue(["old"]);
    await tick();
    saver.enqueue(["new"]);
    old.reject("stale failure");
    await expect(saver.flush()).rejects.toBe("offline");
    expect(failed).toHaveBeenCalledTimes(1);
    expect(failed).toHaveBeenCalledWith("offline");
    saver.retry();
    await saver.flush();
    expect(save).toHaveBeenLastCalledWith(["new"]);
  });
});
