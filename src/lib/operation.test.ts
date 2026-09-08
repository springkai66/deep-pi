import { describe, expect, it } from "vitest";
import { createOperationRunner } from "./operation";

describe("operation runner", () => {
  it("holds busy state until the backend finishes restoring after cancellation", async () => {
    let finish!: (value: unknown) => void;
    let current: { id: string; cancelling: boolean } | null = null;
    const runner = createOperationRunner(async (command) => {
      if (command === "cancel_operation") return true;
      return new Promise((resolve) => { finish = resolve; });
    }, (state) => { current = state; });
    const pending = runner.run("install_runtime", {});
    expect(current).not.toBeNull();
    await expect(runner.run("install_runtime", {})).rejects.toThrow("already");
    expect(await runner.cancel()).toBe(true);
    expect(current).toMatchObject({ cancelling: true });
    finish({});
    await pending;
    expect(current).toBeNull();
  });

  it("does not claim cancellation when the commit boundary has passed", async () => {
    let finish!: (value: unknown) => void;
    let current: { id: string; cancelling: boolean } | null = null;
    const runner = createOperationRunner(async (command) => {
      if (command === "cancel_operation") return false;
      return new Promise((resolve) => { finish = resolve; });
    }, (state) => { current = state; });
    const pending = runner.run("package_operation", {});
    expect(await runner.cancel()).toBe(false);
    expect(current).toMatchObject({ cancelling: false });
    finish({});
    await pending;
  });
});
