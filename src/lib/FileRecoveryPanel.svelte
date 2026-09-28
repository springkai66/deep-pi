<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Eye, Columns2, Save, Trash2, ListX, RefreshCw, X } from "@lucide/svelte";
  import { Text } from "@codemirror/state";
  import { createRecoveryManager, type RecoveryItem, type RecoveryState, type RecoveryAction } from "./file-recovery";
  import type { FilePreview } from "./files";
  import type { ComparisonSnapshot } from "./file-comparison";
  import type { DialogRequest, DialogValue } from "./dialog";
  import { t, tm } from "$lib/i18n.svelte";

  let { projectId, projectName, onClose, onBusyChange, onChanged, canOperate, requestDialog }: {
    projectId: string; projectName: string; onClose: () => void;
    onBusyChange: (busy: boolean) => void; onChanged: (projectId: string) => void;
    canOperate: () => boolean; requestDialog: (request: Omit<DialogRequest, "id" | "resolve">) => Promise<DialogValue>;
  } = $props();
  let recoveryState = $state.raw<RecoveryState>({ projectId: "", items: [], total: 0, nextOffset: null,
    loading: false, writing: false, message: "", error: "" });
  const manager = createRecoveryManager(invoke, (next) => { recoveryState = next; }, () => {}, (id) => onChanged(id));
  let preparing = $state(false);
  let reading = $state(false);
  let error = $state("");
  let preview = $state.raw<FilePreview | null>(null);
  let comparison = $state.raw<ComparisonSnapshot | null>(null);
  let comparisonModule = $state.raw<Promise<typeof import("./FileComparison.svelte")> | null>(null);
  let selected = $state.raw<RecoveryItem | null>(null);
  let sequence = 0;
  const busy = $derived(preparing || recoveryState.writing);
  const labels = $derived({ available: t("可用"), missing: t("文件不存在"), projectChanged: t("目录已变化"), unavailable: t("无法读取") });
  onMount(() => {
    manager.select(projectId);
    void manager.load();
    return () => { sequence++; };
  });
  function clearPreview() {
    sequence++;
    reading = false;
    preview = null;
    comparison = null;
    selected = null;
  }
  async function refresh() {
    error = "";
    clearPreview();
    await manager.load();
  }
  async function inspect(item: RecoveryItem, compare: boolean) {
    const generation = ++sequence;
    reading = true;
    preview = null;
    comparison = null;
    selected = item;
    error = "";
    try {
      const source = await invoke<FilePreview>("read_project_recovery", {
        projectId, recordId: item.id, expectedVersion: item.version,
      });
      if (generation !== sequence) return;
      if (source.path !== item.path || source.version !== item.version) throw new Error(t("副本已变化，请刷新列表。"));
      if (!compare) { preview = source; return; }
      const disk = await invoke<FilePreview>("read_project_file", { projectId, relativePath: item.target });
      if (generation !== sequence) return;
      if (disk.path !== item.target) throw new Error(t("目标路径与请求不一致。"));
      comparisonModule ??= import("./FileComparison.svelte");
      comparison = {
        sequence: generation, documentId: item.id, projectId, path: item.target,
        draft: disk.content, draftDocument: Text.of(disk.content.split("\n")),
        baselineVersion: disk.version, observedDiskVersion: disk.version,
        source: { kind: item.kind, label: "恢复副本", path: source.path, content: source.content, version: source.version },
      };
    } catch (cause) {
      if (generation === sequence) error = tm(String(cause));
    } finally {
      if (generation === sequence) reading = false;
    }
  }
  async function operate(action: RecoveryAction, item: RecoveryItem) {
    if (busy || recoveryState.loading) return;
    if (!canOperate()) { error = t("项目文件或 Git 正在处理，请完成后重试。"); return; }
    error = "";
    preparing = true;
    onBusyChange(true);
    try {
      let target: string | undefined;
      if (action === "restore") {
        const result = await requestDialog({ kind: "input", title: t("恢复到新文件"),
          message: item.target, initialValue: `${item.target}.restored`, confirmLabel: t("继续"), maxLength: 4096 });
        if (typeof result !== "string" || !result.trim()) return;
        target = result.trim();
      }
      if (!canOperate()) { error = t("项目文件或 Git 正在处理，本次未执行。"); return; }
      clearPreview();
      await manager.run(action, item, target);
    } catch (cause) { error = tm(String(cause)); }
    finally { preparing = false; onBusyChange(false); }
  }
</script>

<section class="recovery-panel" aria-label={t("恢复副本")} aria-busy={busy}>
  <header>
    <h2>{t("恢复副本")} <span>{projectName}</span></h2>
    <button title={t("刷新副本列表")} aria-label={t("刷新副本列表")} disabled={busy || recoveryState.loading} onclick={refresh}><RefreshCw size={16} /></button>
    <button title={t("关闭恢复副本")} aria-label={t("关闭恢复副本")} disabled={busy} onclick={onClose}><X size={16} /></button>
  </header>
  {#if error || recoveryState.error}<p role="alert">{tm(error || recoveryState.error)}</p>{/if}
  {#if recoveryState.message}<p role="status">{tm(recoveryState.message)}</p>{/if}
  <div class="recovery-body" class:with-preview={preview || comparison}>
    <div class="records">
      <div class="count" role="status">{recoveryState.loading ? t("正在读取…") : t("{count} 条记录", { count: recoveryState.total })}</div>
      {#each recoveryState.items as item (item.id)}
        <article class:selected={selected?.id === item.id}>
          <strong>{item.target}</strong>
          <code>{item.path}</code>
          <small title={tm(item.detail)}>{item.kind === "recovery" ? t("旧版本") : t("待写内容")} · {labels[item.status]} · {new Date(item.createdAt).toLocaleString()}{item.size === null ? "" : ` · ${item.size} B`}</small>
          <div class="actions">
            <button title={t("预览副本")} aria-label={t("预览副本")} disabled={busy || item.status !== "available"} onclick={() => inspect(item, false)}><Eye size={16} /></button>
            <button title={t("与当前磁盘文件比较")} aria-label={t("与当前磁盘文件比较")} disabled={busy || item.status !== "available"} onclick={() => inspect(item, true)}><Columns2 size={16} /></button>
            <button title={t("另存恢复到新文件")} aria-label={t("另存恢复到新文件")} disabled={busy || recoveryState.loading || item.status !== "available"} onclick={() => operate("restore", item)}><Save size={16} /></button>
            <button title={t("删除副本文件及记录")} aria-label={t("删除副本文件及记录")} disabled={busy || recoveryState.loading || item.status !== "available"} onclick={() => operate("delete", item)}><Trash2 size={16} /></button>
            <button title={t("仅移除记录，保留文件")} aria-label={t("仅移除记录，保留文件")} disabled={busy || recoveryState.loading} onclick={() => operate("forget", item)}><ListX size={16} /></button>
          </div>
        </article>
      {:else}
        {#if !recoveryState.loading}<p>{t("没有已登记的恢复副本。")}</p>{/if}
      {/each}
      {#if recoveryState.nextOffset !== null}
        <button class="more" disabled={busy || recoveryState.loading} onclick={() => manager.load(recoveryState.nextOffset!)}>{t("加载更多")}</button>
      {/if}
    </div>
    {#if reading}<p role="status">{t("正在读取副本…")}</p>{/if}
    {#if preview}
      <section class="preview" aria-label={t("副本内容")}>
        <header><strong>{preview.path}</strong><button title={t("关闭预览")} aria-label={t("关闭预览")} onclick={clearPreview}><X size={16} /></button></header>
        <pre>{preview.content}</pre>
      </section>
    {:else if comparison && comparisonModule}
      <div class="comparison">
        {#await comparisonModule then module}
          {#key comparison.sequence}
            <module.default snapshot={comparison} stale={false} draftLabel={t("磁盘快照")}
              onClose={clearPreview} onRefresh={() => { if (selected) void inspect(selected, true); }} />
          {/key}
        {:catch cause}<p role="alert">{tm(String(cause))}</p>{/await}
      </div>
    {/if}
  </div>
</section>

<style>
  .recovery-panel { position: absolute; inset: 0; z-index: 15; display: flex; flex-direction: column; min-width: 0; background: var(--page-bg); color: var(--text); }
  header { display: flex; align-items: center; gap: 4px; padding: 8px 12px; border-bottom: 1px solid var(--border); }
  h2 { flex: 1; margin: 0; font-size: 14px; min-width: 0; overflow-wrap: anywhere; }
  h2 span { font-size: 12px; font-weight: normal; color: var(--text-muted); }
  button { display: grid; place-items: center; width: 28px; height: 28px; padding: 0; flex: 0 0 28px; border: 0; border-radius: 4px; background: transparent; color: var(--text); cursor: pointer; }
  button:hover { background: var(--surface-hover); }
  button:disabled { opacity: .4; cursor: default; }
  .recovery-body { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .records { flex: 1; overflow: auto; min-height: 0; }
  .with-preview .records { flex: 0 1 35%; border-bottom: 1px solid var(--border); }
  .count, p { padding: 8px 12px; margin: 0; font-size: 12px; overflow-wrap: anywhere; }
  article { padding: 8px 12px; border-bottom: 1px solid var(--border); display: grid; gap: 4px; }
  article.selected { background: var(--surface); }
  strong, code, small { min-width: 0; overflow-wrap: anywhere; font-size: 12px; }
  small { color: var(--text-muted); }
  .actions { display: flex; gap: 4px; }
  .more { width: auto; margin: 8px 12px; padding: 0 12px; }
  .preview, .comparison { flex: 1; min-height: 0; min-width: 0; overflow: hidden; }
  .preview { display: flex; flex-direction: column; }
  .preview strong { flex: 1; }
  pre { flex: 1; overflow: auto; margin: 0; padding: 12px; font-size: 12px; tab-size: 4; }
</style>
