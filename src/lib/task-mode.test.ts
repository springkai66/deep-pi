import { describe, expect, it, vi } from "vitest";
import type { Task } from "./task";
import { createTaskModeSwitcher } from "./task-mode";

function task(): Task {
  return {
    id: "task", runId: "old-run", agent: "pi", interactionMode: "tui", status: "running",
    title: "Test", projectId: "project", projectPath: "F:/project", sessionId: "session",
    sessionFile: null, executionTarget: "local", createdAt: 1, startedAt: 1, completedAt: null, archivedAt: null,
  };
}

describe("task interaction mode switch", () => {
  it("waits for the old process before restarting and preserves session identity", async () => {
    const source = task();
    let finishStop!: () => void;
    const restart = vi.fn(async () => ({ ...source, runId: "new-run", interactionMode: "rpc" as const }));
    const switcher = createTaskModeSwitcher({
      confirm: async () => true,
      stop: () => new Promise<void>((resolve) => { finishStop = resolve; }),
      restart,
    });
    const pending = switcher.switch(source, "rpc");
    await Promise.resolve();
    expect(restart).not.toHaveBeenCalled();
    expect(switcher.isBusy(source.id)).toBe(true);
    finishStop();
    const result = await pending;
    expect(result?.sessionId).toBe("session");
    expect(result?.runId).toBe("new-run");
    expect(source.status).toBe("cancelled");
    expect(switcher.isBusy(source.id)).toBe(false);
  });

  it("does not restart after stop failure", async () => {
    const restart = vi.fn();
    const switcher = createTaskModeSwitcher({
      confirm: async () => true, stop: async () => { throw new Error("still stopping"); }, restart,
    });
    await expect(switcher.switch(task(), "rpc")).rejects.toThrow("still stopping");
    expect(restart).not.toHaveBeenCalled();
    expect(switcher.isBusy("task")).toBe(false);
  });

  it("suppresses duplicate clicks and cancels without stopping", async () => {
    let confirm!: (value: boolean) => void;
    const stop = vi.fn();
    const switcher = createTaskModeSwitcher({
      confirm: () => new Promise<boolean>((resolve) => { confirm = resolve; }), stop, restart: vi.fn(),
    });
    const source = task();
    const pending = switcher.switch(source, "rpc");
    expect(await switcher.switch(source, "rpc")).toBeNull();
    confirm(false);
    expect(await pending).toBeNull();
    expect(stop).not.toHaveBeenCalled();
  });

  it("rejects a task restarted while its confirmation was open", async () => {
    const source = task();
    const stop = vi.fn();
    const switcher = createTaskModeSwitcher({
      confirm: async () => { source.runId = "replacement"; return true; }, stop, restart: vi.fn(),
    });
    await expect(switcher.switch(source, "rpc")).rejects.toThrow();
    expect(stop).not.toHaveBeenCalled();
  });

  it("can switch back from RPC and checks cleanup even after a cancelled status", async () => {
    const source = { ...task(), interactionMode: "rpc" as const, status: "cancelled" as const };
    const stop = vi.fn(async () => {});
    const restart = vi.fn(async () => ({ ...source, interactionMode: "tui" as const, runId: "tui-run" }));
    const switcher = createTaskModeSwitcher({ confirm: async () => true, stop, restart });
    const result = await switcher.switch(source, "tui");
    expect(stop).toHaveBeenCalledOnce();
    expect(result?.interactionMode).toBe("tui");
  });

  it("releases the switch lock when restart fails and keeps the stopped state", async () => {
    const source = task();
    const switcher = createTaskModeSwitcher({
      confirm: async () => true, stop: async () => {}, restart: async () => { throw new Error("runtime missing"); },
    });
    await expect(switcher.switch(source, "rpc")).rejects.toThrow("runtime missing");
    expect(source.status).toBe("cancelled");
    expect(switcher.isBusy(source.id)).toBe(false);
  });
});
