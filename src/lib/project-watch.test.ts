import { afterEach, describe, expect, it, vi } from "vitest";
import { createProjectWatch, type ProjectWatchEvent } from "./project-watch";

function fixture() {
  let emit!: (event: ProjectWatchEvent) => void;
  const start = vi.fn(async (_id: string, callback: typeof emit) => { emit = callback; });
  const close = vi.fn(async () => {});
  const ping = vi.fn(async () => {});
  const changed = vi.fn();
  const status = vi.fn();
  const watch = createProjectWatch("p", { start, close, ping, changed, status });
  return { start, close, ping, changed, status, watch, emit: (event: Partial<ProjectWatchEvent> = {}) =>
    emit({ projectId: "p", watchId: start.mock.calls[0][0], files: true, git: true, status: "changed", ...event }) };
}
afterEach(() => vi.useRealTimers());
describe("project watch lifecycle", () => {
  it("ignores other projects and stale subscriptions and refreshes once after registration", async () => {
    const f = fixture();
    await f.watch.start();
    expect(f.changed).toHaveBeenCalledTimes(1);
    f.emit({ projectId: "other" }); f.emit({ watchId: "old" });
    expect(f.changed).toHaveBeenCalledTimes(1);
    f.emit({ files: false });
    expect(f.changed).toHaveBeenLastCalledWith(false, true);
    await f.watch.dispose();
    f.emit();
    expect(f.changed).toHaveBeenCalledTimes(2);
  });
  it("releases a late registration with its exact lease id", async () => {
    const f = fixture();
    let resolve!: () => void;
    f.start.mockImplementationOnce(() => new Promise<void>((done) => { resolve = done; }));
    const starting = f.watch.start();
    const disposing = f.watch.dispose();
    resolve();
    await starting; await disposing;
    expect(f.close).toHaveBeenCalledTimes(1);
    expect(f.close).toHaveBeenCalledWith(f.start.mock.calls[0][0]);
    expect(f.changed).not.toHaveBeenCalled();
  });
  it("invalidates failed watchers and never keeps emitting after root replacement", async () => {
    const f = fixture();
    await f.watch.start();
    f.emit({ status: "rootChanged" });
    expect(f.status).toHaveBeenLastCalledWith("项目目录已变化，请重新连接文件监听");
    f.emit();
    expect(f.changed).toHaveBeenCalledTimes(2);
    await f.watch.dispose();
  });
  it("renews the lease without overlapping pings and releases timers on disposal", async () => {
    vi.useFakeTimers();
    const f = fixture();
    await f.watch.start();
    let resolve!: () => void;
    f.ping.mockImplementationOnce(() => new Promise<void>((done) => { resolve = done; }));
    await vi.advanceTimersByTimeAsync(90_000);
    expect(f.ping).toHaveBeenCalledTimes(1);
    resolve();
    await f.watch.dispose();
    await vi.advanceTimersByTimeAsync(90_000);
    expect(f.ping).toHaveBeenCalledTimes(1);
  });
});
