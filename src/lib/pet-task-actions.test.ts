import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./task";
import {
  canOpenPetTask,
  canStopPetTask,
  createPetTaskActionClient,
  createPetTaskActionHandler,
  isPetTaskRequest,
  visiblePetTasks,
} from "./pet-task-actions";

function task(overrides: Partial<Task> & { id: string }): Task {
  return {
    runId: "run-1",
    title: `任务 ${overrides.id}`,
    agent: "pi",
    status: "running",
    interactionMode: "rpc",
    projectId: "project",
    projectPath: "F:/project",
    sessionId: "session",
    sessionFile: null,
    executionTarget: "local",
    createdAt: 0,
    startedAt: null,
    completedAt: null,
    archivedAt: null,
    ...overrides,
  } as Task;
}

describe("pet task action gates", () => {
  it("allows stopping only live pi sessions with a run id", () => {
    expect(canStopPetTask(task({ id: "a" }))).toBe(true);
    expect(canStopPetTask(task({ id: "a", status: "waiting" }))).toBe(true);
    expect(canStopPetTask(task({ id: "a", status: "failed" }))).toBe(false);
    expect(canStopPetTask(task({ id: "a", agent: "dsh" }))).toBe(false);
    expect(canStopPetTask(task({ id: "a", runId: null }))).toBe(false);
    expect(canStopPetTask(task({ id: "a", archivedAt: 1 }))).toBe(false);
  });

  it("accepts stop and open requests but still drops continue ones", () => {
    // 桌宠不再暴露「继续」：旧形态的请求一律当作无效载荷丢弃。
    expect(isPetTaskRequest({ requestId: "r", taskId: "t", runId: null, action: "stop" })).toBe(true);
    expect(isPetTaskRequest({ requestId: "r", taskId: "t", runId: null, action: "open" })).toBe(true);
    expect(isPetTaskRequest({ requestId: "r", taskId: "t", runId: null, action: "continue" })).toBe(false);
    expect(isPetTaskRequest({ requestId: "r", taskId: "t", runId: null, action: 1 })).toBe(false);
    expect(isPetTaskRequest(null)).toBe(false);
  });

  it("opens any live or ended task but not archived rows", () => {
    expect(canOpenPetTask(task({ id: "a" }))).toBe(true);
    expect(canOpenPetTask(task({ id: "a", agent: "dsh" }))).toBe(true);
    expect(canOpenPetTask(task({ id: "a", status: "completed", runId: null }))).toBe(true);
    expect(canOpenPetTask(task({ id: "a", archivedAt: 1 }))).toBe(false);
  });

  it("shows live tasks plus retained ended ones and hides archived rows", () => {
    const running = task({ id: "run" });
    const stopped = task({ id: "stop", status: "cancelled", runId: null });
    const completed = task({ id: "done", status: "completed", runId: null });
    // 三种结束方式都要能留在环上把结果讲完：被停止、失败、自然完成。
    const retained = visiblePetTasks(
      [running, stopped, task({ id: "failed", status: "failed", runId: null }), completed],
      new Set(["stop", "failed", "done"]),
    );
    expect(retained.map((task) => task.id)).toEqual(["run", "stop", "failed", "done"]);
    // 没登记保留项的结束任务不显示（避免每次悬停都翻出历史任务）。
    expect(visiblePetTasks([completed], new Set()).map((task) => task.id)).toEqual([]);
    expect(visiblePetTasks([task({ id: "archived", archivedAt: 1 })], new Set(["archived"]))).toEqual([]);
  });
});

describe("pet task action handler (main webview)", () => {
  function ports(overrides: Partial<Parameters<typeof createPetTaskActionHandler>[0]> = {}) {
    const busy = new Set<string>();
    const calls = { stop: [] as Task[], reveal: [] as Task[] };
    return {
      calls,
      busy,
      ports: {
        loadTask: async (id: string) => task({ id }),
        isBusy: (id: string) => busy.has(id),
        busyChanged: (id: string, isBusy: boolean) => { if (isBusy) busy.add(id); else busy.delete(id); },
        stop: async (value: Task) => { calls.stop.push(value); },
        reveal: async (value: Task) => { calls.reveal.push(value); },
        updated: () => {},
        ...overrides,
      } as Parameters<typeof createPetTaskActionHandler>[0],
    };
  }

  it("opens any task without touching the shared busy lock", async () => {
    const { ports: hooks, calls, busy } = ports({
      loadTask: async (id) => task({ id, agent: "dsh", status: "waiting", runId: null }),
    });
    const handler = createPetTaskActionHandler(hooks);
    const opened = await handler({ requestId: "r1", taskId: "t1", runId: null, action: "open" });
    expect(opened.ok).toBe(true);
    expect(calls.reveal.map((value) => value.id)).toEqual(["t1"]);
    // 打开是只读动作：不占锁，随后的停止请求会走到自己的校验（而不是被锁挡回）。
    expect(busy.size).toBe(0);
    const stopped = await handler({ requestId: "r2", taskId: "t1", runId: null, action: "stop" });
    expect(stopped.ok).toBe(false);
    expect(stopped.error).not.toContain("任务正在处理");
  });

  it("reports missing or archived tasks instead of opening them", async () => {
    const { ports: hooks, calls } = ports({
      loadTask: async (id) => (id === "gone" ? undefined : task({ id, archivedAt: 7 })),
    });
    const handler = createPetTaskActionHandler(hooks);
    const missing = await handler({ requestId: "r1", taskId: "gone", runId: null, action: "open" });
    expect(missing.ok).toBe(false);
    const archived = await handler({ requestId: "r2", taskId: "old", runId: null, action: "open" });
    expect(archived.ok).toBe(false);
    expect(calls.reveal).toHaveLength(0);
  });

  it("surfaces reveal failures (main window busy with settings)", async () => {
    const { ports: hooks } = ports({
      reveal: async () => { throw new Error("主窗口正在处理设置操作，请稍后继续"); },
    });
    const handler = createPetTaskActionHandler(hooks);
    const result = await handler({ requestId: "r1", taskId: "t1", runId: null, action: "open" });
    expect(result.ok).toBe(false);
    expect(result.error).toContain("设置操作");
  });

  it("stops a live session and rejects stale run ids", async () => {
    const { ports: hooks, calls } = ports();
    const handler = createPetTaskActionHandler(hooks);
    const stopped = await handler({ requestId: "r1", taskId: "t1", runId: "run-1", action: "stop" });
    expect(stopped.ok).toBe(true);
    expect(calls.stop).toHaveLength(1);
    const stale = await handler({ requestId: "r2", taskId: "t1", runId: "old-run", action: "stop" });
    expect(stale.ok).toBe(false);
    expect(calls.stop).toHaveLength(1);
  });

  it("refuses DSH tasks, which can only be stopped from the main window", async () => {
    const { ports: hooks, calls } = ports({
      loadTask: async (id) => task({ id, agent: "dsh" }),
    });
    const handler = createPetTaskActionHandler(hooks);
    const result = await handler({ requestId: "r1", taskId: "t1", runId: "run-1", action: "stop" });
    expect(result.ok).toBe(false);
    expect(result.error).toContain("主窗口");
    expect(calls.stop).toHaveLength(0);
  });

  it("serializes per task with the shared lock and rejects reentry", async () => {
    const { ports: hooks, busy } = ports();
    const handler = createPetTaskActionHandler(hooks);
    const first = handler({ requestId: "r1", taskId: "t1", runId: "run-1", action: "stop" });
    const second = await handler({ requestId: "r2", taskId: "t1", runId: "run-1", action: "stop" });
    expect(second.ok).toBe(false);
    await first;
    expect(busy.size).toBe(0);
  });

  it("surfaces native stop failures without marking the task updated", async () => {
    const updated: Task[] = [];
    const { ports: hooks } = ports({
      stop: async () => { throw new Error("终端未就绪"); },
      updated: (value: Task) => { updated.push(value); },
    });
    const handler = createPetTaskActionHandler(hooks);
    const result = await handler({ requestId: "r1", taskId: "t1", runId: "run-1", action: "stop" });
    expect(result.ok).toBe(false);
    expect(result.error).toContain("终端未就绪");
    expect(updated).toHaveLength(0);
  });
});

describe("pet task action client", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it("resolves on matching ack and ignores unknown or mismatched replies", async () => {
    const sent: unknown[] = [];
    const client = createPetTaskActionClient(async (request) => { sent.push(request); });
    const pending = client.request("stop", task({ id: "t1" }));
    const requestId = (sent[0] as { requestId: string }).requestId;
    client.receive({ requestId: "other", taskId: "t1", ok: true });
    client.receive({ requestId, taskId: "other", ok: true });
    client.receive({ requestId, taskId: "t1", ok: true });
    await expect(pending).resolves.toBeUndefined();
  });

  it("rejects with the reported error", async () => {
    const sent: { requestId: string }[] = [];
    const client = createPetTaskActionClient(async (request) => { sent.push(request); });
    const pending = client.request("stop", task({ id: "t1" }));
    client.receive({ requestId: sent[0].requestId, taskId: "t1", ok: false, error: "任务状态已改变" });
    await expect(pending).rejects.toThrow("任务状态已改变");
  });

  it("times out unacknowledged actions", async () => {
    const client = createPetTaskActionClient(async () => {});
    const pending = client.request("stop", task({ id: "t1" }));
    const assertion = expect(pending).rejects.toThrow("未确认");
    await vi.advanceTimersByTimeAsync(30_000);
    await assertion;
  });

  it("rejects immediately when sending fails and disposes pending work", async () => {
    let failing = false;
    const client = createPetTaskActionClient(async () => {
      if (failing) throw new Error("event bridge down");
    });
    failing = true;
    await expect(client.request("stop", task({ id: "t1" }))).rejects.toThrow("event bridge down");
    failing = false;
    const pending = client.request("stop", task({ id: "t2" }));
    client.dispose();
    await expect(pending).rejects.toThrow("桌宠已关闭");
  });
});
