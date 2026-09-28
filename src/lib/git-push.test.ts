import { describe, expect, it, vi } from "vitest";
import { createPushController, type PushPorts, type PushState, type PushTargets, type PushResult } from "./git-push";

const targets: PushTargets = {
  sourceRef: "refs/heads/main", sourceOid: "a".repeat(40),
  targets: [{ remote: "origin", destination: "https://github.com/example/first.git" },
    { remote: "backup", destination: "git@github.com:example/second.git" }],
};
function setup(overrides: Partial<PushPorts> = {}) {
  const load = vi.fn(async () => targets);
  const push = vi.fn(async (): Promise<PushResult | null> => ({ outcome: "pushed", sourceOid: targets.sourceOid, targetRef: "refs/heads/main", detail: "accepted" }));
  const cancel = vi.fn(async () => {});
  const verify = vi.fn(async () => ({ outcome: "matches" as const, remoteOid: targets.sourceOid, sourceOid: targets.sourceOid, targetRef: "refs/heads/main" }));
  const sync = vi.fn(async () => ({ outcome: "synced" as const, oid: targets.sourceOid, references: ["refs/remotes/origin/main"] }));
  const applied = vi.fn();
  let state!: PushState;
  const controller = createPushController({ load, push, cancel, verify, sync, ...overrides }, (next) => { state = next; }, applied);
  controller.setProject("first");
  return { controller, load, push, cancel, verify, sync, applied, state: () => state };
}

describe("push workflow", () => {
  it("requires an explicit destination and sends exactly one fixed source and target", async () => {
    const test = setup();
    await test.controller.load();
    await test.controller.push();
    expect(test.push).not.toHaveBeenCalled();
    test.controller.selectTarget(1);
    test.controller.setBranch("feature/中文");
    await test.controller.push();
    expect(test.push).toHaveBeenCalledWith("first", {
      ...targets.targets[1], sourceRef: targets.sourceRef, sourceOid: targets.sourceOid, targetRef: "refs/heads/feature/中文",
    }, expect.any(String));
    expect(test.applied).toHaveBeenCalledExactlyOnceWith("first");
  });

  it("ignores stale target lists after switching project", async () => {
    let done!: (value: PushTargets) => void;
    const test = setup({ load: () => new Promise((resolve) => { done = resolve; }) });
    const loading = test.controller.load();
    test.controller.setProject("second");
    done(targets);
    await loading;
    expect(test.state().targets).toBeNull();
    expect(test.state().projectId).toBe("second");
  });

  it("cancels the active operation once and never automatically retries unknown results", async () => {
    let done!: (value: PushResult) => void;
    const push = vi.fn(() => new Promise<PushResult>((resolve) => { done = resolve; }));
    const test = setup({ push });
    await test.controller.load();
    test.controller.selectTarget(0);
    const pending = test.controller.push();
    await test.controller.push();
    await test.controller.cancel();
    await test.controller.cancel();
    expect(test.cancel).toHaveBeenCalledTimes(1);
    expect(push).toHaveBeenCalledTimes(1);
    done({ outcome: "unknown", sourceOid: targets.sourceOid, targetRef: "refs/heads/main", detail: "check remote" });
    await pending;
    expect(test.state().result?.outcome).toBe("unknown");
    expect(test.state().targets).toBeNull();
    expect(test.applied).not.toHaveBeenCalled();
  });

  it("retains selection after native confirmation is cancelled", async () => {
    const test = setup({ push: async () => null });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    expect(test.state().selected).toBe(0);
    expect(test.state().targets).toEqual(targets);
    expect(test.applied).not.toHaveBeenCalled();
  });

  it("invalidates an old source snapshot after refresh and stops work after disposal", async () => {
    const test = setup();
    await test.controller.load();
    test.controller.selectTarget(0);
    test.controller.invalidate();
    await test.controller.push();
    expect(test.push).not.toHaveBeenCalled();
    test.controller.dispose();
    await test.controller.load();
    expect(test.load).toHaveBeenCalledTimes(1);
  });

  it("retains the exact attempted destination for read-only verification after an unknown push", async () => {
    const test = setup({ push: async () => ({ outcome: "unknown", sourceOid: targets.sourceOid, targetRef: "refs/heads/main", detail: "unknown" }) });
    await test.controller.load();
    test.controller.selectTarget(1);
    await test.controller.push();
    test.controller.invalidate();
    await test.controller.verify();
    expect(test.verify).toHaveBeenCalledWith("first", {
      ...targets.targets[1], sourceRef: targets.sourceRef, sourceOid: targets.sourceOid, targetRef: "refs/heads/main",
    }, expect.any(String));
    expect(test.state().verification?.outcome).toBe("matches");
    expect(test.state().result?.outcome).toBe("unknown");
    expect(test.applied).not.toHaveBeenCalled();
  });

  it("can verify after a transport exception without automatically pushing again", async () => {
    const test = setup({ push: async () => { throw new Error("transport disconnected"); } });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    await test.controller.verify();
    expect(test.verify).toHaveBeenCalledTimes(1);
    expect(test.state().verification?.outcome).toBe("matches");
  });

  it("ignores a verification that finishes after a project switch", async () => {
    let done!: (value: { outcome: "missing"; remoteOid: null; sourceOid: string; targetRef: string }) => void;
    const test = setup({ verify: () => new Promise((resolve) => { done = resolve; }) });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    const pending = test.controller.verify();
    test.controller.setProject("second");
    test.controller.setProject("first");
    done({ outcome: "missing", remoteOid: null, sourceOid: targets.sourceOid, targetRef: "refs/heads/main" });
    await pending;
    expect(test.state().verification).toBeNull();
    expect(test.state().attempted).toBeNull();
  });

  it("keeps a historical target verification valid during unrelated local refreshes", async () => {
    let done!: (value: { outcome: "matches"; remoteOid: string; sourceOid: string; targetRef: string }) => void;
    const test = setup({ verify: () => new Promise((resolve) => { done = resolve; }) });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    const pending = test.controller.verify();
    test.controller.invalidate();
    done({ outcome: "matches", remoteOid: targets.sourceOid, sourceOid: targets.sourceOid, targetRef: "refs/heads/main" });
    await pending;
    expect(test.state().verification?.outcome).toBe("matches");
  });

  it("discards verification results after cancellation without changing the push result", async () => {
    let done!: (value: { outcome: "missing"; remoteOid: null; sourceOid: string; targetRef: string }) => void;
    const test = setup({ verify: () => new Promise((resolve) => { done = resolve; }) });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    const pending = test.controller.verify();
    await test.controller.cancel();
    done({ outcome: "missing", remoteOid: null, sourceOid: targets.sourceOid, targetRef: "refs/heads/main" });
    await pending;
    expect(test.state().verification).toBeNull();
    expect(test.state().error).toContain("已取消远程核对");
    expect(test.state().busy).toBe(false);
    expect(test.state().result?.outcome).toBe("pushed");
  });

  it("does not interpret a failed query as a missing branch", async () => {
    const test = setup({ verify: async () => { throw new Error("network unavailable"); } });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    await test.controller.verify();
    expect(test.state().verification).toBeNull();
    expect(test.state().error).toContain("不能据此判断目标状态");
    expect(test.state().result?.outcome).toBe("pushed");
    expect(test.state().busy).toBe(false);
  });

  it("does not restore an old push result after switching away and back", async () => {
    let done!: (value: PushResult) => void;
    const test = setup({ push: () => new Promise((resolve) => { done = resolve; }) });
    await test.controller.load();
    test.controller.selectTarget(0);
    const pending = test.controller.push();
    test.controller.setProject("second");
    test.controller.setProject("first");
    done({ outcome: "pushed", sourceOid: targets.sourceOid, targetRef: "refs/heads/main", detail: "accepted" });
    await pending;
    expect(test.state().result).toBeNull();
    expect(test.state().attempted).toBeNull();
    expect(test.applied).toHaveBeenCalledExactlyOnceWith("first");
  });

  it("synchronizes the attempted target explicitly and refreshes local status without pushing again", async () => {
    const test = setup();
    await test.controller.load();
    test.controller.selectTarget(1);
    await test.controller.push();
    test.applied.mockClear();
    await test.controller.sync();
    expect(test.sync).toHaveBeenCalledWith("first", expect.objectContaining(targets.targets[1]), expect.any(String));
    expect(test.push).toHaveBeenCalledTimes(1);
    expect(test.state().syncResult?.outcome).toBe("synced");
    expect(test.applied).toHaveBeenCalledExactlyOnceWith("first");
    expect(test.state().result?.outcome).toBe("pushed");
  });

  it("keeps sync exclusive and reports the final outcome even after late cancellation", async () => {
    let done!: (value: { outcome: "synced"; oid: string; references: string[] }) => void;
    const sync = vi.fn(() => new Promise<{ outcome: "synced"; oid: string; references: string[] }>((resolve) => { done = resolve; }));
    const test = setup({ sync });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    const pending = test.controller.sync();
    await test.controller.sync();
    await test.controller.verify();
    expect(sync).toHaveBeenCalledTimes(1);
    expect(test.verify).not.toHaveBeenCalled();
    expect(test.state().phase).toBe("syncing");
    await test.controller.cancel();
    done({ outcome: "synced", oid: targets.sourceOid, references: ["refs/remotes/origin/main"] });
    await pending;
    expect(test.state().syncResult?.outcome).toBe("synced");
    expect(test.state().busy).toBe(false);
  });

  it("keeps unknown synchronization distinct and never automatically retries", async () => {
    const test = setup({ sync: async () => ({ outcome: "unknown", oid: targets.sourceOid, references: ["refs/remotes/origin/main"] }) });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    test.applied.mockClear();
    await test.controller.sync();
    expect(test.state().syncResult?.outcome).toBe("unknown");
    expect(test.applied).toHaveBeenCalledExactlyOnceWith("first");
    expect(test.state().busy).toBe(false);
  });

  it("does not clear the push result when the sync confirmation is cancelled", async () => {
    const test = setup({ sync: async () => null });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    test.applied.mockClear();
    await test.controller.sync();
    expect(test.state().syncResult).toBeNull();
    expect(test.state().result?.outcome).toBe("pushed");
    expect(test.applied).not.toHaveBeenCalled();
  });

  it("does not restore sync results after switching away and back", async () => {
    let done!: (value: { outcome: "synced"; oid: string; references: string[] }) => void;
    const test = setup({ sync: () => new Promise((resolve) => { done = resolve; }) });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    const pending = test.controller.sync();
    test.controller.setProject("second");
    test.controller.setProject("first");
    done({ outcome: "synced", oid: targets.sourceOid, references: ["refs/remotes/origin/main"] });
    await pending;
    expect(test.state().syncResult).toBeNull();
    expect(test.state().attempted).toBeNull();
  });

  it("refreshes after a sync transport error without clearing the push result", async () => {
    const sync = vi.fn(async () => { throw new Error("IPC disconnected"); });
    const test = setup({ sync });
    await test.controller.load();
    test.controller.selectTarget(0);
    await test.controller.push();
    test.applied.mockClear();
    await test.controller.sync();
    expect(test.state().syncResult).toBeNull();
    expect(test.state().error).toContain("不会自动重试");
    expect(test.state().result?.outcome).toBe("pushed");
    expect(test.applied).toHaveBeenCalledExactlyOnceWith("first");
    expect(sync).toHaveBeenCalledTimes(1);
    expect(test.state().busy).toBe(false);
  });
});
