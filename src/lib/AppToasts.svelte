<script lang="ts">
  import { AlertTriangle, Check, Info, X } from "@lucide/svelte";
  import { notices } from "./notices.svelte";
  import { t, tm } from "$lib/i18n.svelte";
</script>

{#if notices.items.length > 0}
  <div class="toast-layer" role="region" aria-label={t("提示")}>
    {#each notices.items as notice (notice.id)}
      <div class="toast" class:error={notice.tone === "error"} class:success={notice.tone === "success"}>
        <span class="toast-icon" aria-hidden="true">
          {#if notice.tone === "error"}
            <AlertTriangle size={14} />
          {:else if notice.tone === "success"}
            <Check size={14} />
          {:else}
            <Info size={14} />
          {/if}
        </span>
        <span class="toast-text" role={notice.tone === "error" ? "alert" : "status"}>{tm(notice.message)}</span>
        <button class="toast-close" type="button" aria-label={t("关闭提示")} title={t("关闭提示")}
          onclick={() => notices.dismiss(notice.id)}><X size={12} /></button>
      </div>
    {/each}
  </div>
{/if}

<style>
  /* 顶部居中的浮字提示：背景贴着文字范围，不遮挡操作，也不阻塞输入。 */
  .toast-layer { position: fixed; z-index: 40; top: 14px; left: 50%; transform: translateX(-50%); display: grid; gap: 8px; justify-items: center; pointer-events: none; }
  .toast { pointer-events: auto; display: inline-flex; align-items: flex-start; gap: 8px; width: fit-content; max-width: min(560px, calc(100vw - 32px)); padding: 8px 10px; border: 1px solid var(--border-strong); border-radius: 7px; color: var(--text); background: var(--surface); box-shadow: 0 10px 30px rgb(0 0 0 / 22%); font-family: var(--text-font); font-size: 12px; animation: toast-in 140ms ease-out; }
  .toast-icon { display: inline-flex; flex-shrink: 0; margin-top: 1px; color: var(--text-muted); }
  .toast.error .toast-icon { color: var(--status-failed); }
  .toast.success .toast-icon { color: var(--accent); }
  .toast-text { min-width: 0; overflow-wrap: anywhere; line-height: 1.5; }
  .toast-close { display: inline-flex; flex-shrink: 0; align-items: center; justify-content: center; width: 20px; height: 20px; margin: -2px -2px 0 0; padding: 0; border: 1px solid transparent; border-radius: 4px; color: var(--text-muted); background: transparent; cursor: pointer; }
  .toast-close:hover { border-color: var(--border-strong); color: var(--text-strong); background: var(--surface-hover); }
  @keyframes toast-in { from { opacity: 0; transform: translateY(-6px); } to { opacity: 1; transform: none; } }
</style>
