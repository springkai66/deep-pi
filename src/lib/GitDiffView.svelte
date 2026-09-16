<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { FileDiff, RefreshCw, Square, X } from "@lucide/svelte";
  import { onDestroy, onMount, untrack } from "svelte";
  import { createGitStatusLoader, type GitLoadState } from "./git-status";
  import { DIFF_AREA_LABELS, diffScrollPosition, parseUnifiedDiff, type ConflictSide, type GitDiff, type GitDiffSelection } from "./git-diff";
  import { t, tm } from "$lib/i18n.svelte";

  type DiffReadRequest = GitDiffSelection & { conflictSide: ConflictSide };
  let { selection, refreshToken, onClose, cancelRead = (operationId: string) => invoke<void>("cancel_git_read", { operationId }), readDiff = (request: DiffReadRequest, operationId: string) => invoke<GitDiff>("project_git_diff", {
    operationId,
    projectId: request.projectId,
    request: { path: request.path, area: request.area, conflictSide: request.conflictSide },
  }) }: {
    selection: GitDiffSelection; refreshToken: number; onClose: () => void;
    readDiff?: (request: DiffReadRequest, operationId: string) => Promise<GitDiff>;
    cancelRead?: (operationId: string) => Promise<void>;
  } = $props();
  let conflictSide = $state<ConflictSide>("ours");
  let localRefresh = $state(0);
  let loadState = $state<GitLoadState<GitDiff>>({ loading: false, value: null, error: "" });
  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let height = $state(600);
  let loadedKey = "";
  const rowHeight = 22;
  const rows = $derived(parseUnifiedDiff(loadState.value?.patch ?? ""));
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - 10));
  const end = $derived(Math.min(rows.length, start + Math.ceil(height / rowHeight) + 20));
  const widthChars = $derived(Math.max(40, ...rows.map((row) => {
    let width = 0;
    for (const char of row.text) width += char === "\t" ? 4 : char.codePointAt(0)! > 127 ? 2 : 1;
    return width;
  })));
  const loader = createGitStatusLoader(
    (request: DiffReadRequest, operationId) => readDiff(request, operationId),
    (next) => {
      const previous = untrack(() => loadState);
      loadState = next.loading ? { ...previous, loading: true, error: "" } : next;
    },
    (operationId) => cancelRead(operationId),
  );
  $effect(() => {
    void refreshToken; void localRefresh;
    const request = { ...selection, conflictSide };
    const key = JSON.stringify(request);
    if (loadedKey !== key) {
      loadedKey = key;
      scrollTop = 0;
      loadState = { loading: false, value: null, error: "" };
      const element = untrack(() => viewport);
      if (element) element.scrollTop = 0;
    }
    loader.load(request);
  });
  onDestroy(() => loader.dispose());
  onMount(() => {
    const refresh = () => { if (!loadState.loading) localRefresh++; };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });
  function scrollWithKeyboard(event: KeyboardEvent) {
    if (!viewport) return;
    const position = diffScrollPosition(event, {
      top: viewport.scrollTop, left: viewport.scrollLeft,
      height: viewport.clientHeight, scrollHeight: viewport.scrollHeight,
    });
    if (!position) return;
    event.preventDefault();
    viewport.scrollTo(position);
  }
</script>

<section class="git-diff-view" aria-label={t("文件差异")} aria-busy={loadState.loading}>
  <header>
    <FileDiff size={16} />
    <strong title={selection.path}>{selection.path}</strong>
    <span>{t(DIFF_AREA_LABELS[selection.area])}</span>
    {#if loadState.loading}
      <button type="button" aria-label={t("取消差异读取")} title={t("取消差异读取")} onclick={() => loader.cancel()}><Square size={14} /></button>
    {/if}
    <button type="button" aria-label={t("刷新差异")} title={t("刷新差异")} disabled={loadState.loading} onclick={() => { localRefresh++; }}><RefreshCw size={15} /></button>
    <button type="button" aria-label={t("关闭差异")} title={t("关闭差异")} onclick={onClose}><X size={16} /></button>
  </header>
  {#if selection.area === "conflict"}
    <div class="conflict-options" role="group" aria-label={t("工作区的比较基准")}>
      <button type="button" aria-pressed={conflictSide === "base"} onclick={() => { conflictSide = "base"; }}>{t("基准")}</button>
      <button type="button" aria-pressed={conflictSide === "ours"} onclick={() => { conflictSide = "ours"; }}>{t("当前分支")}</button>
      <button type="button" aria-pressed={conflictSide === "theirs"} onclick={() => { conflictSide = "theirs"; }}>{t("合入分支")}</button>
      <span>{t("与工作区比较")}</span>
    </div>
  {/if}
  {#if loadState.loading && !loadState.value}
    <p role="status">{t("正在读取差异…")}</p>
  {:else if loadState.error}
    <p role="alert">{tm(loadState.error)}</p>
  {:else if loadState.value}
    {@const diff = loadState.value}
    {#if diff.truncated}<p class="warning" role="status">{t("差异超过显示上限，仅展示部分内容。")}</p>{/if}
    {#if diff.sourceOutsideProject}<p class="warning">{t("重命名源位于项目范围外，仅比较本项目内容。")}</p>{/if}
    {#if diff.format === "binary"}
      <p role="status">{t("二进制文件，无法显示文本差异。")}</p>
    {:else if diff.format === "unsupportedEncoding"}
      <p role="status">{t("文件不是有效 UTF-8，未进行有损转换。请使用支持该编码的外部编辑器查看。")}</p>
    {:else if !rows.length}
      <p role="status">{selection.area === "conflict" ? t("所选侧没有可显示的文本差异，文件仍处于冲突状态。") : t("当前没有可显示的文本差异。")}</p>
    {:else}
      <div class="diff-scroll" bind:this={viewport} bind:clientHeight={height}
        onkeydown={scrollWithKeyboard}
        onscroll={(event) => { scrollTop = event.currentTarget.scrollTop; }}
        tabindex="0" role="textbox" aria-readonly="true" aria-multiline="true" aria-label={t("统一差异，左侧为原始行号，右侧为新行号")}>
        <div class="diff-lines" style:height={`${rows.length * rowHeight}px`} style:min-width={`calc(14ch + ${widthChars}ch)`}>
          <div class="visible-lines" style:transform={`translateY(${start * rowHeight}px)`}>
            {#each rows.slice(start, end) as row, index (start + index)}
              <div class="diff-row" class:add={row.kind === "add"} class:deletion={row.kind === "delete"}
                class:metadata={row.kind === "meta" || row.kind === "hunk" || row.kind === "note"}>
                <span class="line-number">{row.oldLine ?? ""}</span>
                <span class="line-number">{row.newLine ?? ""}</span>
                <span class="prefix">{row.kind === "add" ? "+" : row.kind === "delete" ? "-" : ""}</span>
                <code>{row.text}</code>
              </div>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  {/if}
</section>

<style>
  .git-diff-view { position: absolute; inset: 0; z-index: 2; min-width: 0; min-height: 0; display: flex; flex-direction: column; background: var(--page-bg); }
  header { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; min-height: 40px; padding: 4px 12px; border-bottom: 1px solid var(--border); background: var(--surface); }
  header :global(svg) { flex-shrink: 0; }
  strong { flex: 1; min-width: 50px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  header span { color: var(--text-muted); font-size: 11px; }
  header button { display: grid; place-items: center; flex-shrink: 0; width: 28px; height: 28px; padding: 0; border: 0; border-radius: 4px; background: transparent; color: var(--text); cursor: pointer; }
  button:disabled { opacity: .4; cursor: default; }
  button:hover:not(:disabled) { background: var(--surface-hover); }
  .conflict-options { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; padding: 6px 12px; border-bottom: 1px solid var(--border); }
  .conflict-options button { border: 1px solid var(--border); border-radius: 3px; background: var(--surface); color: var(--text); padding: 4px 8px; font-size: 12px; cursor: pointer; }
  .conflict-options button[aria-pressed="true"] { color: var(--accent); background: var(--surface-hover); border-color: var(--accent); }
  .conflict-options span { color: var(--text-muted); font-size: 11px; }
  p { margin: 0; padding: 12px; color: var(--text-muted); font-size: 12px; overflow-wrap: anywhere; }
  p.warning { border-bottom: 1px solid var(--border); }
  .diff-scroll { flex: 1; min-height: 0; overflow: auto; font: 12px/22px var(--code-font); tab-size: 4; }
  .diff-lines { position: relative; }
  .visible-lines { position: absolute; inset: 0 0 auto; }
  .diff-row { display: grid; grid-template-columns: 5ch 5ch 2ch minmax(0, 1fr); height: 22px; padding-right: 12px; white-space: pre; }
  code { font: inherit; }
  .line-number { color: var(--text-muted); text-align: right; padding-right: 1ch; user-select: none; border-right: 1px solid var(--border); }
  .prefix { text-align: center; user-select: none; }
  .add { background: color-mix(in srgb, #329b61 16%, transparent); }
  .deletion { background: color-mix(in srgb, #d94d58 16%, transparent); }
  .metadata { color: var(--text-muted); background: var(--surface); }
</style>
