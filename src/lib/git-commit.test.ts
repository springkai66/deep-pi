import { describe, expect, it, vi } from "vitest";
import { createCommitController, validCommitMessage, type CommitPorts, type CommitPreview, type CommitResult, type CommitState } from "./git-commit";

const preview: CommitPreview = { reference: "refs/heads/main", head: null, tree: "a".repeat(40), paths: ["file.txt"], author: "Test <test@localhost>", committer: "Test <test@localhost>", sign: false };

function setup(
  prepare: CommitPorts["prepare"] = vi.fn(async () => preview),
  commit: CommitPorts["commit"] = vi.fn(async () => ({ outcome: "committed" as const, oid: "b".repeat(40), detail: "" })),
) {
  let state!: CommitState;
  const applied = vi.fn();
  const controller = createCommitController({ prepare, commit }, (next) => { state = next; }, applied);
  controller.setProject("first");
  return { controller, prepare, commit, applied, state: () => state };
}

describe("commit workflow", () => {
  it("requires a nonempty bounded message without NUL", () => {
    expect(validCommitMessage(" \n ")).toBe(false);
    expect(validCommitMessage("bad\0message")).toBe(false);
    expect(validCommitMessage("中".repeat(30000))).toBe(false);
    expect(validCommitMessage("首次提交\n\n详细说明")).toBe(true);
  });

  it("commits only a prepared snapshot and clears only the successful draft", async () => {
    const test = setup();
    test.controller.setMessage("selected scope");
    await test.controller.submit();
    expect(test.commit).not.toHaveBeenCalled();
    await test.controller.prepare();
    await test.controller.submit();
    expect(test.commit).toHaveBeenCalledWith("first", preview, "selected scope", expect.any(String));
    expect(test.applied).toHaveBeenCalledExactlyOnceWith("first");
    expect(test.state().message).toBe("");
    expect(test.state().result?.outcome).toBe("committed");
  });

  it("ignores a stale preparation and keeps drafts separate across projects", async () => {
    let resolve!: (value: CommitPreview) => void;
    const test = setup(vi.fn(() => new Promise<CommitPreview>((done) => { resolve = done; })));
    test.controller.setMessage("first draft");
    const pending = test.controller.prepare();
    test.controller.setProject("second");
    test.controller.setMessage("second draft");
    resolve(preview);
    await pending;
    expect(test.state().preview).toBeNull();
    expect(test.state().message).toBe("second draft");
    test.controller.setProject("first");
    expect(test.state().message).toBe("first draft");
  });

  it("never queues duplicate submissions and preserves unconfirmed messages", async () => {
    let resolve!: (value: CommitResult | null) => void;
    const commit = vi.fn(() => new Promise<CommitResult | null>((done) => { resolve = done; }));
    const test = setup(undefined, commit);
    test.controller.setMessage("keep this");
    await test.controller.prepare();
    const pending = test.controller.submit();
    await test.controller.submit();
    expect(commit).toHaveBeenCalledTimes(1);
    resolve({ outcome: "unknown", oid: "b".repeat(40), detail: "verify reference" });
    await pending;
    expect(test.state().message).toBe("keep this");
    expect(test.state().preview).toBeNull();
    expect(test.applied).not.toHaveBeenCalled();
  });

  it("does not clear the draft or retry after cancelled native confirmation", async () => {
    const commit = vi.fn(async () => null);
    const test = setup(undefined, commit);
    test.controller.setMessage("pending consent");
    await test.controller.prepare();
    await test.controller.submit();
    expect(test.state().message).toBe("pending consent");
    expect(test.state().preview).toEqual(preview);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(test.applied).not.toHaveBeenCalled();
  });

  it("requires a fresh preview after invalidation and ignores disposed results", async () => {
    const test = setup();
    test.controller.setMessage("message");
    await test.controller.prepare();
    test.controller.invalidate();
    await test.controller.submit();
    expect(test.commit).not.toHaveBeenCalled();
    test.controller.dispose();
    await test.controller.prepare();
    expect(test.prepare).toHaveBeenCalledTimes(1);
  });
});
