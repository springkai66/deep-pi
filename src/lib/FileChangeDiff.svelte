<script lang="ts">
  import { FileDiff, FilePlus2 } from "@lucide/svelte";
  import { t } from "$lib/i18n.svelte";
  import {
    trailingNewlineDiffers,
    type FileChange,
    type LineDiff,
  } from "./file-change-diff";

  let {
    change,
    diff,
    tone = "plain",
    defaultOpen = true,
  }: {
    change: FileChange;
    diff: LineDiff;
    /** pending=运行中（拟变更） done=成功 failed=失败 plain=历史参数对比 */
    tone?: "pending" | "done" | "failed" | "plain";
    defaultOpen?: boolean;
  } = $props();

  const actionLabel = $derived.by(() => {
    if (tone === "failed") return t("调用失败");
    const write = change.tool === "write";
    if (tone === "pending") return write ? t("拟写入") : t("拟修改");
    if (tone === "done") return write ? t("已写入") : t("已修改");
    return write ? t("写入参数对比") : t("修改参数对比");
  });
  const stats = $derived(
    change.tool === "write"
      ? t("+{added} · 原内容未知", { added: diff.added })
      : `+${diff.added} −${diff.removed}`,
  );
  const collapsedTotal = $derived(
    diff.rows.reduce((sum, row) => sum + (row.kind === "gap" && row.count ? row.count : 0), 0),
  );
</script>

<details class="file-change-diff" open={defaultOpen}>
  <summary>
    {#if change.tool === "write"}<FilePlus2 size={14} aria-hidden="true" />
    {:else}<FileDiff size={14} aria-hidden="true" />{/if}
    <span class="fc-path" title={change.path}>{change.path}</span>
    <span class="fc-badge fc-tone-{tone}">{actionLabel}</span>
    <span class="fc-stats">
      {#if diff.truncated}
        {t("差异过大，未展开显示")}
      {:else if diff.newlineOnly}
        {t("仅换行符不同")}
      {:else if diff.equal}
        {t("内容相同")}
      {:else}
        <span class="fc-added">+{diff.added}</span>
        {#if change.tool !== "write"}<span class="fc-removed">−{diff.removed}</span>{/if}
      {/if}
    </span>
  </summary>
  {#if diff.truncated}
    <p class="fc-note">{t("文件过大，差异未展开显示。可在编辑器中查看该文件。")}</p>
  {:else if diff.newlineOnly}
    <p class="fc-note">{t("内容一致，仅换行符风格不同（CRLF/LF）。")}</p>
  {:else if diff.equal}
    <p class="fc-note">{t("内容相同。")}</p>
  {:else}
    {#if trailingNewlineDiffers(change.before, change.after)}
      <p class="fc-note">{t("注意：两侧的末尾换行不同。")}</p>
    {/if}
    <div class="fc-rows" role="table" aria-label={t("差异对比")}>
      {#each diff.rows as row, index (index)}
        {#if row.kind === "gap"}
          <div class="fc-row fc-gap" role="row">
            <span class="fc-gutter">⋯</span>
            <span class="fc-text">{t("已折叠 {count} 行", { count: row.count ?? 0 })}</span>
          </div>
        {:else}
          <div class="fc-row" class:fc-add={row.kind === "add"} class:fc-del={row.kind === "del"} role="row">
            <span class="fc-gutter">{row.oldNo ?? ""}</span>
            <span class="fc-gutter">{row.newNo ?? ""}</span>
            <span class="fc-prefix">{row.kind === "add" ? "+" : row.kind === "del" ? "−" : ""}</span>
            <span class="fc-text">{row.text === "" ? " " : row.text}</span>
          </div>
        {/if}
      {/each}
    </div>
    {#if collapsedTotal > 0}
      <p class="fc-note">{t("已折叠 {count} 行未变化的上下文。", { count: collapsedTotal })}</p>
    {/if}
    <p class="fc-note fc-hint">{t("行号为本次变更片段内的行号，不是文件绝对行号。")}</p>
  {/if}
</details>

<style>
  .file-change-diff {
    margin: 6px 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    background: var(--surface);
    min-width: 0;
  }
  summary {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 9px;
    cursor: pointer;
    color: var(--text);
    list-style: none;
    min-width: 0;
  }
  summary::-webkit-details-marker { display: none; }
  .fc-path {
    font: 12px var(--code-font);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex-shrink: 1;
  }
  .fc-badge {
    flex-shrink: 0;
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 11px;
    border: 1px solid var(--border);
    color: var(--text-muted);
  }
  .fc-tone-pending { color: var(--status-running, var(--accent)); border-color: currentColor; }
  .fc-tone-done { color: var(--status-done, var(--accent)); border-color: currentColor; }
  .fc-tone-failed { color: var(--status-failed); border-color: currentColor; }
  .fc-stats {
    flex-shrink: 0;
    margin-left: auto;
    font: 11px var(--code-font);
    color: var(--text-muted);
    display: inline-flex;
    gap: 6px;
  }
  .fc-added { color: #329b61; }
  .fc-removed { color: #d94d58; }
  .fc-note { margin: 0; padding: 6px 10px; font-size: 12px; color: var(--text-muted); }
  .fc-hint { border-top: 1px dashed var(--border); }
  .fc-rows {
    display: flex;
    flex-direction: column;
    font: 12px/1.55 var(--code-font);
    overflow: auto;
    max-height: 380px;
    border-top: 1px solid var(--border);
  }
  .fc-row { display: flex; min-width: 0; }
  .fc-gutter {
    flex-shrink: 0;
    width: 4ch;
    padding: 0 4px;
    text-align: right;
    color: var(--text-muted);
    user-select: none;
    border-right: 1px solid var(--border);
  }
  .fc-prefix { width: 1.4ch; flex-shrink: 0; text-align: center; user-select: none; }
  .fc-text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    min-width: 0;
    flex: 1;
    padding-right: 8px;
  }
  .fc-row.fc-add { background: color-mix(in srgb, #329b61 16%, transparent); }
  .fc-row.fc-del { background: color-mix(in srgb, #d94d58 16%, transparent); }
  .fc-row.fc-add .fc-prefix { color: #329b61; }
  .fc-row.fc-del .fc-prefix { color: #d94d58; }
  .fc-row.fc-gap { color: var(--text-muted); background: var(--surface); font-size: 11px; }
  .fc-row.fc-gap .fc-gutter { border-right: 0; }
</style>
