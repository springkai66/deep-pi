<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { MergeView } from "@codemirror/merge";
  import { EditorView } from "@codemirror/view";
  import { ArrowUp, ArrowDown, X, RefreshCw } from "@lucide/svelte";
  import { createDiffConfig, diffMetadata, nextDiffIndex } from "./file-diff";
  import type { ComparisonSnapshot } from "./file-comparison";
  import { shortcutLabel, shortcutAria } from "./shortcuts";
  import { t, tm } from "$lib/i18n.svelte";

  let { snapshot, stale, onClose, onRefresh, visible = true, draftLabel = t("草稿快照") }: {
    snapshot: ComparisonSnapshot; stale: boolean; onClose: () => void; onRefresh: () => void;
    visible?: boolean;
    draftLabel?: string;
  } = $props();
  let host: HTMLDivElement;
  let view = $state.raw<MergeView | undefined>();
  let count = $state(0);
  let selected = $state(-1);
  let precise = $state(true);
  let error = $state("");
  const metadata = $derived(diffMetadata(snapshot.source.content, snapshot.draft));
  onMount(() => {
    try {
      view = new MergeView({ ...createDiffConfig(snapshot.source.content, snapshot.draft, navigate), parent: host });
      count = view.chunks.length;
      precise = view.chunks.every((chunk) => chunk.precise);
    } catch (cause) { error = tm(String(cause)); }
    return () => { view?.destroy(); view = undefined; };
  });
  $effect(() => {
    const current = view;
    if (current && visible) untrack(() => { current.a.requestMeasure(); current.b.requestMeasure(); });
  });
  function navigate(direction: 1 | -1) {
    if (!view || !count) return;
    selected = nextDiffIndex(selected, count, direction);
    const chunk = view.chunks[selected];
    for (const [editor, offset] of [[view.a, chunk.fromA], [view.b, chunk.fromB]] as const) {
      const anchor = Math.min(offset, editor.state.doc.length);
      editor.dispatch({ selection: { anchor }, effects: EditorView.scrollIntoView(anchor, { y: "center" }) });
    }
  }
</script>

<section class="file-comparison" aria-label={t("文件差异比较")}>
  <header>
    <strong>{t("差异比较")}</strong>
    <span role="status">{count ? selected < 0 ? t("{count} 处差异", { count }) : `${selected + 1} / ${count}` : metadata.equal ? t("内容相同") : metadata.newlineOnly ? t("仅换行符不同") : error ? t("比较失败") : t("正在比较")}</span>
    <button type="button" title={t("上一个差异 ({shortcut})", { shortcut: shortcutLabel("previousDiff") })} aria-label={t("上一个差异")} aria-keyshortcuts={shortcutAria("previousDiff")} disabled={!count} onclick={() => navigate(-1)}><ArrowUp size={16} /></button>
    <button type="button" title={t("下一个差异 ({shortcut})", { shortcut: shortcutLabel("nextDiff") })} aria-label={t("下一个差异")} aria-keyshortcuts={shortcutAria("nextDiff")} disabled={!count} onclick={() => navigate(1)}><ArrowDown size={16} /></button>
    <button type="button" title={t("重新比较")} aria-label={t("重新比较")} onclick={onRefresh}><RefreshCw size={16} /></button>
    <button type="button" title={t("返回编辑")} aria-label={t("关闭比较")} onclick={onClose}><X size={16} /></button>
  </header>
  {#if stale}<p role="status">{t("草稿或文件基线已变化，当前显示的是先前的比较快照。")}</p>{/if}
  {#if !precise}<p role="status">{t("部分差异按较大范围显示，未逐字展开。")}</p>{/if}
  {#if metadata.newlineOnly}<p role="status">{t("文本内容一致，但原始换行字节不同：{source} → {draft}。", { source: metadata.sourceNewline, draft: metadata.draftNewline })}</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <div class="side-labels">
    <div><strong title={snapshot.source.path}>{t(snapshot.source.label)} · {snapshot.source.path}</strong><span>UTF-8{metadata.sourceBom ? " BOM" : ""} · {metadata.sourceNewline}</span></div>
    <div><strong title={snapshot.path}>{draftLabel} · {snapshot.path}</strong><span>UTF-8{metadata.draftBom ? " BOM" : ""} · {metadata.draftNewline}</span></div>
  </div>
  <div class="diff-host" bind:this={host}></div>
</section>

<style>
  .file-comparison {
    --diff-delete-line: #5f263142; --diff-delete-text: #c2516866; --diff-delete-mark: #f08b9d;
    --diff-add-line: #275e4342; --diff-add-text: #368c6466; --diff-add-mark: #8fd6ad;
    display: flex; flex-direction: column; height: 100%; min-width: 0; min-height: 0; background: var(--page-bg);
  }
  :global(:root[data-color-scheme="light"]) .file-comparison {
    --diff-delete-line: #ffebe9; --diff-delete-text: #ffbcb7; --diff-delete-mark: #b42338;
    --diff-add-line: #dafbe1; --diff-add-text: #aceebb; --diff-add-mark: #217a40;
  }
  header { display: flex; align-items: center; gap: 4px; min-height: 36px; padding: 4px 8px; border-bottom: 1px solid var(--border); }
  header > strong { flex: 1; }
  header > span { color: var(--text-muted); font-size: 11px; min-width: 68px; text-align: right; }
  strong { font-size: 12px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { display: grid; place-items: center; padding: 0; border: 0; border-radius: 4px; background: transparent; color: var(--text); width: 28px; height: 28px; flex: 0 0 28px; cursor: pointer; }
  button:hover { background: var(--surface-hover); }
  button:disabled { opacity: .4; cursor: default; }
  p { margin: 0; padding: 8px 12px; font-size: 12px; overflow-wrap: anywhere; border-bottom: 1px solid var(--border); }
  .side-labels { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); border-bottom: 1px solid var(--border); background: var(--surface); }
  .side-labels > div { display: flex; flex-direction: column; min-width: 0; padding: 6px 10px; gap: 4px; }
  .side-labels > div + div { border-left: 1px solid var(--border); }
  .side-labels span { font-size: 11px; color: var(--text-muted); }
  .diff-host { flex: 1; min-height: 0; min-width: 0; overflow: hidden; }
  .diff-host :global(.cm-mergeView) { height: 100%; overflow: auto; }
  .diff-host :global(.cm-mergeViewEditors) { min-height: 100%; }
  .diff-host :global(.cm-mergeViewEditor) { min-width: 0; }
  .diff-host :global(.cm-mergeViewEditor + .cm-mergeViewEditor) { border-left: 1px solid var(--border); }
  .diff-host :global(.cm-changedText) { text-decoration-thickness: 1px; }
</style>
