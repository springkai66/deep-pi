export interface CommitPreview {
  reference: string;
  head: string | null;
  tree: string;
  paths: string[];
  author: string;
  committer: string;
  sign: boolean;
}

export interface CommitResult { outcome: "committed" | "notCommitted" | "unknown"; oid: string; detail: string }
export interface CommitPorts {
  prepare: (projectId: string, operationId: string) => Promise<CommitPreview>;
  commit: (projectId: string, expected: CommitPreview, message: string, operationId: string) => Promise<CommitResult | null>;
}
export interface CommitState {
  projectId: string | null;
  message: string;
  preview: CommitPreview | null;
  busy: boolean;
  phase: "idle" | "preparing" | "committing";
  error: string;
  result: CommitResult | null;
}

export function validCommitMessage(message: string): boolean {
  return !!message.trim() && !message.includes("\0") && new TextEncoder().encode(message).length <= 64 * 1024;
}

export function createCommitController(
  ports: CommitPorts, changed: (state: CommitState) => void, applied: (projectId: string) => void,
) {
  const drafts = new Map<string, string>();
  let state: CommitState = { projectId: null, message: "", preview: null, busy: false, phase: "idle", error: "", result: null };
  let generation = 0;
  let active = false;
  let disposed = false;
  const emit = (patch: Partial<CommitState>) => {
    state = { ...state, ...patch };
    if (!disposed) changed(state);
  };
  return {
    setProject(projectId: string | null) {
      if (disposed || state.projectId === projectId) return;
      generation++;
      emit({ projectId, message: projectId ? drafts.get(projectId) ?? "" : "", preview: null, error: "", result: null });
    },
    setMessage(message: string) {
      if (disposed || !state.projectId) return;
      drafts.set(state.projectId, message);
      emit({ message });
    },
    invalidate() { generation++; emit({ preview: null }); },
    async prepare() {
      if (disposed || active || !state.projectId) return;
      const projectId = state.projectId;
      active = true;
      emit({ busy: true, phase: "preparing", preview: null, error: "", result: null });
      try {
        // 准备提交会现场写入 Git 对象，文件监听因此可能在准备期间上报一次变更，
        // 使在途结果失效（第二次写出的对象已存在，不会再触发）。后端已校验 HEAD、
        // 索引与引用，提交时还有一次期望值比对，所以这里重试一次对齐最新状态。
        for (let attempt = 0; attempt < 2; attempt++) {
          const expectedGeneration = ++generation;
          const preview = await ports.prepare(projectId, crypto.randomUUID());
          if (disposed || state.projectId !== projectId) return;
          if (generation === expectedGeneration) {
            emit({ preview });
            return;
          }
        }
        emit({ error: "暂存内容在审阅期间发生变化，请重新审阅暂存" });
      } catch (error) {
        if (!disposed && state.projectId === projectId) emit({ error: String(error) });
      } finally {
        active = false;
        emit({ busy: false, phase: "idle" });
      }
    },
    async submit() {
      if (disposed || active || !state.projectId || !state.preview || !validCommitMessage(state.message)) return;
      const { projectId, preview, message } = state;
      active = true;
      let committed = false;
      emit({ busy: true, phase: "committing", error: "", result: null });
      try {
        const result = await ports.commit(projectId, preview, message, crypto.randomUUID());
        if (result?.outcome === "committed") {
          committed = true;
          if (drafts.get(projectId) === message) drafts.delete(projectId);
        }
        if (!disposed && state.projectId === projectId && result) {
          emit({ result, preview: null, message: committed ? drafts.get(projectId) ?? "" : state.message });
        }
      } catch (error) {
        if (!disposed && state.projectId === projectId) emit({
          preview: null, error: `提交结果未确认，请刷新后核对；不会自动重试：${String(error)}`,
        });
      } finally {
        active = false;
        emit({ busy: false, phase: "idle" });
      }
      if (committed && !disposed) applied(projectId);
    },
    dispose() { disposed = true; generation++; },
  };
}
