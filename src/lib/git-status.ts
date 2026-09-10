export interface GitEntry {
  path: string;
  originalPath: string | null;
  indexStatus: string;
  worktreeStatus: string;
  kind: "tracked" | "untracked" | "conflict";
  sourceOutsideProject?: boolean;
}

export type GitDiffArea = "staged" | "unstaged" | "untracked" | "conflict";

export interface GitStatus {
  branch: string | null;
  upstream: string | null;
  oid: string | null;
  unborn: boolean;
  detached: boolean;
  ahead: number;
  behind: number;
  entries: GitEntry[];
  repositoryRoot: string;
  projectPrefix: string;
}

export function groupGitEntries(entries: GitEntry[]): Array<{id: GitDiffArea; label: string; entries: GitEntry[]}> {
  return [
    { id: "conflict", label: "冲突", entries: entries.filter((entry) => entry.kind === "conflict") },
    { id: "staged", label: "已暂存", entries: entries.filter((entry) => entry.kind === "tracked" && entry.indexStatus !== ".") },
    { id: "unstaged", label: "未暂存", entries: entries.filter((entry) => entry.kind === "tracked" && entry.worktreeStatus !== ".") },
    { id: "untracked", label: "未跟踪", entries: entries.filter((entry) => entry.kind === "untracked") },
  ];
}

export interface GitLoadState<T> { loading: boolean; value: T | null; error: string }

export function createGitStatusLoader<T, Request = string>(
  read: (request: Request, operationId: string) => Promise<T>,
  changed: (state: GitLoadState<T>) => void,
  cancelRead?: (operationId: string) => Promise<void>,
) {
  let generation = 0;
  let running = false;
  let disposed = false;
  let pending: { input: Request; generation: number } | null = null;
  let active: { id: string; cancelling: boolean } | null = null;
  function cancelActive() {
    if (!active || active.cancelling || !cancelRead) return;
    active.cancelling = true;
    const expected = generation;
    void cancelRead(active.id).catch((error) => {
      if (!disposed && expected === generation) changed({
        loading: false, value: null,
        error: `取消请求发送失败，旧结果不会显示；后台将按预算停止：${String(error)}`,
      });
    });
  }
  async function drain() {
    if (running) return;
    running = true;
    try {
      while (pending && !disposed) {
        const request = pending;
        pending = null;
        active = { id: crypto.randomUUID(), cancelling: false };
        try {
          const value = await read(request.input, active.id);
          if (!disposed && request.generation === generation) changed({ loading: false, value, error: "" });
        } catch (error) {
          if (!disposed && request.generation === generation) changed({ loading: false, value: null, error: String(error) });
        } finally { active = null; }
      }
    } finally { running = false; }
  }
  return {
    load(input: Request) {
      if (disposed) return;
      pending = { input, generation: ++generation };
      cancelActive();
      changed({ loading: true, value: null, error: "" });
      void drain();
    },
    cancel() {
      generation++; pending = null; cancelActive();
      if (!disposed) changed({ loading: false, value: null, error: "已取消读取" });
    },
    invalidate() { generation++; pending = null; cancelActive(); },
    dispose() { disposed = true; generation++; pending = null; cancelActive(); },
  };
}
