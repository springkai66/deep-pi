import { describe, expect, it, vi } from "vitest";
import { isUnknownPromptOutcome, recoverRpcSession } from "./rpc-recovery";

describe("RPC session recovery", () => {
  function actions() {
    return {
      stillCurrent: vi.fn(() => true),
      probe: vi.fn(async (_runId: string) => true),
      resubscribe: vi.fn(),
      restart: vi.fn(async () => "run-b" as string | null),
    };
  }

  it("resubscribes to a live run without restarting or submitting a prompt", async () => {
    const ops = actions();
    expect(await recoverRpcSession("run-a", ops)).toBe("resubscribed");
    expect(ops.probe).toHaveBeenCalledWith("run-a");
    expect(ops.resubscribe).toHaveBeenCalledOnce();
    expect(ops.restart).not.toHaveBeenCalled();
  });

  it("starts the same task's replacement run only after the old one exits", async () => {
    const ops = actions();
    ops.probe.mockResolvedValue(false);
    expect(await recoverRpcSession("run-a", ops)).toBe("restarted");
    expect(ops.restart).toHaveBeenCalledOnce();
    expect(ops.resubscribe).not.toHaveBeenCalled();
  });

  it("does not touch a newer run when the old probe races with a restart", async () => {
    const ops = actions();
    ops.stillCurrent.mockReturnValueOnce(true).mockReturnValue(false);
    await expect(recoverRpcSession("run-a", ops)).rejects.toThrow("run changed");
    expect(ops.restart).not.toHaveBeenCalled();
    expect(ops.resubscribe).not.toHaveBeenCalled();
  });

  it("does not restart when a probe rejects a stale run", async () => {
    const ops = actions();
    ops.probe.mockRejectedValue(new Error("RPC run is no longer current"));
    await expect(recoverRpcSession("run-a", ops)).rejects.toThrow("no longer current");
    expect(ops.restart).not.toHaveBeenCalled();
  });

  it("restarts a dormant run without probing a non-existent process", async () => {
    const ops = actions();
    expect(await recoverRpcSession(null, ops)).toBe("restarted");
    expect(ops.probe).not.toHaveBeenCalled();
  });

  it("keeps a failed or unconfirmed restart from being reported as recovered", async () => {
    const ops = actions();
    ops.probe.mockResolvedValue(false);
    ops.restart.mockResolvedValue("run-a");
    await expect(recoverRpcSession("run-a", ops)).rejects.toThrow("could not be restarted");
    ops.restart.mockResolvedValue(null);
    await expect(recoverRpcSession("run-a", ops)).rejects.toThrow("could not be restarted");
  });

  it("does not mistake model errors for an unknown prompt acknowledgement", () => {
    expect(isUnknownPromptOutcome("RPC_OUTCOME_UNKNOWN: RPC request timed out")).toBe(true);
    expect(isUnknownPromptOutcome("terminated")).toBe(false);
    expect(isUnknownPromptOutcome("WebSocket error")).toBe(false);
  });
});
