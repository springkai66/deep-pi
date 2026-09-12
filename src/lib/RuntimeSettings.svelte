<script module lang="ts">
  import type { AppSettings } from "./settings";
  import type { AppUpdateState } from "./app-update";
  import type { RuntimeComponent, RuntimeUpdate } from "./runtime";
  import { updateActionsVisible, updateSuppressed } from "./runtime";
  import type { OperationState } from "./operation";
  export interface RuntimeSettingsProps {
    settings: AppSettings;
    runtimes: RuntimeComponent[];
    updates: RuntimeUpdate[];
    isCheckingUpdates: boolean;
    busyRuntime: string | null;
    runtimeOperation: OperationState | null;
    onCancelRuntime: () => void;
    appUpdate: AppUpdateState;
    onCheckUpdates: () => void;
    onCheckAppUpdate: () => void;
    onInstallAppUpdate: () => void;
    onUpdateRuntime: (update: RuntimeUpdate) => void;
    onRollbackRuntime: (update: RuntimeUpdate) => void;
    onSnoozeRuntime: (update: RuntimeUpdate) => void;
    onSkipRuntime: (update: RuntimeUpdate) => void;
  }
</script>

<script lang="ts">
  import { Download, RefreshCw, X } from "@lucide/svelte";
  let { settings, runtimes, updates, isCheckingUpdates, busyRuntime, runtimeOperation, onCancelRuntime,
    appUpdate, onCheckUpdates, onCheckAppUpdate, onInstallAppUpdate, onUpdateRuntime, onRollbackRuntime,
    onSnoozeRuntime, onSkipRuntime }: RuntimeSettingsProps = $props();
  const sourceLabels = { managed: "托管", development: "开发目录", profile: "配置文件" };
</script>

<section class="settings-group" aria-labelledby="app-update-heading">
  <div class="settings-group-header">
    <h3 id="app-update-heading">DeepPi</h3>
    <span class="muted" role="status">{appUpdate.status === "checking" ? "检查中" : appUpdate.status === "installing" ? "安装中" : appUpdate.status === "available" ? `可更新 · ${appUpdate.version}` : appUpdate.status === "current" ? "已是最新" : appUpdate.status === "error" ? "不可用" : "未检查"}</span>
  </div>
  <div class="setting-control">
    <strong>应用更新</strong>
    <div class="runtime-actions">
      {#if appUpdate.status === "available"}
        <button type="button" class="quiet-button" onclick={onInstallAppUpdate}><Download size={14} />安装更新</button>
      {:else}
        <button type="button" class="quiet-button" disabled={appUpdate.status === "checking" || appUpdate.status === "installing"} onclick={onCheckAppUpdate}>
          <RefreshCw size={14} />检查应用更新
        </button>
      {/if}
    </div>
  </div>
  {#if appUpdate.notes}<p class="release-notes">{appUpdate.notes}</p>{/if}
  {#if appUpdate.error}<p role="alert">{appUpdate.error}</p>{/if}
</section>

<section class="settings-group" aria-labelledby="runtime-settings-heading">
  <div class="settings-group-header">
    <h3 id="runtime-settings-heading">组件</h3>
    <button type="button" class="quiet-button" disabled={isCheckingUpdates} onclick={onCheckUpdates}><RefreshCw size={14} />检查组件更新</button>
  </div>
  {#if runtimes.length === 0}<p class="muted" role="status">尚无组件信息</p>
  {:else}
    {#each runtimes as runtime (runtime.id)}
      {@const update = updates.find((candidate) => candidate.id === runtime.id)}
      {@const { skipped, snoozed } = updateSuppressed(update, runtime.id, settings.skippedUpdates, settings.snoozedUpdates)}
      <div class="setting-control">
        <span class="setting-copy"><strong>{runtime.name}</strong><small>{sourceLabels[runtime.source]} · {runtime.currentVersion ?? "未安装"}</small></span>
        <div class="runtime-actions">
          <span role="status" class="muted">
            {#if busyRuntime === runtime.id}{runtimeOperation?.cancelling ? "正在取消并恢复" : "处理中"}
            {:else if skipped}已跳过 {update?.latestVersion}
            {:else if snoozed}已稍后提醒
            {:else if update?.stale}离线缓存 · {update.latestVersion ?? "无版本"}
            {:else if update?.error}检查失败
            {:else if update?.updateAvailable}可更新 · {update.latestVersion}
            {:else if update?.latestVersion}最新{:else}未检查{/if}
          </span>
          {#if updateActionsVisible(update, skipped, snoozed)}
            {#if update.updateAvailable}
              <button type="button" class="quiet-button" disabled={busyRuntime !== null} onclick={() => onUpdateRuntime(update)}>{runtime.currentVersion ? "更新" : "安装"}</button>
              <button type="button" class="quiet-button" onclick={() => onSnoozeRuntime(update)}>稍后</button>
              <button type="button" class="quiet-button" onclick={() => onSkipRuntime(update)}>跳过</button>
            {:else if !update.stale && !update.error}
              <button type="button" class="quiet-button" disabled={busyRuntime !== null} onclick={() => onUpdateRuntime(update)}>修复</button>
            {/if}
          {/if}
          {#if update?.canRollback && !skipped}
            <button type="button" class="quiet-button" disabled={busyRuntime !== null} onclick={() => onRollbackRuntime(update)}>回滚</button>
          {/if}
          {#if busyRuntime === runtime.id && runtimeOperation}
            <button type="button" class="quiet-button icon-button" aria-label="取消组件操作" title="取消组件操作" disabled={runtimeOperation.cancelling} onclick={onCancelRuntime}><X size={14} /></button>
          {/if}
        </div>
      </div>
      {#if update?.error}<p role="alert">{update.error}</p>{/if}
      {#if update?.note && !update.error}<p class="muted" role="status">{update.note}</p>{/if}
    {/each}
  {/if}
</section>

<style>
  .runtime-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; }
  .release-notes { white-space: pre-wrap; }
</style>
