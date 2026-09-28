<script module lang="ts">
  import type { AppSettings } from "./settings";
  import type { AppUpdateState } from "./app-update";
  import type { RuntimeComponent, RuntimeUpdate } from "./runtime";
  import { updateActionsVisible, updateSuppressed } from "./runtime";
  import type { OperationState } from "./operation";
  export interface RuntimeProgress {
    phase: string;
    percent: number | null;
  }
  export interface RuntimeSettingsProps {
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
    onRollbackRuntime: (update: RuntimeUpdate) => void;
    onSnoozeRuntime: (update: RuntimeUpdate) => void;
    onSkipRuntime: (update: RuntimeUpdate) => void;
    onRestartPi: () => void;
    onRestartDsh: () => void;
    restartBusy: string | null;
    runningPiCount: number;
    dshRunning: boolean;
  }
</script>

<script lang="ts">
  import { Download, RefreshCw, RotateCcw, X } from "@lucide/svelte";
  import { t, tm } from "$lib/i18n.svelte";
  let { settings, runtimes, updates, isCheckingUpdates, busyRuntime, runtimeOperation, runtimeProgress, onCancelRuntime,
    appUpdate, onCheckUpdates, onCheckAppUpdate, onInstallAppUpdate, onUpdateRuntime, onRollbackRuntime,
    onSnoozeRuntime, onSkipRuntime, onRestartPi, onRestartDsh, restartBusy, runningPiCount, dshRunning }: RuntimeSettingsProps = $props();
  const sourceLabels = $derived({
    managed: t("托管"),
    development: t("开发目录"),
    profile: t("配置文件"),
  });
</script>

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
  <div class="setting-control">
    <span class="setting-copy"><strong>DSH</strong><small>{dshRunning ? t("运行中") : t("未运行")}</small></span>
    <div class="runtime-actions">
      {#if restartBusy === "dsh"}<span role="status" class="muted">{t("正在重启…")}</span>{/if}
      <button type="button" class="quiet-button" disabled={restartBusy !== null || busyRuntime !== null} onclick={onRestartDsh}>
        <RotateCcw size={14} />{dshRunning ? t("重启") : t("启动")}
      </button>
    </div>
  </div>
  <p class="muted" role="status">{t("重启 Pi 会逐个重启所有运行中的任务（会话内容保留）；重启 DSH 会停止并重新拉起 DSH 服务与界面。")}</p>
</section>

<section class="settings-group" aria-labelledby="runtime-settings-heading">
  <div class="settings-group-header">
    <h3 id="runtime-settings-heading">{t("组件")}</h3>
    <button type="button" class="quiet-button" disabled={isCheckingUpdates} onclick={onCheckUpdates}><RefreshCw size={14} />{t("检查组件更新")}</button>
  </div>
  {#if runtimes.length === 0}<p class="muted" role="status">{t("尚无组件信息")}</p>
  {:else}
    {#each runtimes as runtime (runtime.id)}
      {@const update = updates.find((candidate) => candidate.id === runtime.id)}
      {@const { skipped, snoozed } = updateSuppressed({ componentId: runtime.id, update, skippedUpdates: settings.skippedUpdates, snoozedUpdates: settings.snoozedUpdates })}
      <div class="setting-control">
        <span class="setting-copy"><strong>{runtime.name}</strong><small>{sourceLabels[runtime.source]} · {runtime.currentVersion ?? t("未安装")}</small></span>
        <div class="runtime-actions">
          <span role="status" class="muted">
            {#if busyRuntime === runtime.id}{runtimeOperation?.cancelling ? t("正在取消并恢复") : t("处理中")}
            {:else if skipped}{t("已跳过 {version}", { version: update?.latestVersion ?? "" })}
            {:else if snoozed}{t("已稍后提醒")}
            {:else if update?.stale}{t("离线缓存 · {version}", { version: update.latestVersion ?? t("无版本") })}
            {:else if update?.error}{t("检查失败")}
            {:else if update?.updateAvailable}{t("可更新 · {version}", { version: update.latestVersion ?? "" })}
            {:else if update?.latestVersion}{t("最新")}{:else}{t("未检查")}{/if}
          </span>
          {#if updateActionsVisible(update, skipped, snoozed)}
            {#if update.updateAvailable}
              <button type="button" class="quiet-button" disabled={busyRuntime !== null} onclick={() => onUpdateRuntime(update)}>{runtime.currentVersion ? t("更新") : t("安装")}</button>
              <button type="button" class="quiet-button" onclick={() => onSnoozeRuntime(update)}>{t("稍后")}</button>
              <button type="button" class="quiet-button" onclick={() => onSkipRuntime(update)}>{t("跳过")}</button>
            {:else if !update.stale && !update.error}
              <button type="button" class="quiet-button" disabled={busyRuntime !== null} onclick={() => onUpdateRuntime(update)}>{t("修复")}</button>
            {/if}
          {/if}
          {#if update?.canRollback && !skipped}
            <button type="button" class="quiet-button" disabled={busyRuntime !== null} onclick={() => onRollbackRuntime(update)}>{t("回滚")}</button>
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
