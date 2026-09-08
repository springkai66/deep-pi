import { describe, expect, it, vi } from "vitest";
import { createTerminalSession, type TerminalPorts } from "./terminal-session";

function fixture() {
  const handlers = new Map<string, (payload: unknown) => void>();
  const listeners: Array<() => void> = [];
  const unlisten = vi.fn();
  const ports: TerminalPorts = {
    listen: (name, handler) => {
      handlers.set(name, handler as (payload: unknown) => void);
      return new Promise((resolve) => { listeners.push(() => resolve(unlisten)); });
    },
    acknowledge: vi.fn().mockResolvedValue(undefined),
    output: vi.fn(),
    exit: vi.fn(),
    error: vi.fn(),
  };
  const session = createTerminalSession("task", ports);
  return { session, ports, handlers, unlisten, ready: async () => {
    listeners.forEach((resolve) => resolve());
    await Promise.resolve();
    await Promise.resolve();
  } };
}

describe("terminal session", () => {
  it("acknowledges only after both subscriptions are ready", async () => {
    const test = fixture();
    test.session.setRun("run");
    expect(test.ports.acknowledge).not.toHaveBeenCalled();
    await test.ready();
    expect(test.ports.acknowledge).toHaveBeenCalledWith("task", "run");
  });

  it("ignores delayed output and exits from an earlier run", async () => {
    const test = fixture();
    test.session.setRun("old");
    await test.ready();
    test.session.setRun("new");
    test.handlers.get("pty-output")!({ taskId: "task", runId: "old", data: "old" });
    test.handlers.get("pty-exit")!({ taskId: "task", runId: "old", exitCode: 0, error: null });
    expect(test.ports.output).not.toHaveBeenCalled();
    expect(test.ports.exit).not.toHaveBeenCalled();
    test.handlers.get("pty-output")!({ taskId: "task", runId: "new", data: "new" });
    expect(test.ports.output).toHaveBeenCalledWith("new");
  });

  it("cleans up delayed subscriptions without acknowledging a disposed terminal", async () => {
    const test = fixture();
    test.session.setRun("run");
    test.session.dispose();
    await test.ready();
    expect(test.unlisten).toHaveBeenCalledTimes(2);
    expect(test.ports.acknowledge).not.toHaveBeenCalled();
    test.handlers.get("pty-output")!({ taskId: "task", runId: "run", data: "late" });
    expect(test.ports.output).not.toHaveBeenCalled();
  });
});
