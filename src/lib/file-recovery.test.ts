import { describe, expect, it, vi } from "vitest";
import { createRecoveryManager, type RecoveryState, type RecoveryItem, type RecoveryPage } from "./file-recovery";
import { appConfirmDialog } from "./app-messages";
import { getLocale } from "./i18n.svelte";

const item: RecoveryItem = { id: "record:recovery", target: "src/a.txt", path: "src/.deeppi-id.recovery",
  kind: "recovery", createdAt: 1, status: "available", version: "version1", size: 4, detail: "" };

function fixture() {
  let state!: RecoveryState;
  const invoke = vi.fn(async (_command: string, _args: Record<string, unknown>): Promise<unknown> => ({ items: [item], total: 1, nextOffset: null }));
  const busy = vi.fn();
  const changed = vi.fn();
  const manager = createRecoveryManager(
    <T,>(command: string, args: Record<string, unknown>) => invoke(command, args) as Promise<T>,
    (next) => { state = next; }, busy, changed,
  );
  manager.select("p");
  return { manager, invoke, busy, changed, state: () => state };
}

describe("recovery manager", () => {
  it("deduplicates an offset page when a new intent shifts earlier rows", async () => {
    const f = fixture();
    await f.manager.load();
    const second = { ...item, id: "second:recovery" };
    f.invoke.mockResolvedValueOnce({ items: [item, second], total: 3, nextOffset: null });
    await f.manager.load(1);
    expect(f.state().items.map((row) => row.id)).toEqual([item.id, second.id]);
  });
  it("does not let an older refresh replace a newer page or clear its loading state", async () => {
    const f = fixture();
    let finish!: (value: RecoveryPage) => void;
    f.invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const old = f.manager.load();
    f.invoke.mockResolvedValueOnce({ items: [], total: 0, nextOffset: null });
    await f.manager.load();
    finish({ items: [item], total: 1, nextOffset: null });
    await old;
    expect(f.state().items).toEqual([]);
    expect(f.state().loading).toBe(false);
  });
  it("refuses writes from stale rows and refuses deleting unavailable files", async () => {
    const f = fixture();
    await f.manager.load();
    await f.manager.run("delete", { ...item, version: "old" });
    await f.manager.run("delete", { ...item, id: "not-listed" });
    f.invoke.mockResolvedValueOnce({ items: [{ ...item, status: "missing", version: null }], total: 1, nextOffset: null });
    await f.manager.load();
    await f.manager.run("delete", { ...item, status: "missing", version: null });
    expect(f.invoke).toHaveBeenCalledTimes(2);
    expect(f.busy).not.toHaveBeenCalled();
  });
  it("ignores a stale project list while preserving the selected project's page", async () => {
    const f = fixture();
    let finish!: (value: RecoveryPage) => void;
    f.invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const first = f.manager.load();
    f.manager.select("q");
    f.invoke.mockResolvedValueOnce({ items: [], total: 0, nextOffset: null });
    await f.manager.load();
    finish({ items: [item], total: 1, nextOffset: null });
    await first;
    expect(f.state().items).toEqual([]);
    expect(f.state().projectId).toBe("q");
  });
  it("passes the exact displayed version to delete and keeps native cancellation non-destructive", async () => {
    const f = fixture();
    await f.manager.load();
    f.invoke.mockResolvedValueOnce(false);
    await f.manager.run("delete", item);
    expect(f.invoke).toHaveBeenLastCalledWith("delete_project_recovery", {
      projectId: "p", recordId: item.id, expectedVersion: "version1",
      dialogs: {
        permanent: appConfirmDialog("recovery.delete.dialog.permanent", getLocale()),
        recordOnly: appConfirmDialog("recovery.delete.dialog.record_only", getLocale()),
      },
    });
    expect(f.changed).not.toHaveBeenCalled();
    expect(f.state().items).toEqual([item]);
    expect(f.state().writing).toBe(false);
  });
  it("keeps operations exclusive across project switches and releases busy after an IPC error", async () => {
    const f = fixture();
    await f.manager.load();
    let fail!: (error: Error) => void;
    f.invoke.mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject; }));
    const operation = f.manager.run("delete", item);
    f.manager.select("q");
    await f.manager.run("delete", item);
    expect(f.state().writing).toBe(true);
    fail(new Error("connection lost"));
    await operation;
    expect(f.invoke).toHaveBeenCalledTimes(2);
    expect(f.state().projectId).toBe("q");
    expect(f.state().writing).toBe(false);
    expect(f.busy).toHaveBeenLastCalledWith(false);
  });
  it("forgets records without supplying a deletion version and preserves restore conflicts", async () => {
    const f = fixture();
    await f.manager.load();
    f.invoke.mockResolvedValueOnce(true);
    await f.manager.run("forget", item);
    expect(f.invoke.mock.calls[1]).toEqual(["delete_project_recovery", {
      projectId: "p", recordId: item.id, expectedVersion: null,
      dialogs: {
        permanent: appConfirmDialog("recovery.delete.dialog.permanent", getLocale()),
        recordOnly: appConfirmDialog("recovery.delete.dialog.record_only", getLocale()),
      },
    }]);
    f.invoke.mockResolvedValueOnce({ outcome: "conflict", version: null, detail: "target exists" });
    await f.manager.run("restore", item, "new.txt");
    expect(f.state().message).toContain("target exists");
    expect(f.invoke.mock.calls[3][1]).toEqual({ projectId: "p", request: {
      recordId: item.id, expectedVersion: "version1", relativePath: "new.txt",
    }, dialog: appConfirmDialog("recovery.restore.dialog", getLocale()) });
  });
  it("never automatically repeats a write after an unknown result", async () => {
    const f = fixture();
    await f.manager.load();
    f.invoke.mockRejectedValueOnce(new Error("lost response"));
    await f.manager.run("delete", item);
    expect(f.invoke).toHaveBeenCalledTimes(2);
    expect(f.state().error).toContain("lost response");
    expect(f.state().writing).toBe(false);
  });
});
