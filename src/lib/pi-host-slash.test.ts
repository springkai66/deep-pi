import { describe, expect, it, vi } from "vitest";
import { runSensitiveHostSlash, type SensitiveHostSlashPorts } from "./pi-host-slash";
import { runHostSlash } from "./slash-commands";
import type { Task } from "./task";

const task: Task = {
  id: "task", runId: "run", title: "Test", agent: "pi", status: "running",
  projectId: "project", projectPath: "C:/project", sessionId: "old", sessionFile: null,
  executionTarget: "local", interactionMode: "rpc", piEnvironment: "managed",
  createdAt: 0, startedAt: 0, completedAt: null, archivedAt: null,
};

function harness() {
  const events: string[] = [];
  const invokeMock = vi.fn(async (command: string, options?: Record<string, unknown>): Promise<any> => {
    events.push(command);
    if (command === "pi_slash_import") return options?.confirmed ? { cancelled: false } : { sessionId: "new" };
    if (command === "pi_slash_share") return "https://gist.github.com/example";
    if (command === "pi_slash_config") return options?.action === "get_trust" ? null : true;
    if (command === "pi_auth_providers") return [{ id: "oauth", oauth: true, apiKey: false }, { id: "api-only", oauth: false, apiKey: true }, { id: "unsupported", oauth: false, apiKey: false }];
    if (command === "pi_auth_status") return { credentials: [{ provider: "oauth" }] };
    if (command === "rpc_command") return { isStreaming: false, isCompacting: false };
    return null;
  });
  const confirm = vi.fn(async () => true);
  const dialog = vi.fn(async () => "trust" as string | boolean | null);
  const chooseImport = vi.fn(async () => "C:/project/import.jsonl");
  const reportError = vi.fn();
  const openLogin = vi.fn();
  const notifyModelsChanged = vi.fn();
  const restart = vi.fn(async (current: Task) => { current.runId = "next-run"; });
  const quit = vi.fn(async () => true);
  const ports: SensitiveHostSlashPorts = {
    invoke: invokeMock as unknown as SensitiveHostSlashPorts["invoke"],
    confirm, dialog, chooseImport, reportError, openLogin, notifyModelsChanged, restart, quit,
  };
  return { ports, events, invokeMock, confirm, dialog, chooseImport, reportError, openLogin, notifyModelsChanged, restart, quit };
}

describe("sensitive host slash commands", () => {
  it("previews an import, confirms its ID, then switches; cancellation and missing run have no side effect", async () => {
    const h = harness();
    expect(await runSensitiveHostSlash(task, "import", "", h.ports)).toBe(true);
    expect(h.events).toEqual(["pi_slash_import", "pi_slash_import"]);
    expect(h.invokeMock.mock.calls[1]?.[1]).toMatchObject({ confirmed: true, expectedSessionId: "new" });
    const cancelled = harness();
    cancelled.confirm.mockResolvedValue(false);
    expect(await runSensitiveHostSlash(task, "import", "", cancelled.ports)).toBe(false);
    expect(cancelled.invokeMock).toHaveBeenCalledTimes(1);
    const noFile = harness();
    noFile.chooseImport.mockResolvedValue(null as never);
    expect(await runSensitiveHostSlash(task, "import", "", noFile.ports)).toBe(false);
    expect(noFile.invokeMock).not.toHaveBeenCalled();
    const noRun = harness();
    expect(await runSensitiveHostSlash({ ...task, runId: null }, "import", "", noRun.ports)).toBe(false);
    expect(noRun.reportError).toHaveBeenCalledOnce();
  });

  it("does not publish on cancelled share, reports success URL, and propagates publish failure", async () => {
    const h = harness();
    h.confirm.mockResolvedValue(false);
    expect(await runSensitiveHostSlash(task, "share", "", h.ports)).toBe(false);
    expect(h.invokeMock).not.toHaveBeenCalled();
    h.confirm.mockResolvedValue(true);
    expect(await runSensitiveHostSlash(task, "share", "", h.ports)).toBe(true);
    expect(h.dialog).toHaveBeenCalledWith(expect.objectContaining({ kind: "alert", message: "https://gist.github.com/example" }));
    h.invokeMock.mockRejectedValueOnce(new Error("publish failed"));
    const errors: unknown[] = [];
    expect(await runHostSlash(() => runSensitiveHostSlash(task, "share", "", h.ports), (error) => errors.push(error))).toBe("blocked");
    expect((errors[0] as Error).message).toBe("publish failed");
  });

  it("only writes trust after choice and confirmation", async () => {
    const h = harness();
    h.dialog.mockResolvedValueOnce(null);
    expect(await runSensitiveHostSlash(task, "trust", "", h.ports)).toBe(false);
    expect(h.events).toEqual(["pi_slash_config"]);
    const denied = harness();
    denied.confirm.mockResolvedValue(false);
    expect(await runSensitiveHostSlash(task, "trust", "", denied.ports)).toBe(false);
    expect(denied.events).toEqual(["pi_slash_config"]);
    const saved = harness();
    expect(await runSensitiveHostSlash(task, "trust", "", saved.ports)).toBe(true);
    expect(saved.invokeMock.mock.calls[1]?.[1]).toMatchObject({ action: "set_trust", payload: true });
  });

  it("validates login provider and opens the settings flow without using credentials", async () => {
    const h = harness();
    expect(await runSensitiveHostSlash(task, "login", "unsupported", h.ports)).toBe(false);
    expect(h.reportError).toHaveBeenCalledOnce();
    expect(h.openLogin).not.toHaveBeenCalled();
    expect(await runSensitiveHostSlash(task, "login", "api-only", h.ports)).toBe(true);
    expect(h.openLogin).toHaveBeenCalledWith("api-only");
    expect(await runSensitiveHostSlash(task, "login", "oauth", h.ports)).toBe(true);
    expect(h.openLogin).toHaveBeenCalledWith("oauth");
    expect(await runSensitiveHostSlash(task, "login", "", h.ports)).toBe(true);
    expect(h.openLogin).toHaveBeenLastCalledWith(null);
  });

  it("does not remove credentials without selection and confirmation; refreshes models after removal", async () => {
    const h = harness();
    h.dialog.mockResolvedValueOnce(null);
    expect(await runSensitiveHostSlash(task, "logout", "", h.ports)).toBe(false);
    expect(h.events).toEqual(["pi_auth_status"]);
    const cancelled = harness();
    cancelled.dialog.mockResolvedValueOnce("oauth");
    cancelled.confirm.mockResolvedValue(false);
    expect(await runSensitiveHostSlash(task, "logout", "", cancelled.ports)).toBe(false);
    expect(cancelled.events).toEqual(["pi_auth_status"]);
    const done = harness();
    expect(await runSensitiveHostSlash(task, "logout", "oauth", done.ports)).toBe(true);
    expect(done.invokeMock.mock.calls[1]?.[1]).toEqual({ request: { providerId: "oauth" } });
    expect(done.notifyModelsChanged).toHaveBeenCalledOnce();
    const invalid = harness();
    expect(await runSensitiveHostSlash(task, "logout", "other", invalid.ports)).toBe(false);
    expect(invalid.events).toEqual(["pi_auth_status"]);
  });

  it("refuses reload during streaming and restarts after idle state", async () => {
    const busy = harness();
    busy.invokeMock.mockImplementationOnce(async () => { busy.events.push("rpc_command"); return { isStreaming: true, isCompacting: false }; });
    expect(await runSensitiveHostSlash(task, "reload", "", busy.ports)).toBe(false);
    expect(busy.events).toEqual(["rpc_command"]);
    expect(busy.restart).not.toHaveBeenCalled();
    const idle = harness();
    const current = { ...task };
    expect(await runSensitiveHostSlash(current, "reload", "", idle.ports)).toBe(true);
    expect(idle.events).toEqual(["rpc_command", "stop_rpc_task"]);
    expect(idle.restart).toHaveBeenCalledOnce();
  });

  it("only requests application quit when confirmed and available", async () => {
    const h = harness();
    h.confirm.mockResolvedValue(false);
    expect(await runSensitiveHostSlash(task, "quit", "", h.ports)).toBe(false);
    expect(h.quit).not.toHaveBeenCalled();
    h.confirm.mockResolvedValue(true);
    expect(await runSensitiveHostSlash(task, "quit", "", h.ports)).toBe(true);
    expect(h.quit).toHaveBeenCalledOnce();
    expect(await runSensitiveHostSlash(task, "quit", "", { ...h.ports, quit: null })).toBe(false);
  });
});
