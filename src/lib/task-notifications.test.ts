import { describe, expect, it } from "vitest";
import type { Task, TaskStatus } from "./task";
import {
  createTaskExitNotifier,
  type TaskExitNotifierDeps,
  resolveTaskExitNotice,
  taskExitNoticeBody,
} from "./task-notifications";

function task(overrides: Partial<Task> & { id: string }): Task {
  return {
    runId: "run-1",
    title: `任务 ${overrides.id}`,
    agent: "pi",
    status: "running",
    projectId: null,
    projectPath: "C:/proj",
    sessionId: "s",
    sessionFile: null,
    executionTarget: "local",
    createdAt: 0,
    startedAt: null,
    completedAt: null,
    archivedAt: null,
    ...overrides,
  } as Task;
}

describe("resolveTaskExitNotice", () => {
  const tasks = [
    task({ id: "t1", runId: "run-1", status: "completed", title: "写周报" }),
    task({ id: "t2", runId: "run-2", status: "cancelled" }),
  ];

  it("notifies completed and failed tasks with title", () => {
    const notice = resolveTaskExitNotice({ taskId: "t1", runId: "run-1" }, tasks);
    expect(notice).toEqual({ taskId: "t1", runId: "run-1", title: "写周报", status: "completed" });
    const failed = resolveTaskExitNotice(
      { taskId: "t1", runId: "run-1", status: "failed" },
      [task({ id: "t1", runId: "run-1", status: "failed" })],
    );
    expect(failed?.status).toBe("failed");
  });

  it("stays silent for cancelled tasks and unknown tasks", () => {
    expect(resolveTaskExitNotice({ taskId: "t2", runId: "run-2", status: "cancelled" }, tasks)).toBeNull();
    expect(resolveTaskExitNotice({ taskId: "ghost", runId: "run-9" }, tasks)).toBeNull();
  });

  it("ignores stale events from an older run", () => {
    expect(
      resolveTaskExitNotice(
        { taskId: "t1", runId: "run-0", status: "completed" },
        tasks,
      ),
    ).toBeNull();
  });

  it("keeps resolving for new runs of the same task", () => {
    expect(resolveTaskExitNotice({ taskId: "t1", runId: "run-1" }, tasks)).not.toBeNull();
    expect(
      resolveTaskExitNotice(
        { taskId: "t1", runId: "run-2", status: "completed" },
        [task({ id: "t1", runId: "run-2", status: "completed" })],
      ),
    ).not.toBeNull();
  });

  it("renders localized bodies per status", () => {
    expect(taskExitNoticeBody("completed", (text) => text)).toBe("任务已完成");
    expect(taskExitNoticeBody("failed", (text) => text)).toBe("任务失败");
  });
});

describe("createTaskExitNotifier", () => {
  function makeDeps(overrides: Partial<TaskExitNotifierDeps> = {}): {
    sent: Array<{ title: string; body: string }>;
    deps: TaskExitNotifierDeps;
  } {
    const sent: Array<{ title: string; body: string }> = [];
    const deps: TaskExitNotifierDeps = {
      fetchTasks: async () => [task({ id: "t1", runId: "run-1", status: "completed", title: "写周报" })],
      isEnabled: () => true,
      bodyFor: (status: TaskStatus) => (status === "completed" ? "done" : "failed"),
      send: async (input: { title: string; body: string }) => {
        sent.push(input);
      },
      ...overrides,
    };
    return { sent, deps };
  }

  it("sends a notification for a completed pty exit", async () => {
    const { sent, deps } = makeDeps();
    const notifier = createTaskExitNotifier(deps);
    await notifier.handlePtyExit({ taskId: "t1", runId: "run-1", exitCode: 0, error: null });
    expect(sent).toEqual([{ title: "写周报", body: "done" }]);
  });

  it("does nothing when the setting is off", async () => {
    const { sent, deps } = makeDeps({ isEnabled: () => false });
    const notifier = createTaskExitNotifier(deps);
    await notifier.handleRpcExit({ taskId: "t1", runId: "run-1", status: "completed" });
    expect(sent).toEqual([]);
  });

  it("swallows fetch and send failures", async () => {
    const { sent } = makeDeps();
    const notifier = createTaskExitNotifier({
      fetchTasks: async () => {
        throw new Error("boom");
      },
      isEnabled: () => true,
      bodyFor: () => "done",
      send: async () => {
        throw new Error("no permission");
      },
    });
    await notifier.handlePtyExit({ taskId: "t1", runId: "run-1", exitCode: 0, error: null });
    expect(sent).toEqual([]);
  });

  it("notifies at most once per run across both channels", async () => {
    const { sent, deps } = makeDeps();
    const notifier = createTaskExitNotifier(deps);
    await notifier.handlePtyExit({ taskId: "t1", runId: "run-1", exitCode: 0, error: null });
    await notifier.handleRpcExit({ taskId: "t1", runId: "run-1", status: "completed" });
    expect(sent).toHaveLength(1);
  });
});
