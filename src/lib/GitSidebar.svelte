<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { Check, GitBranch, Minus, Plus, RefreshCw, Square, X } from "@lucide/svelte";
  import { onDestroy, onMount, untrack } from "svelte";
  import { createGitStatusLoader, groupGitEntries, type GitDiffArea, type GitEntry, type GitLoadState, type GitStatus } from "./git-status";
  import { createGitIndexWriter, type IndexWriteState, type WriteIndex } from "./git-index";
  import GitCommitPanel from "./GitCommitPanel.svelte";
  import type { CommitPorts } from "./git-commit";
  import GitPushPanel from "./GitPushPanel.svelte";
  import type { PushPorts } from "./git-push";
  import { t, tm, getLocale } from "$lib/i18n.svelte";
  import { appConfirmDialog, type AppConfirmDialogPayload } from "./app-messages";

  let { projectId, visible, refreshToken, selected, onOpen, onClose, onChanged, onBusyChange, commitPorts, pushPorts,
    onResizeStart = null,
    available = isTauri(),
    readStatus = (id: string, operationId: string, trustDialog: AppConfirmDialogPayload | null) => invoke<GitStatus>("project_git_status", { projectId: id, operationId, trustDialog }),
    cancelRead = (operationId: string) => invoke<void>("cancel_git_read", { operationId }),
    writeIndex = (id, request, operationId) => invoke<boolean>("project_git_change_index", { projectId: id, request, operationId,
      dialogs: {
        stage: appConfirmDialog("git.index.dialog.stage", getLocale()),
        unstage: appConfirmDialog("git.index.dialog.unstage", getLocale()),
        resolve: appConfirmDialog("git.index.dialog.resolve", getLocale()),
      },
    }),
  }: {
    projectId: string | null; visible: boolean; refreshToken: number;
    selected: { path: string; area: GitDiffArea } | null;
    onOpen: (path: string, area: GitDiffArea) => void;
    onClose: () => void;
    onChanged: (projectId: string, path: string) => void;
    onBusyChange: (busy: boolean) => void;
    onResizeStart?: ((event: PointerEvent) => void) | null;
    available?: boolean;
    readStatus?: (projectId: string, operationId: string, trustDialog: AppConfirmDialogPayload | null) => Promise<GitStatus>;
    cancelRead?: (operationId: string) => Promise<void>;
    writeIndex?: WriteIndex;
    commitPorts?: CommitPorts;
    pushPorts?: PushPorts;
  } = $props();
  let requestedProjects = $state(new Set<string>());
  let loadState = $state<GitLoadState<GitStatus>>({ loading: false, value: null, error: "" });
  let localRefresh = $state(0);
  let refreshPending = $state(false);
  let writeState = $state<IndexWriteState>({ busy: false, projectId: null, error: "" });
  let commitBusy = $state(false);
  let pushBusy = $state(false);
  const busy = $derived(writeState.busy || commitBusy || pushBusy);
  const writer = createGitIndexWriter(
    (id, request, operationId) => writeIndex(id, request, operationId),
    (next) => { writeState = next; onBusyChange(next.busy || commitBusy || pushBusy); },
    (id, path) => onChanged(id, path),
  );
  const trustRequests = new Set<string>();
  let loadedProject: string | null = null;
  const requested = $derived(!!projectId && requestedProjects.has(projectId));
  const groups = $derived(groupGitEntries(loadState.value?.entries ?? []));
  const loader = createGitStatusLoader(
    // 信任确认文案由前端按当前语言传入；未请求信任时传 null，后端直接按未信任处理。
    (id, operationId) =>
      readStatus(id, operationId, trustRequests.delete(id) ? appConfirmDialog("git.trust.dialog", getLocale()) : null),
    (next) => {
      const previous = untrack(() => loadState);
      loadState = next.loading ? { ...previous, loading: true, error: "" } : next;
    },
    (operationId) => cancelRead(operationId),
  );
  $effect(() => {
    void refreshToken;
    void localRefresh;
    untrack(() => { refreshPending = true; });
  });
  $effect(() => {
    const id = projectId;
    if (loadedProject !== id) {
      loadedProject = id;
      loadState = { loading: false, value: null, error: "" };
    }
    if (visible && id && requested && available) refreshPending = true;
    else {
      refreshPending = false;
      loader.invalidate();
      loadState = { loading: false, value: null, error: "" };
    }
  });
  $effect(() => {
    if (refreshPending && visible && projectId && requested && available && !loadState.loading && !busy) {
      const id = projectId;
      untrack(() => { refreshPending = false; loader.load(id); });
    }
  });
  onDestroy(() => { loader.dispose(); writer.dispose(); });
  onMount(() => {
    const refresh = () => { if (visible && requested && !loadState.error) localRefresh++; };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });
  function requestStatus() {
    if (projectId) {
      trustRequests.add(projectId);
      requestedProjects = new Set([...requestedProjects, projectId]);
    }
  }
  function refreshExplicitly() {
    if (projectId) trustRequests.add(projectId);
    localRefresh++;
  }
  async function changeIndex(entry: GitEntry, area: GitDiffArea) {
    if (!projectId || busy || loadState.loading) return;
    const id = projectId;
    loader.invalidate();
    await writer.run(id, entry, area);
    if (projectId === id) localRefresh++;
  }
</script>

<aside class="git-sidebar" class:panel-hidden={!visible} aria-label={t("Git 变更")} aria-busy={loadState.loading || busy}>
  {#if onResizeStart}<div class="panel-resize-handle resize-left" role="separator" aria-orientation="vertical" aria-label={t("拖拽调整 Git 栏宽度")} title={t("拖拽调整宽度")} onpointerdown={onResizeStart}></div>{/if}
  <header>
    <GitBranch size={16} /><strong>{t("变更")}</strong>
    {#if loadState.loading}
      <button type="button" aria-label={t("取消 Git 状态读取")} title={t("取消 Git 状态读取")} onclick={() => { refreshPending = false; loader.cancel(); }}><Square size={14} /></button>
    {/if}
    <button type="button" aria-label={t("刷新 Git 状态")} title={t("刷新 Git 状态")} disabled={!requested || loadState.loading || busy || !available}
      onclick={refreshExplicitly}><RefreshCw size={15} /></button>
    <button type="button" aria-label={t("收起变更栏")} title={t("收起变更栏")} onclick={onClose}><X size={16} /></button>
  </header>
  <div class="git-actions">
  <GitCommitPanel {projectId} visible={visible && requested && available}
    blocked={writeState.busy || pushBusy || loadState.loading} stagedCount={groups.find((group) => group.id === "staged")?.entries.length ?? 0}
    refreshToken={refreshToken + localRefresh} ports={commitPorts}
    onBusyChange={(next) => { commitBusy = next; onBusyChange(writeState.busy || next || pushBusy); }}
    onCommitted={(id) => { onChanged(id, ""); if (id === projectId) localRefresh++; }} />
  <GitPushPanel {projectId} visible={visible && requested && available}
    blocked={writeState.busy || commitBusy || loadState.loading} refreshToken={refreshToken + localRefresh} ports={pushPorts}
    onBusyChange={(next) => { pushBusy = next; onBusyChange(writeState.busy || commitBusy || next); }}
    onPushed={(id) => { if (id === projectId) localRefresh++; }} />
  </div>
  {#if !projectId}
    <p role="status">{t("未选择项目")}</p>
  {:else if !available}
    <p role="status">{t("Git 状态仅在桌面应用中可用")}</p>
  {:else if !requested}
    <div class="trust-action"><button type="button" class="read-status" onclick={requestStatus}>{t("读取 Git 状态")}</button></div>
  {:else if loadState.loading && !loadState.value}
    <p role="status">{t("正在读取 Git 状态…")}</p>
  {:else if loadState.error}
    <p role="alert">{tm(loadState.error)}</p>
  {:else if loadState.value}
    <div class="branch-summary">
      <strong title={loadState.value.branch ?? loadState.value.oid ?? ""}>{loadState.value.detached ? t("分离 HEAD") : loadState.value.branch}</strong>
      {#if loadState.value.unborn}<span>{t("尚无提交")}</span>{/if}
      {#if loadState.value.projectPrefix}<span title={loadState.value.repositoryRoot}>{t("范围：{prefix}", { prefix: loadState.value.projectPrefix })}</span>{/if}
      {#if loadState.value.upstream}
        <span title={loadState.value.upstream}>{loadState.value.upstream}</span>
        <span>{t("领先 {ahead} · 落后 {behind}", { ahead: loadState.value.ahead, behind: loadState.value.behind })}</span>
      {/if}
    </div>
    {#if writeState.projectId === projectId && writeState.busy}<p role="status">{t("正在处理 Git 索引…")}</p>{/if}
    {#if writeState.projectId === projectId && writeState.error}<p role="alert">{tm(writeState.error)}</p>{/if}
    <div class="change-list">
      {#each groups as group (group.id)}
        {#if group.entries.length}
          <section aria-label={t(group.label)}>
            <h2>{t(group.label)}<span>{group.entries.length}</span></h2>
            <ul>
              {#each group.entries as entry (entry.path)}
                {@const actionLabel = group.id === "staged" ? t("取消暂存") : group.id === "conflict" ? t("标记已解决") : t("暂存")}
                <li>
                  <button type="button" class="diff-entry"
                    class:selected={selected?.path === entry.path && selected.area === group.id}
                    aria-label={`${t(group.label)} ${entry.path}`} onclick={() => onOpen(entry.path, group.id)}>
                    <span class="path" title={entry.path}>{entry.path}</span>
                    <code aria-label={t("索引 {index} 工作区 {worktree}", { index: entry.indexStatus, worktree: entry.worktreeStatus })}>{entry.indexStatus}{entry.worktreeStatus}</code>
                    {#if entry.originalPath}<small title={entry.originalPath}>{t("原路径：{path}", { path: entry.originalPath })}</small>{/if}
                    {#if entry.sourceOutsideProject}<small>{t("重命名源在项目外")}</small>{/if}
                  </button>
                  <button type="button" class="index-action"
                    aria-label={`${actionLabel} ${entry.path}`}
                    title={entry.sourceOutsideProject ? t("跨项目重命名，请从完整仓库操作") : `${actionLabel} ${entry.path}`}
                    disabled={busy || loadState.loading || entry.sourceOutsideProject}
                    onclick={() => changeIndex(entry, group.id)}>
                    {#if group.id === "staged"}<Minus size={14} />
                    {:else if group.id === "conflict"}<Check size={14} />
                    {:else}<Plus size={14} />{/if}
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      {/each}
      {#if !loadState.value.entries.length}<p role="status">{t("未发现项目范围内的文件变更")}</p>{/if}
    </div>
    <p>{t("子模块内部未检查")}</p>
  {/if}
</aside>

<style>
  .git-sidebar { min-width: 0; min-height: 0; overflow: hidden; display: flex; flex-direction: column; background: var(--surface); border-left: 1px solid var(--border); }
  .git-actions { flex-shrink: 0; max-height: 55%; overflow: auto; }
  .panel-hidden { display: none; }
  header { display: flex; align-items: center; min-height: 40px; padding: 4px 8px; gap: 8px; border-bottom: 1px solid var(--border); }
  header strong { flex: 1; font-size: 12px; }
  header button { display: grid; place-items: center; width: 28px; height: 28px; padding: 0; border: 0; border-radius: 4px; color: var(--text); background: transparent; cursor: pointer; }
  button:disabled { opacity: .45; cursor: default; }
  header button:hover:not(:disabled) { background: var(--surface-hover); }
  p { padding: 12px; margin: 0; color: var(--text-muted); font-size: 12px; overflow-wrap: anywhere; }
  .branch-summary { display: flex; flex-direction: column; gap: 4px; padding: 12px; font-size: 12px; border-bottom: 1px solid var(--border); }
  .branch-summary span { color: var(--text-muted); }
  .branch-summary strong, .branch-summary span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .change-list { flex: 1; min-height: 0; overflow: auto; }
  h2 { display: flex; justify-content: space-between; margin: 0; padding: 10px 12px 6px; font-size: 12px; }
  h2 span { color: var(--text-muted); font-weight: 400; }
  ul { list-style: none; padding: 0; margin: 0; }
  li { margin: 0; display: flex; align-items: center; padding-right: 6px; }
  .index-action { display: grid; place-items: center; flex-shrink: 0; width: 28px; height: 28px; border: 0; border-radius: 4px; background: transparent; color: var(--text); cursor: pointer; }
  .index-action:hover:not(:disabled) { background: var(--surface-hover); }
  .diff-entry { display: grid; flex: 1; min-width: 0; grid-template-columns: minmax(0, 1fr) 24px; gap: 4px 8px; padding: 6px 12px; font: inherit; font-size: 12px; text-align: left; color: var(--text); border: 0; background: transparent; cursor: pointer; }
  .diff-entry:hover, .diff-entry.selected { background: var(--surface-hover); }
  .path, small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { grid-column: 1 / -1; color: var(--text-muted); }
  code { font: 11px var(--code-font); color: var(--text-muted); }
  .trust-action { padding: 12px; }
  .read-status { max-width: 100%; padding: 6px 12px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text); cursor: pointer; }
  @media (max-width: 1100px) { .git-sidebar { position: fixed; top: var(--topbar-h, 32px); bottom: 0; right: 0; width: min(300px, calc(100vw - 44px)); z-index: 5; box-shadow: -4px 0 16px #0002; } }
</style>
