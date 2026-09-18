<script lang="ts">
  import type { invoke } from "@tauri-apps/api/core";
  import PiSettings from "../../src/lib/PiSettings.svelte";
  import PiMcpSkillsSettings from "../../src/lib/PiMcpSkillsSettings.svelte";
  import PiProviderSettings from "../../src/lib/PiProviderSettings.svelte";
  import PiMarketplace from "../../src/lib/PiMarketplace.svelte";
  import AppToasts from "../../src/lib/AppToasts.svelte";
  import { notices } from "../../src/lib/notices.svelte";
  import { DEFAULT_APP_SETTINGS, cssAppFontFamily, cssSessionFontFamily, type AppSettings } from "../../src/lib/settings";
  import type { SettingsCategory } from "../../src/lib/settings-navigation";
  import type { ProviderRecord } from "../../src/lib/provider";

  /** 与 +page.svelte 的 showError 一致：记录到导航栏并弹一条浮字提示。 */
  function reportError(cause: unknown) {
    error = String(cause);
    notices.push(String(cause), "error");
  }

  let category = $state<SettingsCategory>("general");
  let settings = $state<AppSettings>({ ...DEFAULT_APP_SETTINGS });
  let error = $state("");
  let saves = $state(0);
  let changes = $state(0);
  let action = $state("");
  let provider = $state<ProviderRecord | null>(null);
  const command: typeof invoke = async <T,>(name: string, args?: Parameters<typeof invoke>[1]): Promise<T> => {
    if (name === "list_pi_providers") return (provider ? [provider] : []) as T;
    if (name === "search_pi_models" || name === "search_pi_packages" || name === "list_pi_packages") return [] as T;
    if (name === "list_mcp_servers" || name === "list_skills" || name === "list_installed_workflows") return [] as T;
    if (name === "prefetch_agentic_details") return 0 as T;
    if (name === "translate_agentic_texts") return ((args as { request: { texts: string[] } }).request.texts) as T;
    if (name === "search_agentic_mcp") {
      return [
        { slug: "notion", name: "Notion", description: "Pages, databases, search, and comments via Notion's hosted MCP.", longDescription: "The official Notion MCP server provides full access to Notion's workspace.", author: "Notion", category: "Productivity & PM", transport: ["Streamable HTTP"], official: true, requiresApiKey: true, popularity: "~36K visitors/wk", websiteUrl: null, configSource: null, tags: ["notes", "databases"], featured: true, snippets: [], stars: null, heat: 36000, auditPassed: 1, auditTotal: 2 },
        { slug: "github", name: "GitHub", description: "Full GitHub API — repos, issues, PRs, CI/CD, code search.", longDescription: "The official GitHub MCP server provides comprehensive access to the GitHub platform.", author: "GitHub", category: "Developer Tools", transport: ["stdio"], official: true, requiresApiKey: false, popularity: "Listed official", websiteUrl: null, configSource: null, tags: ["git", "repos"], featured: true, snippets: [], stars: 27000, heat: 27000, auditPassed: 6, auditTotal: 7 },
        { slug: "supabase", name: "Supabase", description: "Database, auth, storage, and edge functions for Supabase projects.", longDescription: null, author: "Supabase", category: "Databases & Data", transport: ["stdio"], official: true, requiresApiKey: true, popularity: "~42K visitors/wk", websiteUrl: null, configSource: null, tags: ["postgres"], featured: false, snippets: [], stars: null, heat: 42000, auditPassed: 1, auditTotal: 2 },
      ] as T;
    }
    if (name === "search_agentic_skills") {
      return [
        { slug: "taste-skill", name: "Taste Skill", description: "Anti-slop frontend skill that infers a design direction.", longDescription: "Anti-slop frontend skill that ships interfaces that don't look templated.", author: "Leonxlnx", category: "Design & UI/UX", tags: ["design", "frontend"], platforms: ["claude-code", "codex"], installs: "87.0K", quality: "S", license: "MIT", lastUpdated: "2026-05-26", githubUrl: null, skillMdUrl: null, featured: true, stars: 87002, heat: 87002 },
        { slug: "test-driven-development", name: "Test-Driven Development", description: "Writes failing tests first, then minimal code to pass.", longDescription: null, author: "obra", category: "Code Quality & Testing", tags: ["testing"], platforms: ["claude-code"], installs: "37.5K", quality: "A", license: "MIT", lastUpdated: "2026-07-21", githubUrl: null, skillMdUrl: null, featured: false, stars: 37560, heat: 37560 },
      ] as T;
    }
    if (name === "search_agentic_workflows") {
      return [
        { slug: "fullstack-saas", name: "Full-Stack SaaS", category: "Developer Tools", level: "Intermediate", description: "Ship a SaaS from scratch.", skillCount: 5, mcpCount: 4 },
        { slug: "seo-content-sprint", name: "The SEO Content Sprint", category: "Content & Marketing", level: "Intermediate", description: "Audit, outline, draft, publish.", skillCount: 5, mcpCount: 4 },
      ] as T;
    }
    if (name === "save_pi_provider") {
      saves++;
      provider = (args as { request: ProviderRecord }).request;
      return provider as T;
    }
    if (name === "provider_credential_status") return { apiKeyConfigured: false, nativeAuthConfigured: false, authType: null, configured: false } as T;
    // 官方（OAuth）通道：模型设置页的「官方供应商」分组与新建弹窗都走这些命令。
    if (name === "pi_auth_providers") {
      return [
        { id: "openai-codex", name: "OpenAI Codex", oauth: true, oauthName: "OpenAI (ChatGPT Plus/Pro)", isSubscription: true },
        { id: "anthropic", name: "Anthropic", oauth: true, oauthName: "Anthropic (Claude Pro/Max)", isSubscription: true },
        { id: "github-copilot", name: "GitHub Copilot", oauth: true, oauthName: "GitHub Copilot", isSubscription: true },
      ] as T;
    }
    if (name === "pi_auth_status") return { credentials: [], activeLogin: null, activeLoginProvider: null } as T;
    if (name === "pi_auth_provider_models") return [] as T;
    throw new Error(`Fixture: unsupported command ${name}`);
  };
  $effect(() => {
    document.documentElement.dataset.colorScheme = settings.colorMode === "light" ? "light" : "dark";
    document.documentElement.style.setProperty("--app-font", cssAppFontFamily(settings.appFontName));
    document.documentElement.style.setProperty("--session-font", cssSessionFontFamily(settings.sessionFontName));
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
    {#snippet models()}<PiProviderSettings embedded invokeCommand={command} confirm={async () => true} onClose={() => {}} onError={reportError} />{/snippet}
    {#snippet extensions()}<PiMarketplace embedded projectPath={null} invokeCommand={command} confirm={async () => true} onClose={() => {}} onError={(cause) => { error = String(cause); }} />{/snippet}
    {#snippet mcp()}<PiMcpSkillsSettings mode="mcp" invokeCommand={command} onError={(cause) => { error = String(cause); }} onBusyChange={() => {}} />{/snippet}
    {#snippet skills()}<PiMcpSkillsSettings mode="skills" invokeCommand={command} onError={(cause) => { error = String(cause); }} onBusyChange={() => {}} />{/snippet}
    {#snippet dsh()}<p>DSH</p>{/snippet}
  </PiSettings>
</main>

<!-- 与正式应用一致：错误以顶部浮字提示展示，便于夹具验证。 -->
<AppToasts />

<style>
  :global(#app) { height: 100%; display: flex; flex-direction: column; }
  nav { display: flex; flex-wrap: wrap; gap: 12px; min-height: 30px; padding: 6px 12px; color: var(--text-muted); font-size: 12px; }
  main { flex: 1; min-height: 0; min-width: 0; }
</style>
