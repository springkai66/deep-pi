import type { FilePreview } from "./files";
import type { FileDocument } from "./file-workspace";
import type { Text } from "@codemirror/state";

export type ComparisonSourceKind = "disk" | "target" | "recovery" | "pending";

export interface ComparisonSource {
  kind: ComparisonSourceKind;
  label: string;
  path: string;
}

export interface ComparisonSnapshot {
  sequence: number;
  documentId: string;
  projectId: string;
  path: string;
  draft: string;
  draftDocument: Text;
  baselineVersion: string | null;
  observedDiskVersion: string | null;
  source: ComparisonSource & { content: string; version: string };
}

export interface ComparisonState {
  status: "idle" | "loading" | "ready" | "error";
  snapshot: ComparisonSnapshot | null;
  error: string;
}

export function comparisonSources(doc: FileDocument): ComparisonSource[] {
  const sources: ComparisonSource[] = [{ kind: "disk", label: "当前文件（磁盘）", path: doc.path }];
  if (doc.issue?.target && doc.issue.target !== doc.path) {
    sources.push({ kind: "target", label: "上次保存目标", path: doc.issue.target });
  }
  if (doc.issue?.recoveryPath) sources.push({ kind: "recovery", label: "旧版本恢复副本", path: doc.issue.recoveryPath });
  if (doc.issue?.pendingPath) sources.push({ kind: "pending", label: "待写内容副本", path: doc.issue.pendingPath });
  return sources;
}

export function comparisonStale(snapshot: ComparisonSnapshot, doc: FileDocument | undefined): boolean {
  return !doc || doc.id !== snapshot.documentId || doc.version !== snapshot.baselineVersion
    || !doc.state?.doc.eq(snapshot.draftDocument)
    || snapshot.source.kind === "disk" && !!doc.disk
      && doc.disk.version !== snapshot.observedDiskVersion && doc.disk.version !== snapshot.source.version;
}

function validateText(content: string) {
  if (content.includes("\0") || new TextEncoder().encode(content).length > 2 * 1024 * 1024) {
    throw new Error("只能比较不含 NUL、最大 2 MiB 的 UTF-8 文本。");
  }
}

export function createFileComparison(
  read: (projectId: string, path: string) => Promise<FilePreview>,
  publish: (state: ComparisonState) => void,
) {
  let sequence = 0;
  return {
    reset() {
      sequence++;
      publish({ status: "idle", snapshot: null, error: "" });
    },
    async load(doc: FileDocument, kind: ComparisonSourceKind) {
      const current = ++sequence;
      publish({ status: "loading", snapshot: null, error: "" });
      try {
        const source = comparisonSources(doc).find((entry) => entry.kind === kind);
        if (!source || !doc.state) throw new Error("比较来源不可用，请重新选择。");
        const draft = doc.state.sliceDoc();
        validateText(draft);
        const baselineVersion = doc.version;
        const observedDiskVersion = doc.disk?.version ?? null;
        const loaded = await read(doc.projectId, source.path);
        if (current !== sequence) return;
        if (loaded.path !== source.path) throw new Error("读取结果与比较路径不一致。");
        validateText(loaded.content);
        publish({ status: "ready", error: "", snapshot: {
          sequence: current, documentId: doc.id, projectId: doc.projectId, path: doc.path,
          draft, draftDocument: doc.state.doc, baselineVersion, observedDiskVersion,
          source: { ...source, content: loaded.content, version: loaded.version },
        } });
      } catch (error) {
        if (current === sequence) publish({ status: "error", snapshot: null, error: String(error) });
      }
    },
  };
}
