import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./task";
import {
  canContinuePetTask,
  canStopPetTask,
  createPetTaskActionClient,
  createPetTaskActionHandler,
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

  it("continues waiting live sessions or dormant stopped ones, never running processes", () => {
    expect(canContinuePetTask(task({ id: "a", status: "waiting" }))).toBe(true);
    expect(canContinuePetTask(task({ id: "a", status: "cancelled", runId: null }))).toBe(true);
    expect(canContinuePetTask(task({ id: "a", status: "cancelled" }))).toBe(false);
    expect(canContinuePetTask(task({ id: "a" }))).toBe(false);
    expect(canContinuePetTask(task({ id: "a", status: "completed" }))).toBe(false);
    expect(canContinuePetTask(task({ id: "a", projectId: null }))).toBe(false);
  });

  it("shows live tasks plus retained stopped ones and hides archived rows", () => {
    const running = task({ id: "run" });
    const stopped = task({ id: "stop", status: "cancelled", runId: null });
    const retained = visiblePetTasks([running, stopped, task({ id: "old", status: "failed", runId: null })], new Set(["stop"]));
    expect(retained.map((task) => task.id)).toEqual(["run", "stop"]);
    expect(visiblePetTasks([task({ id: "archived", archivedAt: 1 })], new Set(["archived"]))).toEqual([]);
  });
});

describe("pet task action handler (main webview)", () => {
  function ports(overrides: Partial<Parameters<typeof createPetTaskActionHandler>[0]> = {}) {
    const busy = new Set<string>();
    const calls = { stop: [] as Task[], restart: [] as Task[], open: [] as Task[], prepare: 0 };
    return {
      calls,
      busy,
      ports: {
        loadTask: async (id: string) => task({ id }),
        isBusy: (id: string) => busy.has(id),
        busyChanged: (id: string, isBusy: boolean) => { if (isBusy) busy.add(id); else busy.delete(id); },
        stop: async (value: Task) => { calls.stop.push(value); },
        restart: async (value: Task) => { calls.restart.push(value); return { ...value, runId: "run-2", status: "running" } as Task; },
        prepareContinue: async () => { calls.prepare += 1; },
        open: (value: Task) => { calls.open.push(value); },
        updated: () => {},
        ...overrides,
      } as Parameters<typeof createPetTaskActionHandler>[0],
    };
  }

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

  it("continues a waiting live session without creating a duplicate process", async () => {
    const { ports: hooks, calls } = ports({
      loadTask: async (id) => task({ id, status: "waiting" }),
    });
    const handler = createPetTaskActionHandler(hooks);
    const result = await handler({ requestId: "r1", taskId: "t1", runId: "run-1", action: "continue" });
    expect(result.ok).toBe(true);
    expect(calls.restart).toHaveLength(0);
    expect(calls.open).toHaveLength(1);
    expect(calls.open[0].runId).toBe("run-1");
  });

  it("restarts a dormant stopped session when continuing", async () => {
    const { ports: hooks, calls } = ports({
      loadTask: async (id) => task({ id, status: "cancelled", runId: null }),
    });
    const handler = createPetTaskActionHandler(hooks);
    const result = await handler({ requestId: "r1", taskId: "t1", runId: null, action: "continue" });
    expect(result.ok).toBe(true);
    expect(calls.restart).toHaveLength(1);
    expect(calls.open).toHaveLength(1);
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

  it("surfaces prepareContinue failures without opening the task", async () => {
    const { ports: hooks, calls } = ports({
      prepareContinue: async () => { throw new Error("设置操作中"); },
    });
    const handler = createPetTaskActionHandler(hooks);
    const result = await handler({
      requestId: "r1", taskId: "t1", runId: null,
      action: "continue",
    });
    expect(result.ok).toBe(false);
    expect(calls.restart).toHaveLength(0);
    expect(calls.open).toHaveLength(0);
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
    const pending = client.request("continue", task({ id: "t1" }));
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
