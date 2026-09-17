<script lang="ts">
  import type { invoke } from "@tauri-apps/api/core";
  import PiSettings from "../../src/lib/PiSettings.svelte";
  import PiMcpSkillsSettings from "../../src/lib/PiMcpSkillsSettings.svelte";
  import PiProviderSettings from "../../src/lib/PiProviderSettings.svelte";
  import PiMarketplace from "../../src/lib/PiMarketplace.svelte";
  import { DEFAULT_APP_SETTINGS, cssAppFontFamily, cssSessionFontFamily, type AppSettings } from "../../src/lib/settings";
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
    if (name === "list_mcp_servers" || name === "list_skills" || name === "list_installed_workflows") return [] as T;
    if (name === "prefetch_agentic_details") return 0 as T;
    if (name === "translate_agentic_texts") return ((args as { request: { texts: string[] } }).request.texts) as T;
    if (name === "search_agentic_mcp") {
      return [
        { slug: "notion", name: "Notion", description: "Pages, databases, search, and comments via Notion's hosted MCP.", longDescription: "The official Notion MCP server provides full access to Notion's workspace through a hosted MCP endpoint.", author: "Notion", category: "Productivity & PM", transport: ["Streamable HTTP"], official: true, requiresApiKey: true, popularity: "~36K visitors/wk", websiteUrl: "https://developers.notion.com", configSource: "https://developers.notion.com/docs", tags: ["notes", "databases", "wiki"], featured: true, snippets: [], stars: null, heat: 36000, auditPassed: 1, auditTotal: 2 },
        { slug: "github", name: "GitHub", description: "Full GitHub API — repos, issues, PRs, CI/CD, code search.", longDescription: "The official GitHub MCP server provides comprehensive access to the GitHub platform.", author: "GitHub", category: "Developer Tools", transport: ["stdio"], official: true, requiresApiKey: false, popularity: "Listed official", websiteUrl: "https://github.com", configSource: "https://github.com/github/github-mcp-server", tags: ["git", "repos", "issues"], featured: true, snippets: [{ label: "VS Code - remote (OAuth)", file: ".vscode/mcp.json", code: "{\n  \"servers\": {\n    \"github\": {\n      \"type\": \"http\",\n      \"url\": \"https://api.githubcopilot.com/mcp/\"\n    }\n  }\n}" }], stars: 27000, heat: 27000, auditPassed: 6, auditTotal: 7 },
        { slug: "supabase", name: "Supabase", description: "Database, auth, storage, and edge functions for Supabase projects.", author: "Supabase", category: "Databases & Data", transport: ["stdio"], official: true, requiresApiKey: true, popularity: "~42K visitors/wk", websiteUrl: "https://supabase.com", configSource: "https://supabase.com/docs", tags: ["postgres", "auth"], featured: false, snippets: [], stars: null, heat: 42000, auditPassed: 1, auditTotal: 2 },
        { slug: "figma", name: "Figma", description: "Read design files, variables, and comments from Figma.", author: "Figma", category: "Design & UI/UX", transport: ["Streamable HTTP"], official: true, requiresApiKey: false, popularity: null, websiteUrl: "https://figma.com", configSource: "https://help.figma.com", tags: ["design"], featured: false, snippets: [], stars: null, heat: null, auditPassed: null, auditTotal: null },
        { slug: "home-assistant", name: "Home Assistant", description: "Control lights, switches, and sensors in a Home Assistant setup.", author: "community", category: "Home & IoT", transport: ["stdio"], official: false, requiresApiKey: false, popularity: null, websiteUrl: null, configSource: null, tags: ["iot"], featured: false, snippets: [], stars: null, heat: null, auditPassed: null, auditTotal: null },
      ] as T;
    }
    if (name === "search_agentic_skills") {
      return [
        { slug: "taste-skill", name: "Taste Skill", description: "Anti-slop frontend skill that infers a design direction from the brief.", longDescription: "Anti-slop frontend skill that infers a design direction from the brief and ships interfaces that don't look templated.", author: "Leonxlnx", category: "Design & UI/UX", tags: ["design", "frontend", "redesign"], platforms: ["claude-code", "codex", "cursor", "multi-platform"], installs: "87.0K", quality: "S", license: "MIT", lastUpdated: "2026-05-26", githubUrl: "https://github.com/Leonxlnx/taste-skill", skillMdUrl: "https://raw.githubusercontent.com/Leonxlnx/taste-skill/main/skills/taste-skill/SKILL.md", featured: true, stars: 87002, heat: 87002 },
        { slug: "test-driven-development", name: "Test-Driven Development", description: "Writes failing tests first, then minimal code to pass.", author: "obra", category: "Code Quality & Testing", tags: ["testing", "tdd"], platforms: ["claude-code", "codex"], installs: "37.5K", quality: "A", license: "MIT", lastUpdated: "2026-07-21", githubUrl: "https://github.com/obra/superpowers", skillMdUrl: null, featured: false, stars: 37560, heat: 37560 },
        { slug: "claude-seo", name: "Claude SEO", description: "Technical SEO audits, keyword research, and content scoring.", author: "community", category: "SEO & Growth", tags: ["seo", "marketing"], platforms: ["claude-code", "cursor"], installs: "31.2K", quality: "A", license: "MIT", lastUpdated: "2026-02-12", githubUrl: null, skillMdUrl: null, featured: false, stars: 31180, heat: 31180 },
        { slug: "internal-comms", name: "Internal Comms", description: "Draft internal announcements and status updates.", author: "Anthropic", category: "Content & Marketing", tags: ["writing"], platforms: ["claude-code", "multi-platform"], installs: "12.4K", quality: "B", license: "Apache-2.0", lastUpdated: "2026-01-08", githubUrl: null, skillMdUrl: null, featured: false, stars: 12410, heat: 12410 },
        { slug: "sentry-triage", name: "Sentry Triage", description: "Triage Sentry issues and propose fixes.", author: "community", category: "Analytics & Monitoring", tags: ["monitoring"], platforms: ["codex"], installs: null, quality: null, license: null, lastUpdated: null, githubUrl: null, skillMdUrl: null, featured: false, stars: null, heat: null },
      ] as T;
    }
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
    {#snippet models()}<PiProviderSettings embedded invokeCommand={command} confirm={async () => true} onClose={() => {}} onError={(cause) => { error = String(cause); }} />{/snippet}
    {#snippet extensions()}<PiMarketplace embedded projectPath={null} invokeCommand={command} confirm={async () => true} onClose={() => {}} onError={(cause) => { error = String(cause); }} />{/snippet}
    {#snippet mcp()}<PiMcpSkillsSettings mode="mcp" invokeCommand={command} onError={(cause) => { error = String(cause); }} onBusyChange={() => {}} />{/snippet}
    {#snippet skills()}<PiMcpSkillsSettings mode="skills" invokeCommand={command} onError={(cause) => { error = String(cause); }} onBusyChange={() => {}} />{/snippet}
    {#snippet dsh()}<p>DSH</p>{/snippet}
  </PiSettings>
</main>

<style>
  :global(#app) { height: 100%; display: flex; flex-direction: column; }
  nav { display: flex; flex-wrap: wrap; gap: 12px; min-height: 30px; padding: 6px 12px; color: var(--text-muted); font-size: 12px; }
  main { flex: 1; min-height: 0; min-width: 0; }
</style>
