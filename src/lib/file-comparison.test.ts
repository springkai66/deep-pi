import { describe, expect, it, vi } from "vitest";
import { comparisonSources, comparisonStale, createFileComparison, type ComparisonState } from "./file-comparison";
import { createFileWorkspace } from "./file-workspace";

async function document() {
  const workspace = createFileWorkspace({
    read: async (_id, path) => ({ path, content: "draft\r\n", size: 7, version: "base" }),
    save: vi.fn(), chooseClose: async () => null, changed: () => {}, saved: () => {},
  });
  const id = await workspace.open("p", "src/a.txt");
  return workspace.get(id)!;
}

function fixture() {
  let state: ComparisonState = { status: "idle", snapshot: null, error: "" };
  const read = vi.fn(async (_project: string, path: string) => ({ path, content: "disk\n", size: 5, version: "disk-v1" }));
  const comparison = createFileComparison(read, (next) => { state = next; });
  return { read, comparison, state: () => state };
}

describe("file comparison", () => {
  it("exposes the current file, attempted target and recovery paths without conflating them", async () => {
    const doc = await document();
    doc.issue = { outcome: "unknown", target: "src/b.txt", recoveryPath: "src/.deeppi-x.recovery",
      pendingPath: "src/.deeppi-x.pending", version: null, detail: "unknown" };
    expect(comparisonSources(doc).map((item) => [item.kind, item.path])).toEqual([
      ["disk", "src/a.txt"], ["target", "src/b.txt"],
      ["recovery", "src/.deeppi-x.recovery"], ["pending", "src/.deeppi-x.pending"],
    ]);
    doc.issue.target = doc.path;
    expect(comparisonSources(doc).filter((source) => source.path === doc.path)).toHaveLength(1);
  });

  it("captures a read-only draft snapshot and marks later editing as stale", async () => {
    const doc = await document();
    const f = fixture();
    await f.comparison.load(doc, "disk");
    const snapshot = f.state().snapshot!;
    expect(snapshot.draft).toBe("draft\r\n");
    expect(snapshot.source.content).toBe("disk\n");
    expect(comparisonStale(snapshot, doc)).toBe(false);
    const next = { ...doc, state: doc.state!.update({ changes: { from: 0, insert: "later" } }).state };
    expect(comparisonStale(snapshot, next)).toBe(true);
    expect(snapshot.draft).toBe("draft\r\n");
    expect(doc.state!.sliceDoc()).toBe("draft\r\n");
    expect(f.read).toHaveBeenCalledWith("p", "src/a.txt");
    const refreshed = { ...doc, disk: { path: doc.path, content: "new disk", size: 8, version: "disk-v2" } };
    expect(comparisonStale(snapshot, refreshed)).toBe(true);
    refreshed.disk.version = snapshot.source.version;
    expect(comparisonStale(snapshot, refreshed)).toBe(false);
  });

  it("ignores earlier requests when a different source is requested", async () => {
    const doc = await document();
    doc.issue = { outcome: "unknown", target: doc.path, recoveryPath: "old.recovery",
      pendingPath: null, version: null, detail: "" };
    const f = fixture();
    let finish!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    f.read.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const first = f.comparison.load(doc, "recovery");
    await f.comparison.load(doc, "disk");
    finish({ path: "old.recovery", content: "old result", size: 10, version: "old" });
    await first;
    expect(f.state().status).toBe("ready");
    expect(f.state().snapshot!.source.path).toBe(doc.path);
    expect(f.state().snapshot!.source.content).toBe("disk\n");
  });

  it("does not resurrect a closed comparison when a read completes", async () => {
    const doc = await document();
    const f = fixture();
    let finish!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    f.read.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const loading = f.comparison.load(doc, "disk");
    f.comparison.reset();
    finish({ path: doc.path, content: "late", size: 4, version: "late" });
    await loading;
    expect(f.state()).toEqual({ status: "idle", snapshot: null, error: "" });
  });

  it("does not fall back to another file when the requested recovery file is missing", async () => {
    const doc = await document();
    doc.issue = { outcome: "unknown", target: doc.path, recoveryPath: "missing.recovery",
      pendingPath: null, version: null, detail: "" };
    const f = fixture();
    f.read.mockRejectedValueOnce(new Error("not found"));
    await f.comparison.load(doc, "recovery");
    expect(f.state().status).toBe("error");
    expect(f.state().snapshot).toBeNull();
    expect(f.read).toHaveBeenCalledTimes(1);
    expect(f.state().error).toContain("not found");
  });

  it("rejects unavailable sources and stale document identity without performing writes", async () => {
    const doc = await document();
    const f = fixture();
    await f.comparison.load(doc, "pending");
    expect(f.state().status).toBe("error");
    expect(f.read).not.toHaveBeenCalled();
    await f.comparison.load(doc, "disk");
    expect(comparisonStale(f.state().snapshot!, { ...doc, version: "changed" })).toBe(true);
    expect(comparisonStale(f.state().snapshot!, { ...doc, id: "other" })).toBe(true);
    expect(comparisonStale(f.state().snapshot!, undefined)).toBe(true);
  });

  it("refuses binary, oversized or mismatched source responses", async () => {
    const doc = await document();
    const f = fixture();
    for (const content of ["a\0b", "x".repeat(2 * 1024 * 1024 + 1)]) {
      f.read.mockResolvedValueOnce({ path: doc.path, content, size: content.length, version: "v" });
      await f.comparison.load(doc, "disk");
      expect(f.state().status).toBe("error");
    }
    f.read.mockResolvedValueOnce({ path: "wrong", content: "", size: 0, version: "v" });
    await f.comparison.load(doc, "disk");
    expect(f.state().status).toBe("error");
  });
});
