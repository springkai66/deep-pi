import { describe, expect, it } from "vitest";
import { createDialogQueue, type DialogRequest } from "./dialog";

describe("dialog queue", () => {
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
