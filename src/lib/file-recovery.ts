import type { FileSaveResult } from "./file-workspace";

export interface RecoveryItem {
  id: string;
  target: string;
  path: string;
  kind: "pending" | "recovery";
  createdAt: number;
  status: "available" | "missing" | "projectChanged" | "unavailable";
  version: string | null;
  size: number | null;
  detail: string;
}
export interface RecoveryPage {
  items: RecoveryItem[];
  total: number;
  nextOffset: number | null;
}
export interface RecoveryState extends RecoveryPage {
  projectId: string;
  loading: boolean;
  writing: boolean;
  message: string;
  error: string;
}
export type RecoveryAction = "delete" | "forget" | "restore";

export function createRecoveryManager(
  invoke: <T>(command: string, args: Record<string, unknown>) => Promise<T>,
  publish: (state: RecoveryState) => void,
  busyChanged: (busy: boolean) => void,
  changed: (projectId: string) => void,
) {
  let scope = 0;
  let request = 0;
  let state: RecoveryState = {
    projectId: "", items: [], total: 0, nextOffset: null,
    loading: false, writing: false, message: "", error: "",
  };
  function update(patch: Partial<RecoveryState>) {
    state = { ...state, ...patch };
    publish(state);
  }
  function select(projectId: string) {
    scope++;
    request++;
    update({ projectId, items: [], total: 0, nextOffset: null, loading: false, message: "", error: "" });
  }
  async function load(offset = 0) {
    if (!state.projectId) return;
    const generation = scope;
    const sequence = ++request;
    const projectId = state.projectId;
    update({ loading: true, error: "" });
    try {
      const page = await invoke<RecoveryPage>("list_project_recoveries", { projectId, offset });
      if (generation !== scope || sequence !== request) return;
      const rows = offset ? [...state.items, ...page.items] : page.items;
      update({ ...page, items: [...new Map(rows.map((row) => [row.id, row])).values()] });
    } catch (cause) {
      if (generation === scope && sequence === request) update({ error: String(cause) });
    } finally {
      if (generation === scope && sequence === request) update({ loading: false });
    }
  }
  async function run(action: RecoveryAction, item: RecoveryItem, relativePath?: string) {
    if (state.writing || state.loading || !state.projectId) return;
    const current = state.items.find((row) => row.id === item.id);
    if (!current || current.version !== item.version || current.status !== item.status) return;
    if (action !== "forget" && (item.status !== "available" || !item.version)) return;
    if (action === "restore" && !relativePath?.trim()) return;
    const projectId = state.projectId;
    const generation = scope;
    update({ writing: true, error: "", message: "" });
    busyChanged(true);
    try {
      const result = action === "restore"
        ? await invoke<FileSaveResult | null>("restore_project_recovery", { projectId, request: {
          recordId: item.id, expectedVersion: item.version, relativePath,
        } })
        : await invoke<boolean>("delete_project_recovery", {
          projectId, recordId: item.id, expectedVersion: action === "forget" ? null : item.version,
        });
      if (!result) return;
      changed(projectId);
      if (generation !== scope) return;
      const message = typeof result === "boolean"
        ? action === "forget" ? "已移除记录，未删除副本文件。" : "已删除副本。"
        : `${result.outcome === "saved" ? "已另存恢复。" : "恢复结果需要核对。"} ${result.detail ?? ""}`;
      await load();
      if (generation === scope) update({ message });
    } catch (cause) {
      if (generation === scope) update({ error: `操作结果未确认，请刷新核对；不会自动重试。${String(cause)}` });
    } finally {
      update({ writing: false });
      busyChanged(false);
    }
  }
  return { select, load, run };
}
