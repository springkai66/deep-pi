import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Task } from "./task";
import { createPetTaskBubbles, type PetTaskBubblesState } from "./pet-task-bubbles";

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

function harness() {
  const states: PetTaskBubblesState[] = [];
  const resizes: { open: boolean; force: boolean }[] = [];
  const actions: { action: "stop" | "continue"; id: string }[] = [];
  let nextTasks: Task[] = [];
  let loadFailure: string | null = null;
  let gateLoad = false;
  let gateResize = false;
  const loadQueue: { resolve(tasks: Task[]): void; reject(error: Error): void }[] = [];
  const resizeQueue: { resolve(): void; reject(error: Error): void }[] = [];
  const actionQueue: (() => void)[] = [];
  let gateAction = false;
  const bubbles = createPetTaskBubbles({
    load: () => {
      if (!gateLoad) return loadFailure ? Promise.reject(new Error(loadFailure)) : Promise.resolve(nextTasks);
      return new Promise((resolve, reject) => loadQueue.push({ resolve, reject }));
    },
    resize: (open, force = false) => {
      resizes.push({ open, force });
      if (!gateResize) return Promise.resolve();
      return new Promise((resolve, reject) => resizeQueue.push({ resolve, reject }));
    },
    action: async (action, value) => {
      actions.push({ action, id: value.id });
      if (gateAction) await new Promise<void>((resolve) => actionQueue.push(resolve));
    },
    changed: (state) => states.push(structuredClone(state)),
  });
  return {
    states, resizes, actions, bubbles,
    setTasks: (tasks: Task[]) => { nextTasks = tasks; },
    failLoads: (message: string | null) => { loadFailure = message; },
    gateLoads: () => { gateLoad = true; },
    resolveLoads: (tasks: Task[]) => {
      // One resolution per call: lets tests serve different data per read.
      loadQueue.shift()?.resolve(tasks);
    },
    gateResizes: () => { gateResize = true; },
    resolveResizes: () => { for (const pending of resizeQueue.splice(0)) pending.resolve(); },
    rejectResizes: (error: Error) => { for (const pending of resizeQueue.splice(0)) pending.reject(error); },
    gateActions: () => { gateAction = true; },
    resolveActions: () => { for (const resolve of actionQueue.splice(0)) resolve(); },
  };
}

/** Flush the promise chains that schedule native resize/load calls. */
const tick = async () => {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
};

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { vi.useRealTimers(); });

describe("pet task bubble controller", () => {
  it("opens on hover with a fresh snapshot and closes after the leave grace", async () => {
    const app = harness();
    app.setTasks([task({ id: "t1" })]);
    const entering = app.bubbles.enter();
    await tick();
    expect(app.resizes).toEqual([{ open: true, force: false }]);
    await entering;
    await tick();
    expect(app.states.at(-1)?.tasks.map((value) => value.id)).toEqual(["t1"]);
    app.bubbles.leave();
    expect(app.resizes).toHaveLength(1); // Not closed before the grace elapses.
    await vi.advanceTimersByTimeAsync(400);
    expect(app.resizes.at(-1)).toEqual({ open: false, force: false });
    expect(app.states.at(-1)?.open).toBe(false);
  });

  it("cancels a pending close when the pointer re-enters", async () => {
    const app = harness();
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.bubbles.leave();
    app.bubbles.enter();
    await vi.advanceTimersByTimeAsync(1000);
    expect(app.resizes.filter((resize) => !resize.open)).toHaveLength(0);
  });

  it("keeps the bubble open while an action is pending, then refreshes", async () => {
    const app = harness();
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.bubbles.leave();
    app.gateActions();
    const acting = app.bubbles.act("stop", task({ id: "t1" }));
    await tick();
    await vi.advanceTimersByTimeAsync(1000);
    expect(app.resizes.filter((resize) => !resize.open)).toHaveLength(0);
    app.resolveActions();
    await acting;
    await tick();
    await vi.advanceTimersByTimeAsync(400);
    expect(app.resizes.at(-1)).toEqual({ open: false, force: false });
    expect(app.actions).toEqual([{ action: "stop", id: "t1" }]);
  });

  it("suppresses the pre-action snapshot; only post-action reads publish", async () => {
    const app = harness();
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.gateLoads(); // Hold the next background read in flight.
    const reading = app.bubbles.refresh();
    const acting = app.bubbles.act("stop", task({ id: "t1" }));
    await tick();
    // Read A started before the action: it must never publish, even though it
    // resolves with tasks. Read B (the post-action refresh) publishes its data.
    app.resolveLoads([task({ id: "pre-action" })]);
    await reading;
    expect(app.states.some((state) => state.tasks.some((value) => value.id === "pre-action"))).toBe(false);
    app.resolveLoads([task({ id: "post-action" })]);
    await acting;
    expect(app.states.at(-1)?.tasks.map((value) => value.id)).toEqual(["post-action"]);
    expect(app.actions).toEqual([{ action: "stop", id: "t1" }]);
  });

  it("rolls the native window back when opening fails", async () => {
    const app = harness();
    app.gateResizes();
    const entering = app.bubbles.enter();
    await tick();
    app.rejectResizes(new Error("native failure"));
    await tick(); // Let the controller schedule the rollback hide.
    app.resolveResizes(); // Release it.
    await entering;
    expect(app.states.at(-1)?.open).toBe(false);
    expect(app.states.at(-1)?.error).toContain("桌宠窗口调整失败");
    app.resolveResizes(); // Release the rollback hide.
    await tick();
    expect(app.resizes.at(-1)).toEqual({ open: false, force: false });
  });

  it("shows read failures and recovers on retry", async () => {
    const app = harness();
    app.failLoads("ipc down");
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    expect(app.states.at(-1)?.error).toContain("无法读取任务");
    app.failLoads(null);
    app.setTasks([task({ id: "t1" })]);
    await app.bubbles.retry();
    expect(app.states.at(-1)?.error).toBe("");
    expect(app.states.at(-1)?.tasks.map((value) => value.id)).toEqual(["t1"]);
  });

  it("forces the native close on disposal", async () => {
    const app = harness();
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.bubbles.dispose();
    await tick();
    expect(app.resizes.at(-1)).toEqual({ open: false, force: true });
  });
});
