import { describe, expect, it, vi } from "vitest";
import { createTerminalInput } from "./terminal-input";

describe("terminal input queue", () => {
  it("serializes input and drops queued data after a run switch", async () => {
    let run = "first";
    let finish!: () => void;
    const write = vi.fn().mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; })).mockResolvedValue(undefined);
    const queue = createTerminalInput({ currentRun: () => run, write, error: vi.fn() });
    queue.send("a");
    queue.send("old");
    expect(write).toHaveBeenCalledTimes(1);
    run = "next";
    queue.send("new");
    finish();
    await Promise.resolve();
    await Promise.resolve();
    expect(write.mock.calls).toEqual([["first", "a"], ["next", "new"]]);
  });
  it("bounds bytes including in-flight UTF-8 and does not write after disposal", async () => {
    let finish!: () => void;
    const write = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
    const error = vi.fn();
    const queue = createTerminalInput({ currentRun: () => "run", write, error }, 5);
    queue.send("中");
    queue.send("文");
    expect(error).toHaveBeenCalledOnce();
    queue.send("a");
    queue.dispose();
    finish();
    await Promise.resolve();
    queue.send("later");
    expect(write).toHaveBeenCalledOnce();
  });
  it("discards the queued suffix on failure and reports the error once", async () => {
    let fail!: (cause: unknown) => void;
    const write = vi.fn(() => new Promise<void>((_, reject) => { fail = reject; }));
    const error = vi.fn();
    const queue = createTerminalInput({ currentRun: () => "run", write, error });
    queue.send("first");
    queue.send("second");
    fail(new Error("closed"));
    await Promise.resolve();
    expect(write).toHaveBeenCalledOnce();
    expect(error).toHaveBeenCalledOnce();
  });
});
