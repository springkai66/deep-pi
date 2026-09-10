import { describe, expect, it } from "vitest";
import { createGitStatusLoader, groupGitEntries, type GitEntry } from "./git-status";

describe("Git status", () => {
  it("cancels obsolete in-flight reads once and waits for cleanup", async () => {
    const calls: string[] = [];
    const cancelled: string[] = [];
    const finish: Array<(value: string) => void> = [];
    const loader = createGitStatusLoader(
      (input: string, id: string) => {
        calls.push(id);
        return new Promise<string>((resolve) => finish.push(resolve));
      },
      () => {},
      async (id) => { cancelled.push(id); },
    );
    loader.load("first");
    loader.load("second");
    loader.load("third");
    expect(cancelled).toEqual([calls[0]]);
    expect(calls).toHaveLength(1);
    finish[0]("old");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(calls).toHaveLength(2);
    expect(calls[1]).not.toBe(calls[0]);
    loader.cancel();
    expect(cancelled).toEqual(calls);
    finish[1]("late");
    loader.dispose();
  });

  it("publishes cancellation and rejects late results even when cancellation IPC fails", async () => {
    let finish!: (value: string) => void;
    const values: string[] = [];
    const errors: string[] = [];
    const loader = createGitStatusLoader(
      (_input: string) => new Promise<string>((resolve) => { finish = resolve; }),
      (state) => { if (state.value) values.push(state.value); if (state.error) errors.push(state.error); },
      async () => { throw new Error("transport disconnected"); },
    );
    loader.load("first");
    loader.cancel();
    finish("late");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(values).toEqual([]);
    expect(errors).toContain("已取消读取");
    expect(errors.some((error) => error.includes("transport disconnected"))).toBe(true);
  });

  it("shows mixed changes in both groups without duplicating conflicts", () => {
    const entries: GitEntry[] = [
      { path: "a", originalPath: null, kind: "tracked", indexStatus: "M", worktreeStatus: "M" },
      { path: "b", originalPath: null, kind: "conflict", indexStatus: "U", worktreeStatus: "U" },
      { path: "c", originalPath: null, kind: "untracked", indexStatus: "?", worktreeStatus: "?" },
    ];
    const groups = groupGitEntries(entries);
    expect(groups.map((group) => group.entries.map((entry) => entry.path))).toEqual([["b"], ["a"], ["a"], ["c"]]);
  });

  it("serializes refreshes and only publishes the newest request", async () => {
    const resolvers: Array<(value: string) => void> = [];
    const calls: string[] = [];
    const results: string[] = [];
    const loader = createGitStatusLoader(
      (id) => { calls.push(id); return new Promise<string>((resolve) => resolvers.push(resolve)); },
      (state) => { if (state.value) results.push(state.value); },
    );
    loader.load("a");
    loader.load("b");
    loader.load("c");
    expect(calls).toEqual(["a"]);
    resolvers[0]("old");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(calls).toEqual(["a", "c"]);
    resolvers[1]("current");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(results).toEqual(["current"]);
  });

  it("invalidates in-flight results on disposal", async () => {
    let finish!: (value: string) => void;
    const values: string[] = [];
    const loader = createGitStatusLoader(
      () => new Promise<string>((resolve) => { finish = resolve; }),
      (state) => { if (state.value) values.push(state.value); },
    );
    loader.load("a");
    loader.dispose();
    finish("late");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(values).toEqual([]);
  });

  it("reports failures and permits a later retry", async () => {
    const errors: string[] = [];
    const values: string[] = [];
    let attempts = 0;
    const loader = createGitStatusLoader(
      async () => { if (++attempts === 1) throw new Error("index locked"); return "recovered"; },
      (state) => { if (state.error) errors.push(state.error); if (state.value) values.push(state.value); },
    );
    loader.load("a");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(errors).toEqual(["Error: index locked"]);
    loader.load("a");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(values).toEqual(["recovered"]);
  });

  it("invalidates a queued project without disposing the loader", async () => {
    let finish!: (value: string) => void;
    const calls: string[] = [];
    const values: string[] = [];
    const loader = createGitStatusLoader(
      (id) => { calls.push(id); return new Promise<string>((resolve) => { finish = resolve; }); },
      (state) => { if (state.value) values.push(state.value); },
    );
    loader.load("a");
    loader.load("b");
    loader.invalidate();
    finish("old");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(calls).toEqual(["a"]);
    expect(values).toEqual([]);
    loader.load("c");
    finish("fresh");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(values).toEqual(["fresh"]);
  });
});
