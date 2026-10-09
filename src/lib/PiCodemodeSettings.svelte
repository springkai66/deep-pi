<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { onDestroy, untrack } from "svelte";
  import { t, tm } from "$lib/i18n.svelte";

  interface PiCodemodeSettings {
    enabled: boolean;
    mode: "on" | "only";
    inlineBudget: number;
  }

  interface Props {
    projectPath: string | null;
    onError: (error: unknown) => void;
    onBusyChange?: (busy: boolean) => void;
    invokeCommand?: typeof nativeInvoke;
  }

  let { projectPath, onError, onBusyChange = () => {}, invokeCommand = nativeInvoke }: Props = $props();
  let scope = $state<"global" | "project">("global");
  let settings = $state<PiCodemodeSettings>({ enabled: false, mode: "on", inlineBudget: 3000 });
  let loading = $state(true);
  let loaded = $state(false);
  let saving = $state(false);
  let notice = $state("");
  let errorMessage = $state("");
  let settingsRequest = 0;

  $effect(() => onBusyChange(saving));
  onDestroy(() => { settingsRequest++; onBusyChange(false); });

  $effect(() => {
    if (!projectPath && scope === "project") scope = "global";
    const requestedScope = scope;
    const requestedProjectPath = projectPath;
    untrack(() => {
      notice = "";
      void refreshSettings(requestedScope, requestedProjectPath);
    });
  });

  function reportError(error: unknown) {
    errorMessage = tm(error instanceof Error ? error.message : String(error));
    onError(error);
  }

  async function refreshSettings(requestedScope: "global" | "project", requestedProjectPath: string | null) {
    const request = ++settingsRequest;
    loading = true;
    loaded = false;
    errorMessage = "";
    try {
      const result = await invokeCommand<PiCodemodeSettings>("pi_codemode_settings", {
        scope: requestedScope,
        projectPath: requestedProjectPath,
      });
      if (request === settingsRequest) {
        settings = result;
        loaded = true;
      }
    } catch (error) {
      if (request === settingsRequest) reportError(error);
    } finally {
      if (request === settingsRequest) loading = false;
    }
  }

  async function saveSettings() {
    if (saving || loading || !loaded) return;
    if (!Number.isInteger(settings.inlineBudget) || settings.inlineBudget < 0 || settings.inlineBudget > 100000) {
      reportError(t("Codemode 工具说明预算必须是 0 到 100000 的整数"));
      return;
    }
    const request = settingsRequest;
    const savedScope = scope;
    const savedProjectPath = projectPath;
    saving = true;
    notice = "";
    errorMessage = "";
    try {
      await invokeCommand("save_pi_codemode_settings", {
        request: { scope: savedScope, projectPath: savedProjectPath, ...settings },
      });
      if (request === settingsRequest) {
        notice = t("Codemode 设置已保存；已运行的 Pi 任务重启后生效");
        await refreshSettings(savedScope, savedProjectPath);
      }
    } catch (error) {
      if (request === settingsRequest) reportError(error);
    } finally {
      saving = false;
    }
  }
</script>

<section class="settings-group codemode-settings" aria-label={t("Codemode 设置")}>
  <div class="settings-group-header">
    <h3>Codemode</h3>
    <div class="scope-switch" role="group" aria-label={t("Codemode 配置范围")}>
      <button type="button" class:active={scope === "global"} aria-pressed={scope === "global"} disabled={saving}
        onclick={() => { scope = "global"; }}>{t("全局")}</button>
      <button type="button" class:active={scope === "project"} aria-pressed={scope === "project"} disabled={saving || !projectPath}
        title={projectPath ? "" : t("请先选择一个项目")}
        onclick={() => { scope = "project"; }}>{t("当前项目")}</button>
    </div>
  </div>
  <p class="muted">{t("Codemode 让模型编写 JavaScript，在沙箱中组合调用已启用的工具；MCP 默认曝光也可能按需自动启用它。")}</p>
  {#if scope === "project"}<p class="muted">{t("项目范围 Codemode 设置仅在受信任的项目中生效。")}</p>{/if}
  {#if loading}
    <p class="muted" role="status">{t("正在读取 Codemode 设置…")}</p>
  {:else if loaded}
    <label class="setting-control setting-check codemode-toggle">
      <input type="checkbox" checked={settings.enabled} disabled={saving} aria-label={t("Pi 会话默认启动时启用 Codemode")}
        onchange={(event) => { settings.enabled = event.currentTarget.checked; }} />
      <strong>{t("Pi 会话默认启动时启用 Codemode")}</strong>
    </label>
    <p class="muted">{t("未配置时默认关闭；MCP 仍可能按需自动启用 Codemode。")}</p>
    <label class="setting-control"><strong>{t("Codemode 模式")}</strong>
      <select bind:value={settings.mode} disabled={saving} aria-label={t("Codemode 模式")}>
        <option value="on">{t("on · 直接工具仍可调用")}</option>
        <option value="only">{t("only · 工具只通过 Codemode 调用")}</option>
      </select>
    </label>
    <label class="setting-control"><strong>{t("工具描述预算（tokens）")}</strong>
      <input type="number" min="0" max="100000" step="1" value={settings.inlineBudget} disabled={saving}
        aria-label={t("工具描述预算（tokens）")}
        oninput={(event) => { settings.inlineBudget = Number(event.currentTarget.value); }} />
    </label>
    <button type="button" class="quiet-button save-codemode" disabled={saving} onclick={() => void saveSettings()}>{t("保存 Codemode 设置")}</button>
  {/if}
  {#if errorMessage}
    <p role="alert">{errorMessage}</p>
    {#if !loaded && !loading}<button type="button" class="quiet-button" onclick={() => void refreshSettings(scope, projectPath)}>{t("重试")}</button>{/if}
  {/if}
  {#if notice}<p class="codemode-notice" role="status">{notice}</p>{/if}
</section>

<style>
  .codemode-settings { min-width: 0; }
  .scope-switch { display: inline-flex; gap: 3px; padding: 3px; border: 1px solid var(--border-strong); border-radius: 8px; background: var(--surface); }
  .scope-switch button { min-height: 32px; padding: 6px 12px; border: 0; border-radius: 5px; color: var(--text-muted); background: transparent; font: inherit; font-size: 12px; cursor: pointer; }
  .scope-switch button.active { color: var(--accent); background: var(--surface-raised); box-shadow: inset 0 -2px var(--accent); font-weight: 700; }
  .scope-switch button:hover:not(:disabled) { color: var(--text); background: var(--surface-hover); }
  .scope-switch button:disabled { opacity: .45; cursor: default; }
  .codemode-toggle input { width: 15px; height: 15px; flex-shrink: 0; accent-color: var(--accent); }
  .save-codemode { margin-top: 12px; }
  .codemode-notice { color: var(--accent); }
</style>
