<script lang="ts">
  import { Ban, ChevronRight, Clock3, Download } from "@lucide/svelte";
  import { t, tm } from "$lib/i18n.svelte";
  import type { RuntimeUpdate } from "$lib/runtime";

  interface Props {
    updates: RuntimeUpdate[];
    busyRuntime: string | null;
    onInstall: (update: RuntimeUpdate) => void;
    onSnooze: (update: RuntimeUpdate) => void;
    onSkip: (update: RuntimeUpdate) => void;
  }

  let { updates, busyRuntime, onInstall, onSnooze, onSkip }: Props = $props();
  let index = $state(0);
  const current = $derived(updates.length ? updates[index % updates.length] : undefined);
</script>

{#if current}
  <div class="runtime-update-notice" role="status" aria-label={t("组件")}>
    <span class="runtime-update-name" title={`${current.name} ${current.latestVersion}${current.note ? ` · ${tm(current.note)}` : ""}`}>
      {current.name} · {current.latestVersion}
    </span>
    {#if updates.length > 1}
      <span class="runtime-update-count">{index % updates.length + 1}/{updates.length}</span>
      <button type="button" aria-label={t("下一项")} title={t("下一项")} onclick={() => { index += 1; }}><ChevronRight size={14} /></button>
    {/if}
    <button type="button" disabled={busyRuntime !== null} aria-label={`${t(current.currentVersion ? "更新" : "安装")} ${current.name} ${current.latestVersion}`} title={t(current.currentVersion ? "更新" : "安装")} onclick={() => onInstall(current)}><Download size={14} /></button>
    <button type="button" aria-label={`${t("稍后")} ${current.name}`} title={t("稍后")} onclick={() => onSnooze(current)}><Clock3 size={14} /></button>
    <button type="button" aria-label={`${t("跳过")} ${current.name} ${current.latestVersion}`} title={t("跳过")} onclick={() => onSkip(current)}><Ban size={14} /></button>
  </div>
{/if}

<style>
  .runtime-update-notice { display: flex; align-items: center; justify-self: end; min-width: 0; max-width: 100%; gap: 3px; padding: 0 4px 0 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface-alt); color: var(--text-strong); }
  .runtime-update-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; }
  .runtime-update-count { flex: none; color: var(--text-muted); font-size: 10px; }
  button { flex: none; display: grid; place-items: center; width: 21px; height: 23px; padding: 0; border: 0; border-radius: 3px; color: var(--text-muted); background: transparent; cursor: pointer; }
  button:hover { color: var(--accent); background: var(--surface-hover); }
  button:disabled { opacity: .45; cursor: default; }
  @media (max-width: 560px) { .runtime-update-count { display: none; } }
</style>
