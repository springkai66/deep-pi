import { describe, expect, it, vi } from "vitest";
import { createGitIndexWriter, indexActionForArea } from "./git-index";
import type { GitEntry } from "./git-status";

const entry: GitEntry = { path: "file.txt", originalPath: null, sourceOutsideProject: false, indexStatus: ".", worktreeStatus: "M", kind: "tracked" };

describe("Git index writes", () => {
  it("separates stage, unstage and explicit conflict resolution", () => {
    expect(indexActionForArea("staged")).toBe("unstage");
    expect(indexActionForArea("unstaged")).toBe("stage");
    expect(indexActionForArea("untracked")).toBe("stage");
    expect(indexActionForArea("conflict")).toBe("resolve");
  });

  it("does not queue duplicate clicks and refreshes only the applied project", async () => {
    let finish!: (value: boolean) => void;
    const write = vi.fn(() => new Promise<boolean>((resolve) => { finish = resolve; }));
    const changed = vi.fn();
    const applied = vi.fn();
    const writer = createGitIndexWriter(write, changed, applied);
    const first = writer.run("first", entry, "unstaged");
    await writer.run("second", entry, "staged");
    expect(write).toHaveBeenCalledTimes(1);
    expect(write.mock.calls[0]).toEqual(["first", { entry, action: "stage" }, expect.any(String)]);
    finish(true);
    await first;
    expect(applied).toHaveBeenCalledExactlyOnceWith("first", "file.txt");
    expect(changed).toHaveBeenLastCalledWith({ busy: false, projectId: "first", error: "" });
  });

  it("allows manual retry after failure but never retries writes automatically", async () => {
    const write = vi.fn().mockRejectedValueOnce(new Error("index locked")).mockResolvedValue(false);
    const changed = vi.fn();
    const applied = vi.fn();
    const writer = createGitIndexWriter(write, changed, applied);
    await writer.run("project", entry, "unstaged");
    expect(write).toHaveBeenCalledTimes(1);
    expect(changed.mock.lastCall?.[0]).toMatchObject({ busy: false, error: expect.stringContaining("index locked") });
    await writer.run("project", entry, "unstaged");
    expect(write).toHaveBeenCalledTimes(2);
    expect(write.mock.calls[0][2]).not.toBe(write.mock.calls[1][2]);
    expect(applied).not.toHaveBeenCalled();
  });

  it("does not notify destroyed views or start writes after disposal", async () => {
    let finish!: (value: boolean) => void;
    const write = vi.fn(() => new Promise<boolean>((resolve) => { finish = resolve; }));
    const changed = vi.fn();
    const applied = vi.fn();
    const writer = createGitIndexWriter(write, changed, applied);
    const running = writer.run("project", entry, "unstaged");
    writer.dispose();
    changed.mockClear();
    finish(true);
    await running;
    await writer.run("project", entry, "unstaged");
    expect(write).toHaveBeenCalledTimes(1);
    expect(changed).not.toHaveBeenCalled();
    expect(applied).not.toHaveBeenCalled();
  });
});
