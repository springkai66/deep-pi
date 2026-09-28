<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { RefreshCw } from "@lucide/svelte";
  import { t } from "./i18n.svelte";
  import { createQuotaTracker, quotaTarget, quotaWindowRows, type QuotaResponse, type QuotaState } from "./provider-quota";

  let { modelKey, visible, sessionKey }: { modelKey: string; visible: boolean; sessionKey: string } = $props();
  let state = $state<QuotaState | null>(null);
  const target = $derived(quotaTarget(modelKey));
  const windowRows = $derived(quotaWindowRows(state));
  const tracker = createQuotaTracker(
    (providerId) => invoke<QuotaResponse>("provider_quota", { request: { providerId } }),
    (next) => { state = next; },
  );

  $effect(() => {
    void sessionKey;
    if (!visible || !target) { tracker.stop(); return; }
    tracker.load(target);
    const timer = setInterval(() => tracker.load(target), 120_000);
    return () => { clearInterval(timer); tracker.stop(); };
  });

  const statusText = $derived.by(() => {
    if (!state) return "";
    if (state.status === "loading") return t("查询中");
    if (state.status === "missing") return t("未配置凭证");
    if (state.status === "unavailable") return t("官方额度暂不可查询");
    if (state.status !== "available" && state.status !== "exhausted") return t("额度查询失败");
    if ("windows" in state) return windowRows.length ? "" : t("额度查询失败");
    if (!Number.isFinite(state.amount) || !state.currency) return t("额度查询失败");
    const numeric = state.amount > 0 && state.amount < 0.000001
      ? state.amount.toExponential(2)
      : new Intl.NumberFormat(undefined, { maximumFractionDigits: 6 }).format(state.amount);
    const amount = `${state.currency} ${numeric}`;
    return state.status === "exhausted" ? t("余额已耗尽（{amount}）", { amount }) : t("余额 {amount}", { amount });
  });
</script>

{#if target && visible}
  <div class="provider-quota" role="status" title={target.label}>
    <div class="quota-content">
      <span class="provider-name">{target.label}</span>
      {#if windowRows.length}
        <div class="quota-windows">
          {#each windowRows as row (row.label)}
            <div class="quota-window" class:exhausted={row.remainingPercent === 0}>
              <span>{row.label === "5h" ? t("5 小时") : t("每周")}</span>
              <span>{t("剩余 {percent}%", { percent: new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(row.remainingPercent) })}</span>
            </div>
          {/each}
        </div>
      {:else}
        <span class="quota-value" class:exhausted={state?.status === "exhausted"} class:failed={state?.status === "error"}>{statusText}</span>
      {/if}
    </div>
    <button type="button" aria-label={t("刷新账户额度")} title={t("刷新账户额度")} disabled={state?.status === "loading"}
      onclick={() => tracker.load(target)}><RefreshCw size={13} /></button>
  </div>
{/if}

<style>
  .provider-quota { display: flex; align-items: flex-start; gap: 6px; width: 100%; min-width: 0; font-size: 11px; color: var(--text-muted); }
  .quota-content { display: flex; flex: 1; flex-direction: column; gap: 3px; min-width: 0; }
  .provider-name { color: var(--text); font-weight: 600; overflow-wrap: anywhere; }
  .quota-value { overflow-wrap: anywhere; }
  .quota-value.exhausted, .quota-value.failed, .quota-window.exhausted { color: var(--status-failed); }
  .quota-windows { display: flex; flex-direction: column; gap: 3px; }
  .quota-window { display: flex; justify-content: space-between; gap: 8px; }
  button { display: grid; place-items: center; width: 22px; height: 22px; padding: 0; flex: none; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; }
  button:hover { background: var(--surface-hover); color: var(--text); }
  button:disabled { opacity: .45; cursor: default; }
</style>
