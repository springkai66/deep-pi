import { describe, expect, it, vi } from "vitest";
import { createFileWorkspace, documentDirty, type FileDocument } from "./file-workspace";
import { createDocumentState } from "./editor-document";
import { createWindowCloseHandler } from "./window-close";

function fixture() {
  const read = vi.fn(async (_project: string, path: string) => ({
    path, content: "original\r\n", size: 10, version: "v1",
  }));
  const save = vi.fn(async () => ({
    outcome: "saved" as const, version: "v2", recoveryPath: null, pendingPath: null, detail: "saved",
  }));
  const chooseClose = vi.fn(async () => "discard" as "discard" | "save" | null);
  let documents: FileDocument[] = [];
  const workspace = createFileWorkspace({ read, save, chooseClose, changed: (next) => { documents = next; }, saved: vi.fn() });
  return { workspace, read, save, chooseClose, documents: () => documents };
}

function edit(f: ReturnType<typeof fixture>, id: string, value: string) {
  const doc = f.workspace.get(id)!;
  f.workspace.update(id, doc.state!.update({ changes: { from: 0, to: doc.state!.doc.length, insert: value } }).state);
}

describe("file workspace", () => {
  it("coalesces changes during reads and performs one follow-up without dropping the last update", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    let resolve!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    f.read.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const reading = f.workspace.refresh(id);
    for (let count = 0; count < 20; count++) await f.workspace.refresh(id);
    f.read.mockResolvedValueOnce({ path: "a.txt", content: "newest", size: 6, version: "v3" });
    resolve({ path: "a.txt", content: "older", size: 5, version: "v2" });
    await reading;
    await vi.waitFor(() => expect(f.workspace.get(id)?.version).toBe("v3"));
    expect(f.read).toHaveBeenCalledTimes(3);
  });

  it("checks disk after a save finishes and preserves a draft typed during the save", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "saving");
    let resolve!: (value: Awaited<ReturnType<typeof f.save>>) => void;
    f.save.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const saving = f.workspace.save(id);
    edit(f, id, "new draft");
    await f.workspace.refresh(id);
    f.read.mockResolvedValueOnce({ path: "a.txt", content: "external", size: 8, version: "v3" });
    resolve({ outcome: "saved", version: "v2", recoveryPath: null, pendingPath: null, detail: "saved" });
    await saving;
    await vi.waitFor(() => expect(f.workspace.get(id)?.disk?.version).toBe("v3"));
    expect(f.workspace.get(id)?.state?.sliceDoc()).toBe("new draft");
    expect(f.workspace.get(id)?.issue?.outcome).toBe("conflict");
  });

  it("moves a queued refresh to the new document after save as", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "saved");
    let resolve!: (value: Awaited<ReturnType<typeof f.save>>) => void;
    f.save.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const saving = f.workspace.save(id, "b.txt");
    edit(f, id, "new typing");
    await f.workspace.refresh(id);
    f.read.mockResolvedValueOnce({ path: "b.txt", content: "external", size: 8, version: "v3" });
    resolve({ outcome: "saved", version: "v2", recoveryPath: null, pendingPath: null, detail: "saved" });
    const nextId = (await saving)!;
    await vi.waitFor(() => expect(f.workspace.get(nextId)?.disk?.version).toBe("v3"));
    expect(f.workspace.get(nextId)?.state?.sliceDoc()).toBe("new typing");
    expect(f.read).toHaveBeenLastCalledWith("p", "b.txt");
    expect(f.workspace.get(id)).toBeUndefined();
  });

  it("resumes a queued refresh after exit preparation is released", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    const release = await f.workspace.prepareExit();
    await f.workspace.refresh(id);
    expect(f.read).toHaveBeenCalledTimes(1);
    f.read.mockResolvedValueOnce({ path: "a.txt", content: "external", size: 8, version: "v3" });
    if (typeof release === "function") release();
    await vi.waitFor(() => expect(f.workspace.get(id)?.disk?.version).toBe("v3"));
    expect(f.workspace.get(id)?.state?.sliceDoc()).toBe("draft");
  });

  it("does not transfer a closed document's queued refresh to a new instance", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    let resolve!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    f.read.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const reading = f.workspace.refresh(id);
    await f.workspace.refresh(id);
    await f.workspace.close(id);
    await f.workspace.open("p", "a.txt");
    resolve({ path: "a.txt", content: "stale", size: 5, version: "old" });
    await reading;
    expect(f.read).toHaveBeenCalledTimes(3);
    expect(f.workspace.get(id)?.version).toBe("v1");
  });
  it("locks drafts until the exit lease releases and rejects new opens and writes", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "approved draft");
    const release = await f.workspace.prepareExit();
    expect(typeof release).toBe("function");
    expect(f.workspace.get(id)!.locked).toBe(true);
    expect(f.workspace.busy()).toBe(false);
    edit(f, id, "late typing");
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("approved draft");
    await expect(f.workspace.open("p", "b.txt")).rejects.toThrow();
    expect(await f.workspace.save(id)).toBeNull();
    expect(await f.workspace.close(id)).toBe(false);
    await expect(f.workspace.removeProject("p", async () => {})).rejects.toThrow();
    if (typeof release === "function") { release(); release(); }
    expect(f.workspace.get(id)!.locked).toBe(false);
    edit(f, id, "after failure");
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("after failure");
  });

  it("saves the approved draft under the exit lock and unlocks on cancellation or error", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    f.chooseClose.mockResolvedValueOnce("save");
    const release = await f.workspace.prepareExit();
    expect(typeof release).toBe("function");
    expect(f.save).toHaveBeenCalledWith("p", "a.txt", "draft", "v1");
    expect(documentDirty(f.workspace.get(id)!)).toBe(false);
    if (typeof release === "function") release();
    edit(f, id, "new draft");
    f.chooseClose.mockResolvedValueOnce(null);
    expect(await f.workspace.prepareExit()).toBe(false);
    expect(f.workspace.get(id)!.locked).toBe(false);
    f.chooseClose.mockRejectedValueOnce(new Error("dialog failed"));
    await expect(f.workspace.prepareExit()).rejects.toThrow("dialog failed");
    expect(f.workspace.get(id)!.locked).toBe(false);
  });

  it("does not let a repeated exit request release the first request's lock", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    const release = await f.workspace.prepareExit();
    expect(await f.workspace.prepareExit()).toBe(false);
    expect(f.workspace.get(id)!.locked).toBe(true);
    if (typeof release === "function") release();
    expect(f.workspace.get(id)!.locked).toBe(false);
  });

  it("resumes an interrupted initial read when an exit is cancelled", async () => {
    const f = fixture();
    let oldRead!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    f.read.mockImplementationOnce(() => new Promise((resolve) => { oldRead = resolve; }));
    const opening = f.workspace.open("p", "a.txt");
    const id = f.workspace.id("p", "a.txt");
    const release = await f.workspace.prepareExit();
    expect(f.workspace.get(id)!.locked).toBe(true);
    if (typeof release === "function") release();
    oldRead({ path: "a.txt", content: "stale", size: 5, version: "old" });
    await opening;
    await vi.waitFor(() => expect(f.workspace.get(id)!.state?.sliceDoc()).toBe("original\r\n"));
    expect(f.workspace.get(id)!.loading).toBe(false);
  });

  it("keeps drafts immutable through real close-handler shutdown and unlocks before retry", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "approved draft");
    const stopPi = vi.fn(async () => {
      expect(f.workspace.get(id)!.locked).toBe(true);
      edit(f, id, "late typing");
      expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("approved draft");
    });
    const destroy = vi.fn().mockRejectedValueOnce(new Error("destroy failed")).mockResolvedValueOnce(undefined);
    const error = vi.fn();
    const close = createWindowCloseHandler({
      behavior: () => "exit", blocked: f.workspace.busy, hasActiveTasks: () => false,
      choose: async () => null, confirm: async () => true, remember: () => {},
      minimize: async () => {}, prepareExit: f.workspace.prepareExit,
      stopPi, stopDsh: async () => {}, destroy, error,
    });
    await close({ preventDefault() {} });
    expect(error).toHaveBeenCalledOnce();
    expect(f.workspace.get(id)!.locked).toBe(false);
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("approved draft");
    await close({ preventDefault() {} });
    expect(stopPi).toHaveBeenCalledTimes(2);
    expect(destroy).toHaveBeenCalledTimes(2);
    expect(f.workspace.get(id)!.locked).toBe(false);
  });

  it("does not apply an old read to a closed and reopened path", async () => {
    const f = fixture();
    let first!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    let second!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    f.read.mockImplementationOnce(() => new Promise((resolve) => { first = resolve; }))
      .mockImplementationOnce(() => new Promise((resolve) => { second = resolve; }));
    const oldOpen = f.workspace.open("p", "a.txt");
    const id = f.workspace.id("p", "a.txt");
    await f.workspace.close(id);
    const newOpen = f.workspace.open("p", "a.txt");
    second({ path: "a.txt", content: "new instance", size: 12, version: "new" });
    await newOpen;
    first({ path: "a.txt", content: "stale instance", size: 14, version: "old" });
    await oldOpen;
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("new instance");
    expect(f.workspace.get(id)!.version).toBe("new");
  });

  it("retains BOM and every existing newline byte including mixed endings", () => {
    for (const source of ["", "\uFEFF中文\r\nnext\r\n", "a\nb\n", "a\rb\r", "a\r\nb\nc\rd"]) {
      const state = createDocumentState(source);
      expect(state.sliceDoc()).toBe(source);
      const edited = state.update({ changes: { from: 0, insert: "x" } }).state;
      expect(edited.sliceDoc()).toBe("x" + source);
    }
  });
  it("retains independent drafts and undo states when reopening files or projects", async () => {
    const f = fixture();
    const a = await f.workspace.open("p1", "a.txt");
    edit(f, a, "draft");
    const state = f.workspace.get(a)!.state;
    await f.workspace.open("p2", "a.txt");
    await f.workspace.open("p1", "b.txt");
    await f.workspace.open("p1", "a.txt");
    expect(f.read).toHaveBeenCalledTimes(3);
    expect(f.workspace.get(a)!.state).toBe(state);
    expect(documentDirty(f.workspace.get(a)!)).toBe(true);
  });
  it("only marks the submitted snapshot saved when more typing occurs during a save", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "first");
    let finish!: (value: Awaited<ReturnType<typeof f.save>>) => void;
    f.save.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const saving = f.workspace.save(id);
    edit(f, id, "second");
    finish({ outcome: "saved", version: "v2", recoveryPath: null, pendingPath: null, detail: "saved" });
    expect(await saving).toBe(id);
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("second");
    expect(documentDirty(f.workspace.get(id)!)).toBe(true);
    expect(f.workspace.get(id)!.version).toBe("v2");
  });
  it("never replaces dirty text during a disk refresh", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    f.read.mockResolvedValueOnce({ path: "a.txt", content: "external", size: 8, version: "v3" });
    await f.workspace.refresh(id);
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("draft");
    expect(f.workspace.get(id)!.disk?.content).toBe("external");
    expect(f.workspace.get(id)!.version).toBe("v1");
    expect(f.workspace.get(id)!.issue?.outcome).toBe("conflict");
  });
  it("retains a draft on IPC failure and never retries an unknown write automatically", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    f.save.mockRejectedValueOnce(new Error("transport lost"));
    expect(await f.workspace.save(id)).toBeNull();
    expect(f.workspace.get(id)!.issue?.outcome).toBe("unknown");
    expect(await f.workspace.save(id)).toBeNull();
    expect(f.save).toHaveBeenCalledTimes(1);
    expect(documentDirty(f.workspace.get(id)!)).toBe(true);
  });
  it("retargets save-as only after confirmed success and keeps subsequent edits dirty", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    const next = await f.workspace.save(id, "b.txt");
    expect(next).not.toBe(id);
    expect(f.workspace.get(id)).toBeUndefined();
    expect(f.workspace.get(next!)!.path).toBe("b.txt");
    expect(documentDirty(f.workspace.get(next!)!)).toBe(false);
    expect(f.save).toHaveBeenCalledWith("p", "b.txt", "draft", null);
  });
  it("cancelled close keeps the document and discard on exit does not destroy live drafts", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    f.chooseClose.mockResolvedValueOnce(null);
    expect(await f.workspace.close(id)).toBe(false);
    expect(f.workspace.get(id)).toBeDefined();
    expect(await f.workspace.prepareClose()).toBe(true);
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("draft");
  });
  it("refuses to discard text that changed while the close dialog was pending", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "first");
    let answer!: (value: "discard") => void;
    f.chooseClose.mockImplementationOnce(() => new Promise((resolve) => { answer = resolve; }));
    const closing = f.workspace.close(id);
    edit(f, id, "second");
    answer("discard");
    expect(await closing).toBe(false);
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("second");
  });
  it("does not overwrite edits made while an automatic refresh is in flight", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    let finish!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    f.read.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const refreshing = f.workspace.refresh(id);
    edit(f, id, "late draft");
    finish({ path: "a.txt", content: "disk changed", size: 12, version: "v3" });
    await refreshing;
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("late draft");
    expect(documentDirty(f.workspace.get(id)!)).toBe(true);
  });
  it("keeps unknown results visible even if undo returns the draft to its old baseline", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    f.save.mockRejectedValueOnce(new Error("response lost"));
    await f.workspace.save(id);
    edit(f, id, "original\r\n");
    f.read.mockResolvedValueOnce({ path: "a.txt", content: "draft", size: 5, version: "v3" });
    await f.workspace.refresh(id);
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("original\r\n");
    expect(f.workspace.get(id)!.version).toBe("v1");
    expect(f.workspace.get(id)!.issue).not.toBeNull();
  });
  it("ignores a late read response after its tab closes", async () => {
    const f = fixture();
    let finish!: (value: Awaited<ReturnType<typeof f.read>>) => void;
    f.read.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const opened = f.workspace.open("p", "a.txt");
    const id = f.workspace.id("p", "a.txt");
    expect(await f.workspace.close(id)).toBe(true);
    finish({ path: "a.txt", content: "late", size: 4, version: "v1" });
    await opened;
    expect(f.workspace.get(id)).toBeUndefined();
  });
  it("reserves save-as targets against concurrent opens and duplicate writes", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    let finish!: (value: Awaited<ReturnType<typeof f.save>>) => void;
    f.save.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const saving = f.workspace.save(id, "new.txt");
    expect(await f.workspace.save(id, "new.txt")).toBeNull();
    await expect(f.workspace.open("p", "new.txt")).rejects.toThrow();
    finish({ outcome: "saved", version: "v2", recoveryPath: null, pendingPath: null, detail: "saved" });
    await saving;
    expect(f.save).toHaveBeenCalledTimes(1);
  });
  it("locks a project during removal and restores its drafts when the backend fails", async () => {
    const f = fixture();
    const id = await f.workspace.open("p", "a.txt");
    edit(f, id, "draft");
    let fail!: (error: Error) => void;
    const removing = f.workspace.removeProject("p", () => new Promise<void>((_resolve, reject) => { fail = reject; }));
    expect(f.workspace.get(id)!.locked).toBe(true);
    edit(f, id, "late typing");
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("draft");
    await expect(f.workspace.open("p", "other.txt")).rejects.toThrow();
    expect(await f.workspace.save(id)).toBeNull();
    fail(new Error("remove failed"));
    await expect(removing).rejects.toThrow("remove failed");
    expect(f.workspace.get(id)!.locked).toBe(false);
    expect(f.workspace.get(id)!.state!.sliceDoc()).toBe("draft");
  });
});
