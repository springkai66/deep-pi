import { describe, expect, it, vi } from "vitest";
import { createDiagnosticsController, type DiagnosticReport, type DiagnosticsState } from "./diagnostics";
import { createWindowCloseHandler } from "./window-close";

const report: DiagnosticReport = {
  schemaVersion: 1, snapshotId: "snapshot", appVersion: "0.1.0", os: "windows", arch: "x86_64",
  elapsedMs: 10, droppedEvents: 0, events: [],
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (value: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function fixture(invoke = vi.fn(async (command: string): Promise<unknown> => command === "diagnostics_snapshot" ? report : true)) {
  let state: DiagnosticsState;
  const busy = vi.fn();
  const confirm = vi.fn(async () => true);
  const controller = createDiagnosticsController({ invoke, confirm, publish: (next) => { state = next; }, busy });
  return { controller, invoke, busy, confirm, state: () => state! };
}

describe("diagnostics controller", () => {
  it("exports only the displayed snapshot and handles native cancellation", async () => {
    const f = fixture();
    await f.controller.refresh();
    f.invoke.mockResolvedValueOnce(false);
    await f.controller.export();
    expect(f.invoke).toHaveBeenLastCalledWith("diagnostics_export", { snapshotId: "snapshot" });
    expect(f.state().status).toBe("已取消导出");
    expect(f.state().report).toEqual(report);
    expect(f.busy).toHaveBeenLastCalledWith(false);
  });

  it("prevents repeated operations while the native export is pending", async () => {
    const f = fixture();
    await f.controller.refresh();
    const pending = deferred<unknown>();
    f.invoke.mockReturnValueOnce(pending.promise);
    const exporting = f.controller.export();
    await f.controller.export();
    await f.controller.clear();
    await f.controller.refresh();
    expect(f.invoke).toHaveBeenCalledTimes(2);
    pending.resolve(true);
    await exporting;
    expect(f.state().status).toBe("报告已导出");
  });

  it("clearing invalidates the visible preview even if the subsequent refresh fails", async () => {
    const f = fixture();
    await f.controller.refresh();
    f.invoke.mockResolvedValueOnce(undefined).mockRejectedValueOnce("failed");
    await f.controller.clear();
    expect(f.state().report).toBeNull();
    expect(f.state().error).toBeTruthy();
    const calls = f.invoke.mock.calls.length;
    await f.controller.export();
    expect(f.invoke.mock.calls.length).toBe(calls);
  });

  it("keeps the report when clear confirmation is cancelled", async () => {
    const f = fixture();
    await f.controller.refresh();
    f.confirm.mockResolvedValueOnce(false);
    await f.controller.clear();
    expect(f.invoke).toHaveBeenCalledTimes(1);
    expect(f.state().report).toEqual(report);
  });

  it("ignores late results after disposal and always releases busy ownership", async () => {
    const pending = deferred<unknown>();
    const f = fixture(vi.fn(() => pending.promise));
    const refreshing = f.controller.refresh();
    f.controller.dispose();
    pending.resolve(report);
    await refreshing;
    expect(f.state().report).toBeNull();
    expect(f.busy).toHaveBeenLastCalledWith(false);
    await f.controller.refresh();
    expect(f.invoke).toHaveBeenCalledTimes(1);
  });

  it("does not replay an export after a communication failure", async () => {
    const f = fixture();
    await f.controller.refresh();
    f.invoke.mockRejectedValueOnce("disconnected");
    await f.controller.export();
    expect(f.invoke).toHaveBeenCalledTimes(2);
    expect(f.state().error).toContain("无法确认");
    expect(f.state().busy).toBe(false);
  });

  it("shows allowlisted backend errors but never arbitrary IPC text", async () => {
    const f = fixture();
    await f.controller.refresh();
    f.invoke.mockRejectedValueOnce("诊断预览已失效，请刷新后重新导出");
    await f.controller.export();
    expect(f.state().error).toBe("诊断预览已失效，请刷新后重新导出");
    f.invoke.mockRejectedValueOnce("SECRET_TEST_ONLY private-path");
    await f.controller.export();
    expect(f.state().error).not.toContain("SECRET_TEST_ONLY");
  });

  it("blocks window destruction during export and permits closing after cancellation", async () => {
    const f = fixture();
    await f.controller.refresh();
    const pending = deferred<unknown>();
    f.invoke.mockReturnValueOnce(pending.promise);
    const exporting = f.controller.export();
    const destroy = vi.fn(async () => {});
    const error = vi.fn();
    const close = createWindowCloseHandler({
      behavior: () => "exit", blocked: () => f.state().busy, hasActiveTasks: () => false,
      choose: async () => null, confirm: async () => true, remember: () => {},
      minimize: async () => {}, stopPi: async () => {}, stopDsh: async () => {}, destroy, error,
    });
    await close({ preventDefault() {} });
    expect(destroy).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledTimes(1);
    pending.resolve(false);
    await exporting;
    await close({ preventDefault() {} });
    expect(destroy).toHaveBeenCalledTimes(1);
  });
});
