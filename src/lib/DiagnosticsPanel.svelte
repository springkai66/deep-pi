<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { Download, RefreshCw, Trash2 } from "@lucide/svelte";
  import { createDiagnosticsController, DIAGNOSTIC_LABELS, type DiagnosticsState } from "./diagnostics";

  let { confirmClear, onBusyChange }: {
    confirmClear: () => Promise<boolean>;
    onBusyChange: (busy: boolean) => void;
  } = $props();
  let diagnosticState = $state<DiagnosticsState>({ report: null, busy: false, error: "", status: "" });
  const controller = createDiagnosticsController({
    invoke: (command, args) => invoke(command, args), confirm: () => confirmClear(),
    publish: (next) => { diagnosticState = next; }, busy: (busy) => onBusyChange(busy),
  });
  let run = $state("");
  const runs = $derived([...new Set(diagnosticState.report?.events.map((event) => event.run) ?? [])]);
  const events = $derived((diagnosticState.report?.events ?? []).filter((event) => !run || String(event.run) === run).toReversed());
  $effect(() => { if (run && !runs.some((id) => String(id) === run)) run = ""; });
  onMount(() => {
    if (isTauri()) void controller.refresh();
    return () => controller.dispose();
  });
</script>

<section class="settings-group" aria-labelledby="rpc-diagnostics-heading" aria-busy={diagnosticState.busy}>
  <div class="settings-group-header">
    <h3 id="rpc-diagnostics-heading">Pi RPC 诊断</h3>
    <div class="actions">
      <button class="quiet-button icon-button" title="刷新诊断" aria-label="刷新诊断" disabled={diagnosticState.busy || !isTauri()} onclick={() => void controller.refresh()}><RefreshCw size={16} /></button>
      <button class="quiet-button icon-button" title="清空诊断记录" aria-label="清空诊断记录" disabled={diagnosticState.busy || !diagnosticState.report} onclick={() => void controller.clear()}><Trash2 size={16} /></button>
      <button class="quiet-button icon-button" title="导出当前快照（新文件）" aria-label="导出当前快照（新文件）" disabled={diagnosticState.busy || !diagnosticState.report} onclick={() => void controller.export()}><Download size={16} /></button>
    </div>
  </div>
  {#if !isTauri()}<p class="muted" role="status">诊断仅在桌面应用可用</p>{/if}
  {#if diagnosticState.error}<p role="alert">{diagnosticState.error}</p>{/if}
  {#if diagnosticState.busy}<p class="muted" role="status">正在处理…</p>
  {:else if diagnosticState.status}<p class="muted" role="status">{diagnosticState.status}</p>{/if}
  {#if diagnosticState.report}
    <dl class="metadata">
      <div><dt>应用</dt><dd>{diagnosticState.report.appVersion}</dd></div>
      <div><dt>系统</dt><dd>{diagnosticState.report.os} / {diagnosticState.report.arch}</dd></div>
      <div><dt>采集范围</dt><dd>本次应用运行 · Pi RPC · 最多 256 条</dd></div>
      <div><dt>隐私</dt><dd>不含 stderr 原文、提示词、文件内容、路径与凭据</dd></div>
    </dl>
    <label class="setting-control"><strong>运行批次</strong>
      <select bind:value={run} aria-label="筛选诊断运行批次">
        <option value="">全部</option>
        {#each runs as id (id)}<option value={String(id)}>#{id}</option>{/each}
      </select>
    </label>
    {#if diagnosticState.report.droppedEvents > 0}<p class="muted">较早的 {diagnosticState.report.droppedEvents} 条记录已被容量限制移除</p>{/if}
    {#if events.length === 0}<p class="muted" role="status">暂无诊断事件</p>
    {:else}
      <ol class="events" aria-label="诊断事件">
        {#each events as event (event.sequence)}
          <li>
            <div class="event-heading"><strong>{DIAGNOSTIC_LABELS[event.code]}</strong><span>#{event.run} · +{(event.elapsedMs / 1000).toFixed(1)}s</span></div>
            <div class="event-detail"><code>{event.code}</code><span>{event.code === "stderr_observed" ? `${event.count} B` : `× ${event.count}`}{event.exitCode !== null ? ` · exit ${event.exitCode}` : ""}</span></div>
          </li>
        {/each}
      </ol>
    {/if}
    <details><summary>报告 JSON</summary><pre>{JSON.stringify(diagnosticState.report, null, 2)}</pre></details>
  {/if}
</section>

<style>
  .actions { display: flex; gap: 6px; }
  .metadata { margin: 0; font-size: 12px; }
  .metadata div { display: grid; grid-template-columns: 84px minmax(0, 1fr); gap: 8px; padding: 6px 0; }
  dt { color: var(--text-muted); }
  dd { margin: 0; overflow-wrap: anywhere; }
  .events { margin: 12px 0; padding: 0; list-style: none; max-height: 360px; overflow: auto; border-block: 1px solid var(--border); }
  li { padding: 10px 2px; border-bottom: 1px solid var(--border); font-size: 12px; }
  .event-heading, .event-detail { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 6px; }
  .event-heading strong { font-weight: 500; }
  .event-heading span, .event-detail { color: var(--text-muted); }
  .event-detail { margin-top: 6px; }
  code { font-family: var(--code-font); overflow-wrap: anywhere; }
  details { font-size: 12px; }
  summary { cursor: pointer; padding: 8px 0; }
  pre { max-height: 320px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; padding: 12px; background: var(--surface-alt); font-family: var(--code-font); }
</style>
