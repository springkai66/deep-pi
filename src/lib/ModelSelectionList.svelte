<script lang="ts">
  import { Search } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import type { ModelSelectionRow } from "./model-selection";
  import { t } from "$lib/i18n.svelte";

  let { rows, selectedId, ready = true, disabled = false, saving = false, error = "", onPick, onToggle, onSelectAll, onClear, onRetry, tools }: {
    rows: ModelSelectionRow[];
    selectedId: string | null;
    ready?: boolean;
    disabled?: boolean;
    saving?: boolean;
    error?: string;
    onPick: (id: string) => void;
    onToggle: (id: string) => void;
    onSelectAll: () => void;
    onClear: () => void;
    onRetry: () => void;
    tools?: Snippet;
  } = $props();
  let search = $state("");
  let view = $state<"all" | "selected">("all");
  const checkedCount = $derived(rows.filter((row) => row.checked).length);
  const shown = $derived(rows.filter((row) => (view === "all" || row.checked)
    && `${row.name} ${row.id} ${row.sub}`.toLowerCase().includes(search.trim().toLowerCase())));
</script>

<aside class="editor-models" aria-label={t("模型列表")}>
  <div class="models-toolbar"><strong>{t("模型列表")}</strong><span>{t("已选 {selected}/{total}", { selected: ready ? checkedCount : 0, total: rows.length })}</span></div>
  <div class="model-views" role="group" aria-label={t("模型列表")}>
    <button type="button" class:active={view === "all"} aria-pressed={view === "all"} onclick={() => (view = "all")}>{t("全部模型")}</button>
    <button type="button" class:active={view === "selected"} aria-pressed={view === "selected"} onclick={() => (view = "selected")}>{t("已选模型")}</button>
  </div>
  <div class="model-search-row"><Search size={13} /><input bind:value={search} placeholder={t("搜索模型…")} aria-label={t("搜索模型")} autocomplete="off" /></div>
  <div class="selection-actions">
    <button type="button" disabled={!ready || disabled} onclick={onSelectAll}>{t("全选")}</button>
    <button type="button" disabled={!ready || disabled} onclick={onClear}>{t("清空选择")}</button>
  </div>
  {#if tools}{@render tools()}{/if}
  <div class="model-list">
    {#each shown as row (row.id)}
      <div class="model-row" class:selected={selectedId === row.id}>
        <input type="checkbox" checked={ready && row.checked} disabled={!ready || disabled || row.busy}
          aria-label={t("选择 {name}", { name: row.name })} onchange={() => onToggle(row.id)} />
        <button type="button" class="model-open" data-model-id={row.id} aria-pressed={selectedId === row.id} onclick={() => onPick(row.id)}>
          <strong>{row.name}</strong><small>{row.sub}</small>
        </button>
        {#if row.badge}<span class="badge">{row.badge}</span>{/if}
      </div>
    {:else}
      <p class="empty">{view === "selected" && checkedCount === 0 ? t("从全部模型中勾选要显示的模型") : t("没有匹配的模型")}</p>
    {/each}
  </div>
  {#if saving || error}
    <div class="selection-feedback">
      {#if saving}<span role="status">{t("正在保存…")}</span>{/if}
      {#if error}<span class="error" role="alert">{error}</span><button type="button" disabled={disabled} onclick={onRetry}>{t("重试")}</button>{/if}
    </div>
  {/if}
</aside>

<style>
  .editor-models { min-width: 0; min-height: 0; display: flex; flex-direction: column; gap: 6px; padding: 10px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-alt); }
  .models-toolbar, .model-search-row, .model-row, .selection-actions { display: flex; align-items: center; gap: 6px; }
  .models-toolbar { flex-shrink: 0; justify-content: space-between; min-height: 26px; color: var(--text-muted); font-size: 11px; }
  .models-toolbar span { flex-shrink: 0; }
  .model-views { flex-shrink: 0; display: flex; }
  .model-views button { flex: 1; border-radius: 0; }
  .model-views button:first-child { border-radius: 4px 0 0 4px; }
  .model-views button:last-child { margin-left: -1px; border-radius: 0 4px 4px 0; }
  .model-views button.active { position: relative; color: var(--accent); border-color: var(--accent); background: var(--surface-hover); }
  .model-search-row { flex-shrink: 0; color: var(--text-muted); }
  .model-search-row input { flex: 1; height: 28px; }
  .selection-actions { flex-shrink: 0; }
  .model-list { min-height: 0; flex: 1; overflow: auto; display: grid; align-content: start; gap: 6px; }
  .model-row { padding: 2px; border: 1px solid transparent; border-radius: 4px; }
  .model-row:hover, .model-row.selected { border-color: var(--border-strong); background: var(--surface-hover); }
  .model-row input[type="checkbox"] { flex: 0 0 auto; width: 15px; height: 15px; margin: 0 2px 0 4px; padding: 0; accent-color: var(--accent); cursor: pointer; }
  .model-open { flex: 1; min-width: 0; display: grid; gap: 2px; padding: 4px; border: 0; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  strong, small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  strong { color: var(--text-strong); font-size: 12px; font-weight: 650; }
  small { color: var(--text-muted); font-size: 10px; }
  .badge { flex-shrink: 0; color: var(--accent); font-size: 10px; padding-right: 4px; }
  .selection-feedback { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; color: var(--text-muted); font-size: 11px; }
  .error { color: var(--status-failed); overflow-wrap: anywhere; }
  .empty { margin: 10px 0; color: var(--text-muted); font-size: 11px; }
  input { box-sizing: border-box; width: 100%; min-width: 0; height: 30px; padding: 0 9px; border: 1px solid var(--border-strong); border-radius: 4px; outline: none; color: var(--text); background: var(--surface-alt); font: inherit; }
  button { min-height: 30px; padding: 0 9px; border: 1px solid var(--border-strong); border-radius: 4px; color: var(--text); background: var(--surface-raised); font: inherit; font-size: 11px; cursor: pointer; }
  .model-open { border: 0; background: transparent; }
  button:disabled { cursor: default; opacity: .45; }
</style>
