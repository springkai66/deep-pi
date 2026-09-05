<script lang="ts">
  import { ChevronRight, Download, PackageOpen, RefreshCw, Server, Settings2, X } from "@lucide/svelte";
  import {
    CODE_FONT_OPTIONS,
    UI_FONT_OPTIONS,
    type AppSettings,
  } from "$lib/settings";

  import type { AppUpdateState } from "$lib/app-update";
  import type { RuntimeComponent, RuntimeUpdate } from "$lib/runtime";

  interface Props {
    settings: AppSettings;
    runtimes: RuntimeComponent[];
    updates: RuntimeUpdate[];
    isCheckingUpdates: boolean;
    appUpdate: AppUpdateState;
    onChangeSettings: (settings: AppSettings) => void;
    onCheckUpdates: () => void;
    onCheckAppUpdate: () => void;
    onInstallAppUpdate: () => void;
    onUpdateRuntime: (update: RuntimeUpdate) => void;
    onRollbackRuntime: (update: RuntimeUpdate) => void;
    onSnoozeRuntime: (update: RuntimeUpdate) => void;
    onSkipRuntime: (update: RuntimeUpdate) => void;
    onClose: () => void;
    onOpenModels: () => void;
    onOpenMarket: () => void;
  }

  let {
    settings,
    runtimes,
    updates,
    isCheckingUpdates,
    appUpdate,
    onChangeSettings,
    onCheckUpdates,
    onCheckAppUpdate,
    onInstallAppUpdate,
    onUpdateRuntime,
    onRollbackRuntime,
    onSnoozeRuntime,
    onSkipRuntime,
    onClose,
    onOpenModels,
    onOpenMarket,
  }: Props = $props();

  function updateSettings(patch: Partial<AppSettings>) {
    onChangeSettings({ ...settings, ...patch });
  }

  function sourceLabel(source: RuntimeComponent["source"]) {
    if (source === "managed") return "托管";
    if (source === "system") return "本机";
    if (source === "development") return "开发目录";
    return "配置文件";
  }
</script>

<section class="settings-page" aria-label="设置">
  <header class="settings-header">
    <div class="settings-title">
      <button class="icon-button" type="button" aria-label="返回工作区" title="返回" onclick={onClose}>
        <X size={17} />
      </button>
      <Settings2 size={18} />
      <h1>设置</h1>
    </div>
  </header>

  <div class="settings-content">
    <section class="settings-group" aria-labelledby="app-update-heading">
      <div class="settings-group-header">
        <h2 id="app-update-heading">DeepPi</h2>
        <span class="app-update-status">
          {#if appUpdate.status === "checking"}检查中{:else if appUpdate.status === "installing"}安装中{:else if appUpdate.status === "available"}可更新 · {appUpdate.version}{:else if appUpdate.status === "current"}已是最新{:else if appUpdate.status === "error"}不可用{:else}未检查{/if}
        </span>
      </div>
      <div class="app-update-row">
        <span class="setting-copy">
          <strong>应用更新</strong>
          <small>{appUpdate.notes ?? "使用签名的 Release 更新包"}</small>
        </span>
        <span class="runtime-actions">
          {#if appUpdate.status === "available"}
            <button type="button" class="quiet-button" onclick={onInstallAppUpdate}>
              <Download size={13} />安装更新
            </button>
          {:else}
            <button type="button" class="quiet-button" disabled={appUpdate.status === "checking" || appUpdate.status === "installing"} onclick={onCheckAppUpdate}>
              <span class:spin={appUpdate.status === "checking"}><RefreshCw size={13} /></span>检查应用更新
            </button>
          {/if}
        </span>
      </div>
      {#if appUpdate.error}
        <p class="runtime-note runtime-status error">{appUpdate.error}</p>
      {/if}
    </section>

    <section class="settings-group" aria-labelledby="pi-settings-heading">
      <h2 id="pi-settings-heading">Pi</h2>
      <button class="setting-row" type="button" onclick={onOpenModels}>
        <span class="setting-icon"><Server size={17} /></span>
        <span class="setting-copy">
          <strong>Provider 与模型</strong>
          <small>配置服务商、凭据和模型元数据</small>
        </span>
        <ChevronRight size={16} />
      </button>
      <button class="setting-row" type="button" onclick={onOpenMarket}>
        <span class="setting-icon"><PackageOpen size={17} /></span>
        <span class="setting-copy">
          <strong>Pi 扩展安装</strong>
          <small>搜索、安装、更新和卸载 Pi Package</small>
        </span>
        <ChevronRight size={16} />
      </button>
    </section>

    <section class="settings-group" aria-labelledby="appearance-settings-heading">
      <h2 id="appearance-settings-heading">外观与行为</h2>
      <label class="setting-control">
        <span class="setting-copy">
          <strong>颜色模式</strong>
          <small>跟随系统、白色或黑色</small>
        </span>
        <select
          value={settings.colorMode}
          aria-label="颜色模式"
          onchange={(event) => updateSettings({ colorMode: event.currentTarget.value as AppSettings["colorMode"] })}
        >
          <option value="system">跟随系统</option>
          <option value="light">白色</option>
          <option value="dark">黑色</option>
        </select>
      </label>
      <label class="setting-control">
        <span class="setting-copy">
          <strong>应用字体</strong>
          <small>按钮、导航和控件使用的字体</small>
        </span>
        <select
          value={settings.appFont}
          aria-label="应用字体"
          onchange={(event) => updateSettings({ appFont: event.currentTarget.value as AppSettings["appFont"] })}
        >
          {#each UI_FONT_OPTIONS as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
      <label class="setting-control">
        <span class="setting-copy">
          <strong>设置文本字体</strong>
          <small>设置窗口内的普通文本字体</small>
        </span>
        <select
          value={settings.textFont}
          aria-label="设置文本字体"
          onchange={(event) => updateSettings({ textFont: event.currentTarget.value as AppSettings["textFont"] })}
        >
          {#each UI_FONT_OPTIONS as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
      <label class="setting-control">
        <span class="setting-copy">
          <strong>代码字体</strong>
          <small>Pi 终端和代码区域使用的字体</small>
        </span>
        <select
          value={settings.codeFont}
          aria-label="代码字体"
          onchange={(event) => updateSettings({ codeFont: event.currentTarget.value as AppSettings["codeFont"] })}
        >
          {#each CODE_FONT_OPTIONS as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
      <label class="setting-control">
        <span class="setting-copy">
          <strong>关闭窗口</strong>
          <small>首次关闭时选择后记住，也可以在这里修改</small>
        </span>
        <select
          value={settings.closeBehavior}
          aria-label="关闭窗口行为"
          onchange={(event) => updateSettings({ closeBehavior: event.currentTarget.value as AppSettings["closeBehavior"] })}
        >
          <option value="ask">首次询问</option>
          <option value="minimize">最小化</option>
          <option value="exit">退出应用</option>
        </select>
      </label>
    </section>

    <section class="settings-group" aria-labelledby="runtime-settings-heading">
      <div class="settings-group-header">
        <h2 id="runtime-settings-heading">组件与更新</h2>
        <button type="button" class="quiet-button" disabled={isCheckingUpdates} onclick={onCheckUpdates}>
          <span class:spin={isCheckingUpdates}><RefreshCw size={13} /></span>检查更新
        </button>
      </div>
      {#if runtimes.length === 0}
        <p class="settings-empty">正在读取组件版本…</p>
      {:else}
        <div class="runtime-list">
          {#each runtimes as runtime (runtime.id)}
            {@const update = updates.find((candidate) => candidate.id === runtime.id)}
            {@const skipped = Boolean(update?.latestVersion && settings.skippedUpdates[runtime.id] === update.latestVersion)}
            {@const snoozed = Boolean(update && (settings.snoozedUpdates[runtime.id] ?? 0) > Date.now())}
            <div class="runtime-row">
              <span class="setting-copy">
                <strong>{runtime.name}</strong>
                <small>{sourceLabel(runtime.source)} · {runtime.currentVersion ?? "未安装"}</small>
              </span>
              <span class="runtime-actions">
                <span class:error={update?.error} class:update={update?.updateAvailable} class:stale={update?.stale} class="runtime-status">
                  {#if skipped}已跳过 {update?.latestVersion}{:else if snoozed}已稍后提醒{:else if update?.stale}离线缓存 · {update.latestVersion ?? "无版本"}{:else if update?.error}检查失败{:else if update?.updateAvailable}可更新 · {update.latestVersion}{:else if update?.latestVersion}最新{:else}未检查{/if}
                </span>
                {#if update?.latestVersion && update.installable && !skipped && !snoozed}
                  {#if update.updateAvailable}
                    <button type="button" class="runtime-action" onclick={() => onUpdateRuntime(update)}>更新</button>
                    <button type="button" class="runtime-action" onclick={() => onSnoozeRuntime(update)}>稍后</button>
                    <button type="button" class="runtime-action" onclick={() => onSkipRuntime(update)}>跳过</button>
                  {:else if !update.stale && !update.error}
                    <button type="button" class="runtime-action" onclick={() => onUpdateRuntime(update)}>修复</button>
                  {/if}
                {/if}
                {#if update?.canRollback && !skipped}
                  <button type="button" class="runtime-action" onclick={() => onRollbackRuntime(update)}>回滚</button>
                {/if}
              </span>
            </div>
          {/each}
        </div>
      {/if}
      <p class="runtime-note">更新检查只读取官方 npm registry，不会覆盖正在运行的组件。</p>
    </section>
  </div>
</section>

<style>
  .settings-page {
    height: 100%;
    padding: 14px 18px 18px;
    overflow: auto;
    color: var(--text);
    font-family: var(--text-font);
  }

  .settings-header,
  .settings-title,
  .setting-row {
    display: flex;
    align-items: center;
  }

  .settings-title {
    gap: 8px;
  }

  h1,
  h2,
  small,
  strong {
    margin: 0;
  }

  h1 {
    color: #f4f7f5;
    font-size: 16px;
    font-weight: 650;
  }

  .icon-button {
    display: inline-grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 1px solid transparent;
    border-radius: 4px;
    color: #aeb7b0;
    background: transparent;
    cursor: pointer;
  }

  .icon-button:hover {
    border-color: #465048;
    color: #f4f7f5;
    background: #252b27;
  }

  .settings-content {
    width: min(720px, 100%);
    margin-top: 26px;
  }

  .settings-group-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .app-update-status {
    color: #89928b;
    font-size: 10px;
  }

  .app-update-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 52px;
    padding: 8px 4px;
    border-bottom: 1px solid #303832;
  }

  .settings-group-header h2 {
    flex: 1;
  }

  .quiet-button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 30px;
    padding: 0 9px;
    border: 1px solid #39433c;
    border-radius: 4px;
    color: #c5cec7;
    background: #202721;
    cursor: pointer;
    font: inherit;
    font-size: 11px;
  }

  .quiet-button:hover:not(:disabled) {
    border-color: #607467;
    color: #f4f7f5;
  }

  .runtime-list {
    display: grid;
  }

  .runtime-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 52px;
    padding: 8px 4px;
    border-bottom: 1px solid #303832;
  }

  .runtime-actions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
  }

  .runtime-action {
    min-height: 26px;
    padding: 0 7px;
    border: 1px solid #39433c;
    border-radius: 4px;
    color: #c5cec7;
    background: #202721;
    cursor: pointer;
    font: inherit;
    font-size: 10px;
  }

  .runtime-action:hover {
    border-color: #8fd6ad;
    color: #f4f7f5;
  }


  .runtime-status.stale {
    color: #c9a954;
  }

  .runtime-status.error {
    color: #d88989;
  }

  .runtime-note,
  .settings-empty {
    margin: 10px 4px 0;
    color: #778078;
    font-size: 10px;
  }

  .spin { animation: spin .8s linear infinite; }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }


  .settings-group {
    border-top: 1px solid #303832;
  }

  .settings-group + .settings-group {
    margin-top: 24px;
  }

  .setting-control {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    min-height: 60px;
    padding: 10px 4px;
    border-bottom: 1px solid #303832;
    color: #aeb7b0;
  }

  .setting-control select {
    flex: 0 1 190px;
    min-width: 140px;
    height: 30px;
    padding: 0 8px;
    border: 1px solid #39433c;
    border-radius: 4px;
    outline: none;
    color: #e7ece8;
    background: #101411;
    font: inherit;
  }

  .setting-control select:focus {
    border-color: #78bd96;
    box-shadow: 0 0 0 2px #78bd9622;
  }

  h2 {
    padding: 10px 2px 6px;
    color: #89928b;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .04em;
    text-transform: uppercase;
  }

  .setting-row {
    width: 100%;
    min-height: 66px;
    gap: 12px;
    padding: 10px 4px;
    border: 0;
    border-bottom: 1px solid #303832;
    color: #aeb7b0;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }

  .setting-row:hover {
    color: #f4f7f5;
    background: #191f1b;
  }

  .setting-icon {
    display: inline-grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 1px solid #39433c;
    border-radius: 4px;
    color: #8fd6ad;
    background: #202721;
  }

  .setting-copy {
    display: grid;
    flex: 1;
    min-width: 0;
    gap: 4px;
  }

  .setting-copy strong {
    overflow: hidden;
    color: #f4f7f5;
    font-size: 12px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .setting-copy small {
    overflow: hidden;
    color: #778078;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
