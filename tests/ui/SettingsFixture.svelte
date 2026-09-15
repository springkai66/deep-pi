<script lang="ts">
  import type { invoke } from "@tauri-apps/api/core";
  import PiSettings from "../../src/lib/PiSettings.svelte";
  import PiProviderSettings from "../../src/lib/PiProviderSettings.svelte";
  import PiMarketplace from "../../src/lib/PiMarketplace.svelte";
  import { DEFAULT_APP_SETTINGS, cssFontFamily, type AppSettings } from "../../src/lib/settings";
  import type { SettingsCategory } from "../../src/lib/settings-navigation";
  import type { ProviderRecord } from "../../src/lib/provider";

  let category = $state<SettingsCategory>("general");
  let settings = $state<AppSettings>({ ...DEFAULT_APP_SETTINGS });
  let error = $state("");
  let saves = $state(0);
  let changes = $state(0);
  let action = $state("");
  let provider = $state<ProviderRecord | null>(null);
  const command: typeof invoke = async <T,>(name: string, args?: Parameters<typeof invoke>[1]): Promise<T> => {
    if (name === "list_pi_providers") return (provider ? [provider] : []) as T;
    if (name === "search_pi_models") return [] as T;
    if (name === "search_pi_packages" || name === "list_pi_packages") return [] as T;
    if (name === "save_pi_provider") {
      saves++;
      provider = (args as { request: ProviderRecord }).request;
      return provider as T;
    }
    if (name === "provider_credential_status") return { apiKeyConfigured: false, nativeAuthConfigured: false, authType: null, configured: false } as T;
    throw new Error(`Fixture: unsupported command ${name}`);
  };
  $effect(() => {
    document.documentElement.dataset.colorScheme = settings.colorMode === "light" ? "light" : "dark";
    document.documentElement.style.setProperty("--app-font", cssFontFamily(settings.appFont));
    document.documentElement.style.setProperty("--text-font", cssFontFamily(settings.textFont));
  });
</script>

<nav aria-label="测试状态">
  <output aria-label="设置修改次数">{changes}</output>
  <output aria-label="Provider 保存次数">{saves}</output>
  <output aria-label="操作结果">{action}</output>
  {#if error}<span role="alert">{error}</span>{/if}
</nav>
<main class="workspace settings-view">
  <PiSettings {category} {settings} onCategoryChange={(next) => { category = next; }}
    onDiagnosticsBusy={() => {}} confirmDiagnosticsClear={async () => false}
    onChangeSettings={(next) => { settings = next; changes++; }}
    onEditorSaved={() => {}} onClose={() => { action = "返回工作区"; }}
    runtimes={[{ id: "pi", name: "Pi", currentVersion: "1.0.0", source: "managed", available: true }, { id: "dsh", name: "DSH", currentVersion: null, source: "managed", available: false }]}
    updates={[{ id: "pi", name: "Pi", currentVersion: "1.0.0", latestVersion: "1.1.0", updateAvailable: true, installable: true, canRollback: true, stale: false, error: null, note: null }]}
    busyRuntime={null} runtimeOperation={null} runtimeProgress={null} isCheckingUpdates={false}
    appUpdate={{ status: "available", version: "2.0.0", notes: "Fixture release notes", error: null }}
    onCancelRuntime={() => { action = "取消组件操作"; }}
    onCheckUpdates={() => { action = "检查组件更新"; }}
    onCheckAppUpdate={() => { action = "检查应用更新"; }}
    onInstallAppUpdate={() => { action = "安装应用更新"; }}
    onUpdateRuntime={() => { action = "更新组件"; }}
    onRollbackRuntime={() => { action = "回滚组件"; }}
    onSnoozeRuntime={() => { action = "稍后"; }}
    onSkipRuntime={() => { action = "跳过"; }}
    onRestartPi={() => { action = "重启 Pi 任务"; }}
    onRestartDsh={() => { action = "重启 DSH"; }}
    restartBusy={null}
    runningPiCount={1}
    dshRunning={true}
  >
    {#snippet models()}<PiProviderSettings embedded invokeCommand={command} confirm={async () => true} onClose={() => {}} onError={(cause) => { error = String(cause); }} />{/snippet}
    {#snippet extensions()}<PiMarketplace embedded projectPath={null} invokeCommand={command} confirm={async () => true} onClose={() => {}} onError={(cause) => { error = String(cause); }} />{/snippet}
    {#snippet mcp()}<p>MCP</p>{/snippet}
    {#snippet skills()}<p>Skills</p>{/snippet}
  </PiSettings>
</main>

<style>
  :global(#app) { height: 100%; display: flex; flex-direction: column; }
  nav { display: flex; flex-wrap: wrap; gap: 12px; min-height: 30px; padding: 6px 12px; color: var(--text-muted); font-size: 12px; }
  main { flex: 1; min-height: 0; min-width: 0; }
</style>
