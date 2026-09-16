<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ExternalLink, Save, CopyPlus, RefreshCw, X, GitCompareArrows, FileText } from "@lucide/svelte";
  import { onDestroy, onMount, untrack } from "svelte";
  import { documentDirty, type FileDocument, type FileWorkspace } from "./file-workspace";
  import { newlineLabel } from "./editor-document";
  import type { FilePreview } from "./files";
  import type { DialogRequest, DialogValue } from "./dialog";
  import CodeEditor from "./CodeEditor.svelte";
  import { createFileComparison, comparisonSources, comparisonStale, type ComparisonState, type ComparisonSourceKind } from "./file-comparison";
  import { shortcutLabel, shortcutAria } from "./shortcuts";
  import { t, tm } from "$lib/i18n.svelte";

  let { documents, controller, projectId, path, visible, line, column, refreshToken,
    editorConfigured, onConfigureEditor, onSelect, onHide, requestDialog, invokeCommand = invoke }: {
    documents: FileDocument[]; controller: FileWorkspace; projectId: string; path: string;
    visible: boolean; line?: number; column?: number; refreshToken: number;
    editorConfigured: boolean; onConfigureEditor: () => void; onHide: () => void;
    onSelect: (path: string) => void;
    requestDialog: (request: Omit<DialogRequest, "id" | "resolve">) => Promise<DialogValue>;
    invokeCommand?: typeof invoke;
  } = $props();
  let current = $derived(documents.find((doc) => doc.id === controller.id(projectId, path)));
  let tabs = $derived(documents.filter((doc) => doc.projectId === projectId));
  let notice = $state("");
  let comparing = $state(false);
  let comparisonKind = $state<ComparisonSourceKind>("disk");
  let comparison = $state.raw<ComparisonState>({ status: "idle", snapshot: null, error: "" });
  let comparisonModule = $state.raw<Promise<typeof import("./FileComparison.svelte")> | null>(null);
  const comparisons = createFileComparison(
    (projectId, relativePath) => invokeCommand<FilePreview>("read_project_file", { projectId, relativePath }),
    (state) => { comparison = state; },
  );
  const sources = $derived(current ? comparisonSources(current) : []);
  const staleComparison = $derived(!!comparison.snapshot && comparisonStale(comparison.snapshot, current));
  let launching = $state(false);
  $effect(() => {
    void path; void projectId;
    comparisons.reset();
    comparisonKind = "disk";
    comparing = false;
    notice = "";
  });
  $effect(() => {
    if (!sources.some((source) => source.kind === comparisonKind)) comparisonKind = "disk";
  });
  onDestroy(() => comparisons.reset());
  $effect(() => {
    const id = controller.id(projectId, path);
    void refreshToken;
    if (visible) untrack(() => { void controller.refresh(id); });
  });
  onMount(() => {
    const refresh = () => { if (visible && current) void controller.refresh(current.id); };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });
  export async function save(asNew = false) {
    const doc = current;
    if (!doc || doc.saving || doc.locked) return;
    let target: string | undefined;
    if (asNew) {
      const result = await requestDialog({ kind: "input", title: t("另存为"), message: t("输入当前项目内的新文件路径。已有文件不会被覆盖。"),
        initialValue: doc.path, confirmLabel: t("另存"), maxLength: 4096 });
      if (typeof result !== "string" || !result) return;
      target = result;
    }
    const next = await controller.save(doc.id, target);
    if (next && current?.id === doc.id) {
      const saved = controller.get(next);
      if (saved) onSelect(saved.path);
    }
  }
  async function close(id: string) {
    const wasCurrent = current?.id === id;
    if (await controller.close(id) && wasCurrent) {
      const next = documents.find((doc) => doc.projectId === projectId && doc.id !== id);
      if (next) onSelect(next.path); else onHide();
    }
  }
  async function compare() {
    const doc = current;
    if (!doc?.state) return;
    comparing = true;
    if (!comparisonModule) comparisonModule = import("./FileComparison.svelte");
    await comparisons.load(doc, comparisonKind);
  }
  function closeComparison() {
    comparing = false;
    comparisons.reset();
  }
  async function external() {
    if (!editorConfigured) { onConfigureEditor(); return; }
    if (launching) return;
    launching = true;
    try {
      await invokeCommand("open_project_in_editor", { projectId, relativePath: path, line: line ?? null, column: column ?? null });
      notice = t("已发送到外部编辑器；内置草稿未自动写入磁盘。");
    } catch (error) { notice = tm(String(error)); }
    finally { launching = false; }
  }
  function tabKey(event: KeyboardEvent, index: number) {
    if (event.isComposing || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1
      : (index + (event.key === "ArrowLeft" ? -1 : 1) + tabs.length) % tabs.length;
    onSelect(tabs[next].path);
    const parent = (event.currentTarget as HTMLElement).closest('[role="tablist"]');
    (parent?.querySelectorAll('[role="tab"]')[next] as HTMLElement)?.focus();
  }
</script>

<section class="file-editor" class:hidden={!visible} aria-label={t("文件编辑")}>
  <div class="file-tabs" role="tablist" aria-label={t("已打开文件")}>
    {#each tabs as doc, index (doc.id)}
      <div class="file-tab" class:active={doc.id === current?.id}>
        <button type="button" role="tab" aria-selected={doc.id === current?.id} tabindex={doc.id === current?.id ? 0 : -1}
          title={doc.path} onclick={() => onSelect(doc.path)} onkeydown={(event) => tabKey(event, index)}>
          <FileText size={14} /><span>{doc.path.split("/").pop()}</span>{#if documentDirty(doc)}<span aria-label={t("未保存")}>*</span>{/if}
        </button>
        <button type="button" class="icon" title={t("关闭 {path}", { path: doc.path })} aria-label={t("关闭 {path}", { path: doc.path })} disabled={doc.saving || doc.locked} onclick={() => void close(doc.id)}><X size={14} /></button>
      </div>
    {/each}
  </div>
  <header>
    <strong title={path}>{path}</strong>
    <button type="button" class="icon" title={t("保存 ({shortcut})", { shortcut: shortcutLabel("save") })} aria-label={t("保存文件")} aria-keyshortcuts={shortcutAria("save")} disabled={!current?.state || current.saving || current.locked} onclick={() => void save()}><Save size={16} /></button>
    <button type="button" class="icon" title={t("另存为 ({shortcut})", { shortcut: shortcutLabel("saveAs") })} aria-label={t("另存为")} aria-keyshortcuts={shortcutAria("saveAs")} disabled={!current?.state || current.saving || current.locked} onclick={() => void save(true)}><CopyPlus size={16} /></button>
    <button type="button" class="icon" title={t("比较磁盘或恢复副本")} aria-label={t("比较磁盘文件")} disabled={!current?.state || comparison.status === "loading"} onclick={() => void compare()}><GitCompareArrows size={16} /></button>
    <button type="button" class="icon" title={t("重新载入")} aria-label={t("重新载入文件")} disabled={!current || current.loading || current.saving} onclick={() => current && void controller.reload(current.id)}><RefreshCw size={16} /></button>
    <button type="button" class="icon" title={editorConfigured ? t("外部编辑器") : t("配置外部编辑器")} aria-label={t("外部编辑器")} disabled={launching} onclick={() => void external()}><ExternalLink size={16} /></button>
    <button type="button" class="icon" title={t("返回任务")} aria-label={t("隐藏文件编辑器")} onclick={onHide}><X size={16} /></button>
  </header>
  {#if notice}<p role="status">{tm(notice)}</p>{/if}
  {#if current?.error}<p role="alert">{tm(current.error)}</p>{/if}
  {#if current?.issue}
    <div class="save-result" role={current.issue.outcome === "saved" ? "status" : "alert"}>
      <span>{tm(current.issue.detail)}</span>
      {#if current.issue.recoveryPath}<span>{t("恢复路径：{path}", { path: current.issue.recoveryPath })}</span>{/if}
      {#if current.issue.pendingPath}<span>{t("待写内容路径：{path}", { path: current.issue.pendingPath })}</span>{/if}
    </div>
  {/if}
  {#if current?.state}
    <div class="editor-body">
      <div class="editable" class:hidden={comparing}>
        {#key `${current.id}:${current.revision}`}
          <CodeEditor document={current} {line} {column} visible={visible && !comparing}
            onUpdate={controller.update}
            onScroll={controller.scroll} onSave={() => void save()} />
        {/key}
      </div>
      {#if comparing}
        <div class="comparison">
          <div class="comparison-source">
            <label for="comparison-source">{t("比较来源")}</label>
            <select id="comparison-source" bind:value={comparisonKind} onchange={() => void compare()}>
              {#each sources as source}<option value={source.kind}>{t(source.label)}</option>{/each}
            </select>
            {#if comparison.status !== "ready"}
              <button type="button" class="icon" aria-label={t("关闭比较")} title={t("关闭比较")} onclick={closeComparison}><X size={14} /></button>
            {/if}
          </div>
          {#if comparison.status === "loading"}<p role="status">{t("正在读取比较来源…")}</p>
          {:else if comparison.status === "error"}
            <p role="alert">{tm(comparison.error)}</p>
            <button type="button" class="comparison-retry" onclick={() => void compare()}><RefreshCw size={14} />{t("重新读取")}</button>
          {:else if comparison.snapshot && comparisonModule}
            {#await comparisonModule}
              <p role="status">{t("正在加载差异组件…")}</p>
              <button type="button" class="comparison-retry" onclick={closeComparison}>{t("返回编辑")}</button>
            {:then module}
              {#key comparison.snapshot.sequence}
                <module.default snapshot={comparison.snapshot} stale={staleComparison} {visible} onClose={closeComparison} onRefresh={() => void compare()} />
              {/key}
            {:catch error}
              <p role="alert">{t("差异组件加载失败：{error}", { error: tm(String(error)) })}</p>
              <button type="button" class="comparison-retry" onclick={() => { comparisonModule = import("./FileComparison.svelte"); }}>{t("重新加载")}</button>
              <button type="button" class="comparison-retry" onclick={closeComparison}>{t("返回编辑")}</button>
            {/await}
          {/if}
        </div>
      {/if}
    </div>
    <footer><span>UTF-8 · {newlineLabel(current.state.sliceDoc())}</span><span>{current.saving ? t("保存中") : documentDirty(current) ? t("未保存") : t("已保存")}</span></footer>
  {:else if current?.loading}<p role="status">{t("正在读取文件…")}</p>
  {/if}
</section>

<style>
  .file-editor { position: absolute; inset: 0; z-index: 2; display: flex; flex-direction: column; min-width: 0; background: var(--page-bg); }
  .hidden { display: none; }
  .file-tabs { display: flex; flex-shrink: 0; height: 36px; overflow-x: auto; background: var(--surface); border-bottom: 1px solid var(--border); }
  .file-tab { display: flex; align-items: center; flex: 0 0 auto; max-width: 240px; border-right: 1px solid var(--border); }
  .file-tab.active { background: var(--page-bg); box-shadow: inset 0 2px var(--accent); }
  .file-tab > button:first-child { display: flex; align-items: center; gap: 6px; min-width: 0; height: 34px; padding: 0 8px; }
  .file-tab span:first-of-type { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { border: 0; background: transparent; color: var(--text); cursor: pointer; font-size: 12px; }
  button:disabled { opacity: .4; cursor: default; }
  button:hover { background: var(--surface-hover); }
  .icon { display: grid; place-items: center; width: 28px; height: 28px; flex: 0 0 28px; border-radius: 4px; }
  header { display: flex; align-items: center; gap: 4px; min-height: 36px; padding: 4px 8px; border-bottom: 1px solid var(--border); }
  strong { flex: 1; min-width: 0; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  p, .save-result { margin: 0; padding: 8px 12px; font-size: 12px; overflow-wrap: anywhere; border-bottom: 1px solid var(--border); }
  .save-result { display: flex; flex-direction: column; gap: 4px; max-height: 120px; overflow: auto; }
  .editor-body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr); overflow: hidden; }
  .editable { min-width: 0; min-height: 0; overflow: hidden; }
  .comparison { display: flex; flex-direction: column; min-width: 0; min-height: 0; overflow: hidden; }
  .comparison-source { display: flex; align-items: center; gap: 8px; padding: 4px 12px; min-height: 36px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .comparison-source label { font-size: 12px; flex-shrink: 0; }
  .comparison-source select { min-width: 0; max-width: 320px; flex: 1; height: 28px; background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 4px; }
  .comparison-retry { display: inline-flex; align-items: center; gap: 6px; margin: 8px 12px; padding: 6px 10px; align-self: flex-start; border: 1px solid var(--border); border-radius: 4px; }
  footer { display: flex; justify-content: space-between; padding: 6px 12px; font-size: 11px; color: var(--text-muted); border-top: 1px solid var(--border); }
</style>
