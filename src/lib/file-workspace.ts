import type { EditorState, Text } from "@codemirror/state";
import type { FilePreview } from "./files";

export interface FileSaveResult {
  outcome: "saved" | "conflict" | "unknown";
  version: string | null;
  recoveryPath: string | null;
  pendingPath: string | null;
  detail: string;
}

export interface FileDocument {
  id: string;
  projectId: string;
  path: string;
  state: EditorState | null;
  baseline: Text | null;
  version: string | null;
  loading: boolean;
  saving: boolean;
  locked: boolean;
  error: string;
  issue: (FileSaveResult & { target: string }) | null;
  disk: FilePreview | null;
  generation: number;
  revision: number;
  scroll: { top: number; left: number };
}

export function documentDirty(doc: FileDocument): boolean {
  return !!doc.state && (!doc.baseline || !doc.state.doc.eq(doc.baseline));
}

function needsAttention(doc: FileDocument): boolean {
  return documentDirty(doc) || !!doc.issue && doc.issue.outcome !== "saved";
}

interface WorkspacePorts {
  read(project: string, path: string): Promise<FilePreview>;
  save(project: string, path: string, content: string, version: string | null): Promise<FileSaveResult>;
  chooseClose(documents: FileDocument[]): Promise<"save" | "discard" | null>;
  changed(documents: FileDocument[]): void;
  saved(project: string, path: string): void;
}

export function createFileWorkspace(ports: WorkspacePorts) {
  const documents = new Map<string, FileDocument>();
  const reserved = new Set<string>();
  const removing = new Set<string>();
  const pendingRefresh = new Set<string>();
  let exiting = false;
  let generationCounter = 0;
  const key = (project: string, path: string) => JSON.stringify([project, path.toLowerCase()]);
  const publish = () => ports.changed([...documents.values()]);
  function patch(id: string, changes: Partial<FileDocument>) {
    const current = documents.get(id);
    if (current) { documents.set(id, { ...current, ...changes }); publish(); }
  }

  async function read(id: string, force = false) {
    const initial = documents.get(id);
    if (!initial) return;
    if (initial.saving || initial.loading || initial.locked) { pendingRefresh.add(id); return; }
    pendingRefresh.delete(id);
    const generation = ++generationCounter;
    patch(id, { loading: true, error: "", generation });
    try {
      const disk = await ports.read(initial.projectId, initial.path);
      const { createDocumentState } = await import("./editor-document");
      const current = documents.get(id);
      if (!current || current.generation !== generation) return;
      const changedWhileReading = !!initial.state && !!current.state && !current.state.doc.eq(initial.state.doc);
      if (changedWhileReading || needsAttention(current) && !force) {
        patch(id, { disk, loading: false, issue: disk.version !== current.version ? {
          outcome: "conflict", version: null, recoveryPath: current.issue?.recoveryPath ?? null,
          pendingPath: current.issue?.pendingPath ?? null, target: current.path,
          detail: "磁盘文件已变化，当前草稿已保留。请比较或重新载入。",
        } : current.issue });
        return;
      }
      if (!force && current.state && disk.version === current.version) {
        patch(id, { disk, loading: false });
        return;
      }
      const state = createDocumentState(disk.content);
      patch(id, { state, baseline: state.doc, version: disk.version, disk, issue: null, loading: false, revision: current.revision + 1 });
    } catch (error) {
      if (documents.get(id)?.generation === generation) patch(id, { loading: false, error: String(error) });
    } finally { drainRefresh(id); }
  }

  function drainRefresh(id: string) {
    const doc = documents.get(id);
    if (doc && !doc.saving && !doc.loading && !doc.locked && pendingRefresh.delete(id)) void read(id);
  }

  async function save(id: string, targetPath?: string, exitSave = false): Promise<string | null> {
    const initial = documents.get(id);
    if (!initial?.state || initial.saving || initial.locked && !exitSave) return null;
    const path = targetPath ?? initial.path;
    const target = key(initial.projectId, path);
    const saveAs = targetPath !== undefined;
    if (reserved.has(target) || saveAs && documents.has(target)) {
      patch(id, { error: "目标文件已打开或正在保存，请选择其他文件名。" });
      return null;
    }
    if (!saveAs && initial.issue?.target === path && initial.issue.outcome !== "saved") {
      patch(id, { error: "上次保存或磁盘状态尚未核对，请先比较、重载或另存。" });
      return null;
    }
    const content = initial.state.sliceDoc();
    if (new TextEncoder().encode(content).length > 2 * 1024 * 1024 || content.includes("\0")) {
      patch(id, { error: "仅支持不含 NUL、最大 2 MiB 的 UTF-8 文本。" });
      return null;
    }
    reserved.add(target);
    patch(id, { saving: true, loading: false, error: "", generation: ++generationCounter });
    try {
      const result = await ports.save(initial.projectId, path, content, saveAs ? null : initial.version);
      const current = documents.get(id);
      if (!current) return null;
      if (result.outcome !== "saved" || !result.version) {
        patch(id, { issue: { ...result, outcome: result.outcome === "saved" ? "unknown" : result.outcome, target: path } });
        return null;
      }
      const next = { ...current, id: target, path, version: result.version, baseline: initial.state.doc,
        issue: { ...result, target: path }, disk: null, saving: false };
      documents.delete(id);
      documents.set(target, next);
      if (pendingRefresh.delete(id)) pendingRefresh.add(target);
      publish();
      ports.saved(initial.projectId, path);
      return target;
    } catch (error) {
      patch(id, { issue: { outcome: "unknown", version: null, recoveryPath: null, pendingPath: null,
        target: path, detail: `保存结果未确认：${String(error)}。草稿已保留，请先核对磁盘。` } });
      return null;
    } finally {
      reserved.delete(target);
      patch(id, { saving: false });
      drainRefresh(id);
      if (target !== id) drainRefresh(target);
    }
  }

  async function prepare(ids: string[], exitSave = false): Promise<boolean> {
    const initial = ids.map((id) => documents.get(id)).filter((doc): doc is FileDocument => !!doc);
    if (initial.some((doc) => doc.saving || doc.locked && !exitSave)) return false;
    const dirty = initial.filter(needsAttention);
    if (!dirty.length) return true;
    const choice = await ports.chooseClose(dirty);
    if (!choice) return false;
    if (initial.some((doc) => {
      const current = documents.get(doc.id);
      return !current || current.saving || current.locked && !exitSave
        || current.generation !== doc.generation || current.version !== doc.version || current.issue !== doc.issue
        || !!current.state !== !!doc.state || !!current.state && !current.state.doc.eq(doc.state!.doc);
    })) return false;
    if (choice === "discard") return true;
    for (const doc of dirty) if (!await save(doc.id, undefined, exitSave)) return false;
    return initial.every((doc) => {
      const current = documents.get(doc.id);
      return !!current && !needsAttention(current) && !current.saving;
    });
  }

  return {
    get: (id: string) => documents.get(id),
    id: key,
    busy: () => reserved.size > 0 || removing.size > 0,
    async open(projectId: string, path: string) {
      if (exiting) throw new Error("窗口正在退出，请等待当前关闭操作完成。");
      if (removing.has(projectId)) throw new Error("项目正在移除，请稍后操作。");
      const id = key(projectId, path);
      if (reserved.has(id) && !documents.has(id)) throw new Error("目标文件正在保存，请稍后打开。");
      if (!documents.has(id)) {
        if (documents.size >= 32) throw new Error("最多同时打开 32 个文件，请先关闭部分标签。");
        documents.set(id, { id, projectId, path, state: null, baseline: null, version: null,
          loading: false, saving: false, locked: false, error: "", issue: null, disk: null, generation: ++generationCounter, revision: 0, scroll: { top: 0, left: 0 } });
        publish();
        await read(id);
      }
      return id;
    },
    update(id: string, state: EditorState) {
      const current = documents.get(id);
      if (current?.locked && current.state && !current.state.doc.eq(state.doc)) { publish(); return; }
      patch(id, { state });
    },
    scroll(id: string, scroll: FileDocument["scroll"]) {
      const doc = documents.get(id);
      if (doc) doc.scroll = scroll;
    },
    refresh: (id: string) => read(id),
    async reload(id: string) { if (await prepare([id])) await read(id, true); },
    save: (id: string, targetPath?: string) => save(id, targetPath),
    async close(id: string) {
      if (!await prepare([id])) return false;
      documents.delete(id);
      pendingRefresh.delete(id);
      publish();
      return true;
    },
    async prepareClose(projectId?: string) {
      const ids = [...documents.values()].filter((doc) => !projectId || doc.projectId === projectId).map((doc) => doc.id);
      if (!await prepare(ids)) return false;
      return [...documents.values()].filter((doc) => !projectId || doc.projectId === projectId).every((doc) => ids.includes(doc.id));
    },
    async prepareExit(): Promise<false | (() => void)> {
      if (exiting || reserved.size || removing.size) return false;
      exiting = true;
      let released = false;
      const release = () => {
        if (released) return;
        released = true;
        exiting = false;
        for (const [id, doc] of documents) documents.set(id, { ...doc, locked: false });
        publish();
        // A cancelled exit must resume documents whose initial reads were invalidated by the lock.
        for (const [id, doc] of documents) {
          if (!doc.state) void read(id); else drainRefresh(id);
        }
      };
      try {
        for (const [id, doc] of documents) {
          if (doc.loading) pendingRefresh.add(id);
          documents.set(id, { ...doc, locked: true, loading: false, generation: ++generationCounter });
        }
        publish();
        if (await prepare([...documents.keys()], true)) return release;
        release();
        return false;
      } catch (error) { release(); throw error; }
    },
    async removeProject(projectId: string, remove: () => Promise<unknown>) {
      if (exiting || removing.has(projectId) || [...documents.values()].some((doc) => doc.projectId === projectId && doc.saving)) {
        throw new Error("项目文件正在处理，请稍后移除。");
      }
      removing.add(projectId);
      for (const [id, doc] of documents) if (doc.projectId === projectId) {
        if (doc.loading) pendingRefresh.add(id);
        documents.set(id, { ...doc, locked: true, loading: false, generation: ++generationCounter });
      }
      publish();
      try {
        await remove();
        for (const [id, doc] of documents) if (doc.projectId === projectId) {
          documents.delete(id);
          pendingRefresh.delete(id);
        }
      } finally {
        removing.delete(projectId);
        for (const [id, doc] of documents) if (doc.projectId === projectId) documents.set(id, { ...doc, locked: false });
        publish();
        for (const [id, doc] of documents) if (doc.projectId === projectId) {
          if (!doc.state) void read(id); else drainRefresh(id);
        }
      }
    },
  };
}

export type FileWorkspace = ReturnType<typeof createFileWorkspace>;
