<script module lang="ts">
  import type { AppSettings } from "./settings";
  import type { AppUpdateState } from "./app-update";
  import type { RuntimeComponent, RuntimeUpdate } from "./runtime";
  import { updateSuppressed } from "./runtime";
  import type { OperationState } from "./operation";
  export interface RuntimeProgress {
    phase: string;
    percent: number | null;
  }
  export type RuntimeSettingsView = "app" | "pi" | "dsh";
  export interface RuntimeSettingsProps {
    view: RuntimeSettingsView;
    settings: AppSettings;
    runtimes: RuntimeComponent[];
    updates: RuntimeUpdate[];
    isCheckingUpdates: boolean;
    busyRuntime: string | null;
    runtimeOperation: OperationState | null;
    runtimeProgress: RuntimeProgress | null;
    onCancelRuntime: () => void;
    appUpdate: AppUpdateState;
    onCheckUpdates: () => void;
    onCheckAppUpdate: () => void;
    onInstallAppUpdate: () => void;
    onUpdateRuntime: (update: RuntimeUpdate) => void;
    onUninstallRuntime: (runtime: RuntimeComponent) => void;
    onRestartPi: () => void;
    restartBusy: string | null;
    runningPiCount: number;
    dshRunning: boolean;
  }
</script>

<script lang="ts">
  import { Download, RefreshCw, RotateCcw, Trash2, X } from "@lucide/svelte";
  import { t, tm } from "$lib/i18n.svelte";
  let { view, settings, runtimes, updates, isCheckingUpdates, busyRuntime, runtimeOperation, runtimeProgress, onCancelRuntime,
    appUpdate, onCheckUpdates, onCheckAppUpdate, onInstallAppUpdate, onUpdateRuntime, onUninstallRuntime,
    onRestartPi, restartBusy, runningPiCount, dshRunning }: RuntimeSettingsProps = $props();
  const visibleRuntimes = $derived(runtimes.filter((runtime) => view === "pi" ? runtime.id === "node" || runtime.id === "pi" : runtime.id === "dsh" || runtime.id === "dshmarket"));
  const sourceLabels = $derived({
    managed: t("托管"),
    development: t("开发目录"),
    profile: t("配置文件"),
  });
</script>

{#if view === "app"}
<section class="settings-group" aria-labelledby="app-update-heading">
  <div class="settings-group-header">
    <h3 id="app-update-heading">DeepPi</h3>
    <span class="muted" role="status">{appUpdate.status === "checking" ? t("检查中") : appUpdate.status === "installing" ? t("安装中") : appUpdate.status === "available" ? t("可更新 · {version}", { version: appUpdate.version ?? "" }) : appUpdate.status === "current" ? t("已是最新") : appUpdate.status === "error" ? t("不可用") : t("未检查")}</span>
  </div>
  <div class="setting-control">
    <strong>{t("应用更新")}</strong>
    <div class="runtime-actions">
      {#if appUpdate.status === "available"}
        <button type="button" class="quiet-button" onclick={onInstallAppUpdate}><Download size={14} />{t("安装更新")}</button>
      {:else}
        <button type="button" class="quiet-button" disabled={appUpdate.status === "checking" || appUpdate.status === "installing"} onclick={onCheckAppUpdate}>
          <RefreshCw size={14} />{t("检查应用更新")}
        </button>
      {/if}
    </div>
  </div>
  {#if appUpdate.notes}<p class="release-notes">{tm(appUpdate.notes)}</p>{/if}
  {#if appUpdate.error}<p role="alert">{tm(appUpdate.error)}</p>{/if}
</section>
{/if}

{#if view === "pi"}
<section class="settings-group" aria-labelledby="service-restart-heading">
  <div class="settings-group-header">
    <h3 id="service-restart-heading">{t("运行服务")}</h3>
  </div>
  <div class="setting-control">
    <span class="setting-copy"><strong>Pi Coding Agent</strong><small>{runningPiCount > 0 ? t("{count} 个任务运行中", { count: runningPiCount }) : t("没有运行中的任务")}</small></span>
    <div class="runtime-actions">
      {#if restartBusy === "pi"}<span role="status" class="muted">{t("正在逐个重启任务…")}</span>{/if}
      <button type="button" class="quiet-button" disabled={restartBusy !== null || busyRuntime !== null || runningPiCount === 0} onclick={onRestartPi}>
        <RotateCcw size={14} />{t("重启全部任务")}
      </button>
    </div>
  </div>
  <p class="muted" role="status">{t("重启 Pi 会逐个重启所有运行中的任务（会话内容保留）。")}</p>
</section>
{/if}

{#if view !== "app"}
<section class="settings-group" aria-labelledby="runtime-settings-heading">
  <div class="settings-group-header">
    <h3 id="runtime-settings-heading">{t("组件")}</h3>
    <button type="button" class="quiet-button" disabled={isCheckingUpdates} onclick={onCheckUpdates}><RefreshCw size={14} />{t("检查组件更新")}</button>
  </div>
  {#if visibleRuntimes.length === 0}<p class="muted" role="status">{t("尚无组件信息")}</p>
  {:else}
    {#each visibleRuntimes as runtime (runtime.id)}
      {@const update = updates.find((candidate) => candidate.id === runtime.id)}
      {@const { skipped, snoozed } = updateSuppressed({ componentId: runtime.id, update, skippedUpdates: settings.skippedUpdates, snoozedUpdates: settings.snoozedUpdates })}
      <div class="setting-control">
        <span class="setting-copy"><strong>{runtime.name}</strong><small>{sourceLabels[runtime.source]} · {runtime.currentVersion ?? (runtime.installed ? t("不可用") : t("未安装"))}</small></span>
        <div class="runtime-actions">
          <span role="status" class="muted">
            {#if busyRuntime === runtime.id}{runtimeOperation?.cancelling ? t("正在取消并恢复") : t("处理中")}
            {:else if skipped}{t("已跳过 {version}", { version: update?.latestVersion ?? "" })}
            {:else if snoozed}{t("已稍后提醒")}
            {:else if update?.stale}{t("离线缓存 · {version}", { version: update.latestVersion ?? t("无版本") })}
            {:else if update?.error}{t("检查失败")}
            {:else if update?.updateAvailable}{t("可更新 · {version}", { version: update.latestVersion ?? "" })}{runtime.id === "dsh" && update.latestVersion?.includes("-") ? ` · ${t("预发布")}` : ""}
            {:else if update?.latestVersion}{t("最新")}{:else}{t("未检查")}{/if}
          </span>
          {#if update?.updateAvailable && update.installable && !update.stale && !update.error}
            <button type="button" class="quiet-button" disabled={busyRuntime !== null} onclick={() => onUpdateRuntime(update)}><Download size={14} />{t("安装")}</button>
          {/if}
          {#if runtime.id !== "deeppi" && runtime.source !== "development" && runtime.installed}
            <button type="button" class="quiet-button" disabled={busyRuntime !== null || (runtime.id === "dsh" || runtime.id === "dshmarket") && dshRunning || runtime.id === "pi" && runningPiCount > 0} onclick={() => onUninstallRuntime(runtime)}>
              <Trash2 size={14} />{t("卸载")}
            </button>
          {/if}
          {#if busyRuntime === runtime.id && runtimeOperation}
            <button type="button" class="quiet-button icon-button" aria-label={t("取消组件操作")} title={t("取消组件操作")} disabled={runtimeOperation.cancelling} onclick={onCancelRuntime}><X size={14} /></button>
          {/if}
        </div>
      </div>
      {#if update?.error}<p role="alert">{tm(update.error)}</p>{/if}
      {#if update?.note && !update.error}<p class="muted" role="status">{tm(update.note)}</p>{/if}
      {#if busyRuntime === runtime.id && runtimeProgress}
        <div class="runtime-progress" role="status" aria-label={t("组件更新进度")}>
          <div class="runtime-progress-bar" aria-hidden="true">
            <div
              class="runtime-progress-fill"
              class:indeterminate={runtimeProgress.percent === null}
              style={runtimeProgress.percent === null ? undefined : `width: ${runtimeProgress.percent}%`}
            ></div>
          </div>
          <span class="runtime-progress-text">{runtimeProgress.phase}{runtimeProgress.percent !== null ? ` ${runtimeProgress.percent}%` : ""}</span>
        </div>
      {/if}
    {/each}
  {/if}
</section>
{/if}

<style>
  .runtime-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; }
  .release-notes { white-space: pre-wrap; }
  .runtime-progress { display: grid; gap: 4px; margin-top: 6px; }
  .runtime-progress-bar { height: 4px; border-radius: 2px; background: var(--surface-hover); overflow: hidden; }
  .runtime-progress-fill { height: 100%; border-radius: 2px; background: var(--accent); transition: width .3s ease; }
  .runtime-progress-fill.indeterminate { width: 40%; animation: runtime-progress-slide 1.2s ease-in-out infinite; }
  @keyframes runtime-progress-slide {
    0% { margin-left: -40%; }
    100% { margin-left: 100%; }
  }
  .runtime-progress-text { font-size: 11px; color: var(--text-muted); }
</style>
