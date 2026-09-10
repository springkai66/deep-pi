import { describe, expect, it, vi } from "vitest";
import { createDialogQueue, type DialogRequest } from "./dialog";

describe("dialog queue", () => {
  it("cancels only dialogs belonging to the stopped RPC run", async () => {
    let current: DialogRequest | null = null;
    const queue = createDialogQueue((request) => { current = request; });
    const rpc = queue.request({ kind: "confirm", title: "RPC", message: "", scope: "run-a" });
    const other = queue.request({ kind: "confirm", title: "Other", message: "" });
    queue.cancelScope("run-a");
    await expect(rpc).resolves.toBeNull();
    expect(current!.title).toBe("Other");
    queue.resolve(true);
    await expect(other).resolves.toBe(true);
  });
  it("expires timed dialogs without resolving an unrelated next dialog", async () => {
    vi.useFakeTimers();
    try {
      const queue = createDialogQueue(() => {});
      const first = queue.request({ kind: "input", title: "Timed", message: "", timeoutMs: 100 });
      const next = queue.request({ kind: "input", title: "Next", message: "" });
      await vi.advanceTimersByTimeAsync(100);
      await expect(first).resolves.toBeNull();
      queue.resolve("kept");
      await expect(next).resolves.toBe("kept");
      queue.dispose();
    } finally { vi.useRealTimers(); }
  });
  it("keeps a confirmation pending when an error arrives", async () => {
    let current: DialogRequest | null = null;
    const queue = createDialogQueue((request) => { current = request; });
    const confirmation = queue.request({ kind: "confirm", title: "Confirm", message: "" });
    const firstId = current!.id;
    const alert = queue.request({ kind: "alert", title: "Error", message: "" });
    expect(current!.id).toBe(firstId);
    queue.resolve(true);
    await expect(confirmation).resolves.toBe(true);
    expect(current!.kind).toBe("alert");
    queue.resolve(null);
    await expect(alert).resolves.toBeNull();
    expect(current).toBeNull();
  });

  it("settles every request on disposal, including future requests", async () => {
    const queue = createDialogQueue(() => {});
    const first = queue.request({ kind: "input", title: "First", message: "" });
    const second = queue.request({ kind: "confirm", title: "Second", message: "" });
    queue.dispose();
    await expect(first).resolves.toBeNull();
    await expect(second).resolves.toBeNull();
    await expect(queue.request({ kind: "alert", title: "Late", message: "" })).resolves.toBeNull();
  });
});
