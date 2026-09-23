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

function harness(options: Parameters<typeof createPetTaskBubbles>[1] = {}) {
  const states: PetTaskBubblesState[] = [];
  const resizes: { open: boolean; force: boolean }[] = [];
  const actions: { id: string }[] = [];
  let nextTasks: Task[] = [];
  let loadFailure: string | null = null;
  let gateLoad = false;
  let gateResize = false;
  const loadQueue: { resolve(tasks: Task[]): void; reject(error: Error): void }[] = [];
  const resizeQueue: { resolve(): void; reject(error: Error): void }[] = [];
  const actionQueue: (() => void)[] = [];
  let actionFailure: string | null = null;
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
    action: async (_action, value) => {
      actions.push({ id: value.id });
      if (actionFailure) throw new Error(actionFailure);
      if (gateAction) await new Promise<void>((resolve) => actionQueue.push(resolve));
    },
    changed: (state) => states.push(structuredClone(state)),
  }, options);
  return {
    states, resizes, actions, bubbles,
    setTasks: (tasks: Task[]) => { nextTasks = tasks; },
    failLoads: (message: string | null) => { loadFailure = message; },
    failActions: (message: string | null) => { actionFailure = message; },
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

/** 走完一次「鼠标离开 → 退场动画播完」的完整收起。 */
async function leaveAndSettle(app: ReturnType<typeof harness>) {
  app.bubbles.leave();
  await vi.advanceTimersByTimeAsync(400);
  expect(app.states.at(-1)?.leaving).toBe(true);
  await app.bubbles.finishClose();
  await tick();
}

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
    // 宽限期结束只进入退场：窗口要等动画播完才隐藏。
    expect(app.states.at(-1)?.leaving).toBe(true);
    expect(app.resizes).toHaveLength(1);
    await app.bubbles.finishClose();
    await tick();
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

  it("revives the ring when the pointer comes back during the exit animation", async () => {
    const app = harness();
    app.setTasks([task({ id: "t1" })]);
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.bubbles.leave();
    await vi.advanceTimersByTimeAsync(400);
    expect(app.states.at(-1)?.leaving).toBe(true);
    // 指针在退场途中回到环上：动画播完后不该把窗口藏掉。
    app.bubbles.keepOpen();
    await app.bubbles.finishClose();
    await tick();
    expect(app.states.at(-1)?.leaving).toBe(false);
    expect(app.states.at(-1)?.open).toBe(true);
    expect(app.resizes.filter((resize) => !resize.open)).toHaveLength(0);
  });

  it("hides immediately when there is nothing to animate out", async () => {
    // 环上一个元素都没有：退场动画没有可播的内容，不该白等一百多毫秒。
    const app = harness();
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.bubbles.leave();
    await vi.advanceTimersByTimeAsync(400);
    expect(app.states.at(-1)?.open).toBe(false);
    expect(app.resizes.at(-1)).toEqual({ open: false, force: false });
  });

  it("keeps the bubble open while an action is pending, then refreshes", async () => {
    const app = harness();
    app.setTasks([task({ id: "t1" })]);
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.bubbles.leave();
    app.gateActions();
    const acting = app.bubbles.act("stop", task({ id: "t1" }));
    await tick();
    await vi.advanceTimersByTimeAsync(1000);
    // 动作在途：宽限期到期也不收起（否则用户会看到气泡在操作中途消失）。
    expect(app.resizes.filter((resize) => !resize.open)).toHaveLength(0);
    expect(app.states.at(-1)?.leaving).toBe(false);
    app.resolveActions();
    await acting;
    await tick();
    // 动作结束后指针已经离开：补上退场，不允许环挂在屏幕上。
    expect(app.states.at(-1)?.leaving).toBe(true);
    expect(app.actions).toEqual([{ id: "t1" }]);
    expect(app.resizes.filter((resize) => !resize.open)).toHaveLength(0);
    await app.bubbles.finishClose();
    await tick();
    expect(app.resizes.at(-1)).toEqual({ open: false, force: false });
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
    expect(app.actions).toEqual([{ id: "t1" }]);
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

describe("task end notice", () => {
  it("holds a naturally completed task on the ring, then fades it out like the rest", async () => {
    const app = harness({ endedHoldMs: 1000 });
    app.setTasks([task({ id: "t1" })]);
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    // 任务自己跑完了（没有经过结束按钮）：气泡要留下来把结果讲完。
    app.setTasks([task({ id: "t1", status: "completed", runId: null, completedAt: 5 })]);
    await app.bubbles.refresh();
    expect(app.states.at(-1)?.tasks.map((value) => value.id)).toEqual(["t1"]);
    expect(app.states.at(-1)?.endedAt.t1).toBeTypeOf("number");
    await vi.advanceTimersByTimeAsync(1000);
    expect(app.states.at(-1)?.leaving).toBe(true);
    await app.bubbles.finishClose();
    await tick();
    expect(app.states.at(-1)?.open).toBe(false);
  });

  it("gives a stopped task the same end notice", async () => {
    const app = harness({ endedHoldMs: 1000 });
    app.setTasks([task({ id: "t1" })]);
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    await app.bubbles.act("stop", task({ id: "t1" }));
    // 停止后任务进入 cancelled：气泡留着，等待停留时间走完。
    app.setTasks([task({ id: "t1", status: "cancelled", runId: null })]);
    await app.bubbles.refresh();
    expect(app.states.at(-1)?.tasks.map((value) => value.id)).toEqual(["t1"]);
    expect(app.states.at(-1)?.endedAt.t1).toBeTypeOf("number");
    await vi.advanceTimersByTimeAsync(1000);
    expect(app.states.at(-1)?.leaving).toBe(true);
  });

  it("does not restart the hold when a refresh repeats the same ended task", async () => {
    const app = harness({ endedHoldMs: 1000 });
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    const ended = task({ id: "t1", status: "completed", runId: null, completedAt: 5 });
    app.setTasks([ended]);
    await app.bubbles.refresh();
    const firstSeen = app.states.at(-1)?.endedAt.t1;
    await vi.advanceTimersByTimeAsync(600);
    await app.bubbles.refresh();
    expect(app.states.at(-1)?.endedAt.t1).toBe(firstSeen);
  });

  it("replays the animation from zero on every open", async () => {
    const app = harness();
    app.setTasks([task({ id: "t1" }), task({ id: "t2" })]);
    const first = app.bubbles.enter();
    await tick();
    await first;
    await leaveAndSettle(app);
    const second = app.bubbles.enter();
    await tick();
    await second;
    expect(app.states.at(-1)?.open).toBe(true);
    expect(app.states.at(-1)?.leaving).toBe(false);
    expect(app.states.at(-1)?.tasks.map((value) => value.id)).toEqual(["t1", "t2"]);
    expect(app.resizes.filter((resize) => resize.open).length).toBe(2);
  });
});

describe("per-task action errors", () => {
  it("keeps the failed task's bubble visible with its own message", async () => {
    const app = harness();
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.setTasks([task({ id: "t1" })]);
    await app.bubbles.refresh();
    app.failActions("任务状态已改变，请刷新后重试");
    await app.bubbles.act("stop", task({ id: "t1" }));
    expect(app.states.at(-1)?.actionErrors.t1).toContain("任务状态已改变");
    // 列表读取失败与单任务动作失败分开存放，环级提示不会被污染。
    expect(app.states.at(-1)?.error).toBe("");
    // 动作失败时即使指针已经离开，也不该把环收掉——否则用户看不到失败原因。
    app.bubbles.leave();
    await vi.advanceTimersByTimeAsync(2000);
    expect(app.states.at(-1)?.leaving).toBe(false);
  });

  it("clears the failure before retrying the same task", async () => {
    const app = harness();
    const entering = app.bubbles.enter();
    await tick();
    await entering;
    app.setTasks([task({ id: "t1" })]);
    app.failActions("boom");
    await app.bubbles.act("stop", task({ id: "t1" }));
    expect(app.states.at(-1)?.actionErrors.t1).toBe("boom");
    app.failActions(null);
    await app.bubbles.act("stop", task({ id: "t1" }));
    expect(app.states.at(-1)?.actionErrors.t1).toBeUndefined();
    expect(app.actions).toEqual([{ id: "t1" }, { id: "t1" }]);
  });
});
