import { resolveAppMessage } from "./app-messages";
import { getLocale, t } from "./i18n.svelte";
import type { GitDiffArea, GitEntry } from "./git-status";

export type IndexAction = "stage" | "unstage" | "resolve";
export interface IndexRequest { entry: GitEntry; action: IndexAction }
export interface IndexWriteState { busy: boolean; projectId: string | null; error: string }
export type WriteIndex = (projectId: string, request: IndexRequest, operationId: string) => Promise<boolean>;

export function indexActionForArea(area: GitDiffArea): IndexAction {
  return area === "staged" ? "unstage" : area === "conflict" ? "resolve" : "stage";
}

export function createGitIndexWriter(
  write: WriteIndex,
  changed: (state: IndexWriteState) => void,
  applied: (projectId: string, path: string) => void,
) {
  let busy = false;
  let disposed = false;
  return {
    async run(projectId: string, entry: GitEntry, area: GitDiffArea) {
      if (busy || disposed) return;
      busy = true;
      changed({ busy: true, projectId, error: "" });
      let error = "";
      try {
        const result = await write(projectId, { entry, action: indexActionForArea(area) }, crypto.randomUUID());
        if (result && !disposed) applied(projectId, entry.path);
      } catch (failure) {
        error = t("Git 操作未确认完成，请刷新状态后再操作：{error}", { error: resolveAppMessage(String(failure), getLocale()) });
      } finally {
        busy = false;
        if (!disposed) changed({ busy: false, projectId, error });
      }
    },
    dispose() { disposed = true; },
  };
}
