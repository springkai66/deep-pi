import { describe, expect, it } from "vitest";
import type { Task } from "./task";
import { petStateFromExit, petStateFromTasks } from "./pet-state";

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

describe("petStateFromTasks", () => {
  it("works while any task is running or waiting", () => {
    expect(petStateFromTasks([task({ id: "t1", status: "running" })])).toBe("working");
    expect(petStateFromTasks([task({ id: "t1", status: "waiting" })])).toBe("working");
  });

  it("is idle with only terminal tasks or no tasks", () => {
    expect(petStateFromTasks([task({ id: "t1", status: "completed" })])).toBe("idle");
    expect(petStateFromTasks([])).toBe("idle");
  });
});

describe("petStateFromExit", () => {
  const tasks = [
    task({ id: "t1", runId: "run-1", status: "completed" }),
    task({ id: "t2", runId: "run-2", status: "failed" }),
  ];

  it("celebrates completed and mourns failed runs", () => {
    expect(petStateFromExit({ taskId: "t1", runId: "run-1", status: "completed" }, tasks)).toBe("celebrate");
    expect(petStateFromExit({ taskId: "t2", runId: "run-2", status: "failed" }, tasks)).toBe("sad");
  });

  it("stays put for cancels, stale runs and unknown tasks", () => {
    expect(petStateFromExit({ taskId: "t1", runId: "run-1", status: "cancelled" }, tasks)).toBeNull();
    expect(petStateFromExit({ taskId: "t1", runId: "run-9", status: "completed" }, tasks)).toBeNull();
    expect(petStateFromExit({ taskId: "ghost", runId: "run-1", status: "completed" }, tasks)).toBeNull();
  });
});
