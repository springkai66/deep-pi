import { describe, expect, it, vi } from "vitest";
import { createDormantHistorySession, createRpcHistorySession, prependRpcHistory, validateHistoryPage, type RpcHistoryPage } from "./rpc-history";
import { emptyConversation } from "./rpc-state";

const page: RpcHistoryPage = { snapshotId: "snapshot", eventSequence: 5, start: 2, end: 4, total: 4,
  messages: [{ role: "user", content: "2" }, { role: "assistant", content: "3" }] };
function fixture() {
  const invoke = vi.fn(async (_command: string, _args: Record<string, unknown>): Promise<unknown> => page);
  return { invoke, session: createRpcHistorySession(<T,>(command: string, args: Record<string, unknown>) =>
    invoke(command, args) as Promise<T>, "task", "run") };
}

describe("RPC history snapshots", () => {
  it("releases an invalid initial snapshot rather than leaking its backend lease", async () => {
    const f = fixture();
    f.invoke.mockResolvedValueOnce({ ...page, total: 5 });
    await expect(f.session.open()).rejects.toThrow();
    expect(f.invoke).toHaveBeenLastCalledWith("rpc_history_close", { taskId: "task", runId: "run", snapshotId: "snapshot" });
  });
  it("ignores a late older page after closing the history session", async () => {
    const f = fixture();
    await f.session.open();
    let finish!: (value: RpcHistoryPage) => void;
    f.invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const older = f.session.older();
    await f.session.dispose();
    finish({ ...page, start: 0, end: 2 });
    expect(await older).toBeNull();
    expect(f.invoke.mock.calls.filter(([command]) => command === "rpc_history_close")).toHaveLength(1);
  });
  it("deduplicates initial opens and releases the exact snapshot when disposed", async () => {
    const f = fixture();
    await Promise.all([f.session.open(), f.session.open()]);
    expect(f.invoke).toHaveBeenCalledTimes(1);
    await f.session.dispose();
    expect(f.invoke).toHaveBeenLastCalledWith("rpc_history_close", { taskId: "task", runId: "run", snapshotId: "snapshot" });
  });
  it("releases a late initial snapshot without publishing it after disposal", async () => {
    const f = fixture();
    let finish!: (value: RpcHistoryPage) => void;
    f.invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const request = f.session.open();
    await f.session.dispose();
    finish(page);
    expect(await request).toBeNull();
    expect(f.invoke).toHaveBeenLastCalledWith("rpc_history_close", { taskId: "task", runId: "run", snapshotId: "snapshot" });
  });
  it("uses immutable snapshot metadata and rejects a noncontiguous older response", async () => {
    const f = fixture();
    await f.session.open();
    f.invoke.mockResolvedValueOnce({ ...page, start: 0, end: 3 });
    await expect(f.session.older()).rejects.toThrow();
    f.invoke.mockResolvedValueOnce({ ...page, start: 0, end: 2, messages: [{ role: "user", content: "0" }, { role: "assistant", content: "1" }] });
    expect((await f.session.older())?.start).toBe(0);
    expect(f.invoke).toHaveBeenLastCalledWith("rpc_history_page", { taskId: "task", runId: "run", snapshotId: "snapshot", before: 2 });
  });
  it("does not send duplicate older reads while one is pending", async () => {
    const f = fixture();
    await f.session.open();
    let finish!: (value: RpcHistoryPage) => void;
    f.invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const first = f.session.older();
    expect(await f.session.older()).toBeNull();
    finish({ ...page, start: 0, end: 2 });
    await first;
    expect(f.invoke).toHaveBeenCalledTimes(2);
  });
  it("prepends history without resetting live messages, tools, busy state or event cursor", () => {
    const state = { ...emptyConversation(), sequence: 10, busy: true,
      messages: [...page.messages, { role: "assistant", content: "live" }] };
    const older = { ...page, start: 0, end: 2 };
    const next = prependRpcHistory(state, older, 2);
    expect(next.sequence).toBe(10);
    expect(next.busy).toBe(true);
    expect(next.tools).toBe(state.tools);
    expect(next.messages.at(-1)?.content).toBe("live");
    expect(next.messages).toHaveLength(5);
    expect(() => prependRpcHistory(next, older, 0)).toThrow();
  });
  it("rejects malformed page counts and messages instead of dropping invalid rows", () => {
    for (const invalid of [
      { ...page, start: -1 }, { ...page, eventSequence: NaN }, { ...page, total: 2 },
      { ...page, messages: [{ content: "no role" }, { role: "user" }] },
      { ...page, messages: [] },
    ]) expect(() => validateHistoryPage(invalid)).toThrow();
  });

  describe("dormant history sessions", () => {
    function dormantFixture() {
      const invoke = vi.fn(async (_command: string, _args: Record<string, unknown>): Promise<unknown> => page);
      const session = createDormantHistorySession(<T,>(command: string, args: Record<string, unknown>) =>
        invoke(command, args) as Promise<T>, "task");
      return { invoke, session };
    }

    it("opens with a backend-less read and pages backwards by start offset", async () => {
      const f = dormantFixture();
      await f.session.open();
      expect(f.invoke).toHaveBeenLastCalledWith("rpc_dormant_history", { taskId: "task", before: null, limit: 100 });
      f.invoke.mockResolvedValueOnce({ ...page, start: 0, end: 2 });
      expect((await f.session.older())?.start).toBe(0);
      expect(f.invoke).toHaveBeenLastCalledWith("rpc_dormant_history", { taskId: "task", before: 2, limit: 100 });
    });

    it("rejects a noncontiguous older response and stops paging at the start", async () => {
      const f = dormantFixture();
      await f.session.open();
      f.invoke.mockResolvedValueOnce({ ...page, start: 0, end: 3 });
      await expect(f.session.older()).rejects.toThrow();
      f.invoke.mockResolvedValueOnce({ ...page, start: 0, end: 2, messages: [{ role: "user", content: "0" }, { role: "assistant", content: "1" }] });
      expect((await f.session.older())?.start).toBe(0);
      f.invoke.mockClear();
      await expect(f.session.older()).resolves.toBeNull();
      expect(f.invoke).not.toHaveBeenCalled();
    });

    it("ignores a late initial open after disposal and deduplicates initial opens", async () => {
      const f = dormantFixture();
      await Promise.all([f.session.open(), f.session.open()]);
      expect(f.invoke).toHaveBeenCalledTimes(1);
      let finish!: (value: RpcHistoryPage) => void;
      f.invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
      const request = f.session.older();
      await f.session.dispose();
      finish({ ...page, start: 0, end: 2 });
      expect(await request).toBeNull();
      expect(f.invoke.mock.calls.filter(([command]) => command === "rpc_history_close")).toHaveLength(0);
    });
  });
});
