<script lang="ts">
  import type { invoke } from "@tauri-apps/api/core";
  import PiSettings from "../../src/lib/PiSettings.svelte";
  import PiMcpSkillsSettings from "../../src/lib/PiMcpSkillsSettings.svelte";
  import PiProviderSettings from "../../src/lib/PiProviderSettings.svelte";
  import PiMarketplace from "../../src/lib/PiMarketplace.svelte";
  import PiCodemodeSettings from "../../src/lib/PiCodemodeSettings.svelte";
  import AppToasts from "../../src/lib/AppToasts.svelte";
  import { notices } from "../../src/lib/notices.svelte";
  import { DEFAULT_APP_SETTINGS, cssAppFontFamily, cssSessionFontFamily, type AppSettings } from "../../src/lib/settings";
  import type { SettingsCategory } from "../../src/lib/settings-navigation";
  import type { ProviderRecord } from "../../src/lib/provider";
  import { onModelsChanged } from "../../src/lib/model-config-sync";
  import type { PiOfficialModel, PiModelDefaults } from "../../src/lib/pi-auth";

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
  let modelChangeReason = $state("");
  $effect(() => onModelsChanged((reason) => { modelChangeReason = reason; }));
  let action = $state("");
  let listedScope = $state("");
  let provider = $state<ProviderRecord | null>(new URLSearchParams(location.search).has("custom") ? {
    id: "fixture-provider", name: "Fixture Provider", api: "openai-completions", baseUrl: "https://example.invalid/v1",
    headers: {}, proxy: null, models: [{
      id: "fixture-a", name: "Fixture A", reasoning: false, input: ["text"], contextWindow: 128000,
      maxTokens: 8192, thinkingLevels: [], cost: null, api: null,
    }],
  } : null);
  let selectionWrites = $state(0);
  let modelProbes = $state(0);
  let lastSelection = $state("");
  let customSaveFailures = new URLSearchParams(location.search).has("custom-save-fails") ? 1 : 0;
  const fixtureProjectPath = new URLSearchParams(location.search).has("project") ? "F:/fixture-project" : null;
  let installedPackages = $state([{ source: "npm:pi-demo@1.0.0", autoload: true, environment: "managed" as const }]);
  let installedServers = $state<Array<{ name: string; config: Record<string, unknown> }>>([{ name: "local-mcp", config: { url: "https://example.com/mcp" } }]);
  type CodemodeSettings = { enabled: boolean; mode: "on" | "only"; inlineBudget: number };
  let globalCodemode = $state<CodemodeSettings>(new URLSearchParams(location.search).has("codemode-enabled")
    ? { enabled: true, mode: "only", inlineBudget: 4200 } : { enabled: false, mode: "on", inlineBudget: 3000 });
  let projectCodemode = $state<CodemodeSettings | null>(null);
  let codemodeReadScope = $state("");
  let codemodeSaveScope = $state("");
  let codemodeSaves = $state(0);
  let codemodeBusy = $state(false);
  let codemodeReadFailures = new URLSearchParams(location.search).has("codemode-read-fails") ? 1 : 0;
  let codemodeSaveFailures = new URLSearchParams(location.search).has("codemode-save-fails") ? 1 : 0;
  let catalogCalls = $state<string[]>([]);
  let catalogFailures = new Set<string>();
  let pendingCatalogRequests = $state(0);
  const catalogPackage = (name: string, types: string[] = ["extension"], downloads = 200) => ({
    name, description: `Official catalog package ${name}`, types, downloads, publishedAt: 1789300000000, path: name,
  });
  const popularPackages = [catalogPackage("pi-demo", ["extension"], 1200), catalogPackage("pi-other")];
  let installedSkills = $state([{ name: "local-skill", description: "Project helper", path: "skills/local-skill/SKILL.md" }]);
  const catalogModels: PiOfficialModel[] = [
    { id: "fixture-reasoning", name: "Reasoning model", contextWindow: 128000, maxTokens: 8192, reasoning: true, thinkingLevels: ["off", "low", "high"], input: ["text"], inputCost: null, outputCost: null },
    { id: "fixture-fast", name: "Fast model", contextWindow: 64000, maxTokens: 4096, reasoning: false, thinkingLevels: ["off"], input: ["text"], inputCost: null, outputCost: null },
  ];
  let subscriptionModels = $state<PiOfficialModel[]>(catalogModels.map((model) => ({ ...model })));
  let subscriptionDefaults = $state<PiModelDefaults>({ defaultProvider: null, defaultModel: null, modelThinkingLevels: {} });
  let chosenModelIds = $state<string[] | null>(null);
  let selectionReadFailures = new URLSearchParams(location.search).has("selection-read-fails") ? 1 : 0;
  let selectionSaveFailures = new URLSearchParams(location.search).has("selection-save-fails") ? 1 : 0;
  const command: typeof invoke = async <T,>(name: string, args?: Parameters<typeof invoke>[1]): Promise<T> => {
    if (["list_pi_packages", "list_mcp_servers", "list_skills", "list_installed_workflows"].includes(name)) {
      listedScope = (args as { scope: string }).scope;
    }
    if (name === "list_pi_providers") return (provider ? [provider] : []) as T;
    if (name === "search_pi_models") return [] as T;
    if (name === "list_provider_models") return [
      { providerId: "fixture-provider", id: "fixture-a", name: "Fixture A", contextWindow: 128000, maxTokens: 8192, reasoning: false, input: ["text"] },
      { providerId: "fixture-provider", id: "fixture-b", name: "Fixture B", contextWindow: 64000, maxTokens: 4096, reasoning: true, input: ["text"] },
    ] as T;
    if (name === "save_pi_provider_model_selection") {
      const { models } = (args as { request: { providerId: string; models: ProviderRecord["models"] } }).request;
      selectionWrites++;
      lastSelection = models.map((model) => model.id).join(",");
      await new Promise((resolve) => setTimeout(resolve, 60));
      if (customSaveFailures-- > 0) throw new Error("Fixture member save failed");
      if (!provider) throw new Error("Fixture provider missing");
      provider = { ...provider, models: models.map((model) => provider!.models.find((saved) => saved.id === model.id) ?? model) };
      return provider as T;
    }
    if (name === "test_model_connection" || name === "pi_auth_test_model_connection") {
      modelProbes++;
      return { ok: true, status: name === "pi_auth_test_model_connection" ? null : 200, latencyMs: 1, error: null } as T;
    }
    if (name === "search_pi_packages") {
      const { query, page } = (args as { request: { query: string; page: number } }).request;
      const key = `${query}:${page}`;
      catalogCalls = [...catalogCalls, key];
      if (query === "slow" || query === "slow-fail" || (query === "slow-page" && page === 2)) {
        pendingCatalogRequests++;
        await new Promise((resolve) => setTimeout(resolve, 400));
        pendingCatalogRequests--;
      }
      if (query === "slow-fail") throw new Error("Fixture stale catalog request failed");
      if ((query === "fail-once" || (query === "page-fail" && page === 2)) && !catalogFailures.has(key)) {
        catalogFailures.add(key);
        throw new Error("Fixture official catalog unavailable");
      }
      if (query === "empty") return { packages: [], nextPage: null } as T;
      if (query === "" && page === 1) return { packages: popularPackages, nextPage: 2 } as T;
      if (query === "advisor") return { packages: [catalogPackage("pi-advisor-flow"), catalogPackage("pi-advisor", ["skill"])], nextPage: null } as T;
      if (page === 1) return { packages: [catalogPackage("pi-page-first", ["extension", "skill"], 100), catalogPackage("pi-page-theme", ["theme"], 100)], nextPage: 2 } as T;
      return { packages: [catalogPackage("pi-page-theme", ["theme"], 100), catalogPackage("pi-page-prompt", ["prompt"], 100), catalogPackage("pi-page-untyped", [], 100)], nextPage: null } as T;
    }
    if (name === "list_pi_packages") return installedPackages as T;
    if (name === "pi_package_metadata") return { version: "1.0.0", license: "MIT" } as T;
    if (name === "package_operation") {
      const request = (args as { request: { operation: string; spec: string } }).request;
      if (request.operation === "remove") installedPackages = installedPackages.filter((pkg) => pkg.source !== request.spec);
      if (request.operation === "install") installedPackages = [...installedPackages, { source: request.spec, autoload: true, environment: "managed" }];
      action = request.operation;
      return undefined as T;
    }
    if (name === "list_mcp_servers") {
      if (new URLSearchParams(location.search).has("mcp-fails")) throw new Error("Fixture MCP config damaged");
      return installedServers as T;
    }
    if (name === "pi_mcp_list_servers") {
      const { scope } = (args as { request: { scope: "global" | "project" } }).request;
      return {
        available: true,
        servers: installedServers.map((server) => ({
          name: server.name, scope, source: scope === "global" ? "global mcp.json" : ".pi/mcp.json",
          enabled: true, exposure: "codemode", transport: String(server.config.url ?? server.config.command ?? "stdio"),
          state: "connected", tools: ["fixture_tool"], error: null,
        })),
        errors: [], note: null,
      } as T;
    }
    if (name === "pi_codemode_settings") {
      const { scope } = args as { scope: string };
      codemodeReadScope = scope;
      if (codemodeReadFailures-- > 0) throw new Error("Fixture Codemode read failed");
      return { ...(scope === "project" ? projectCodemode ?? globalCodemode : globalCodemode) } as T;
    }
    if (name === "save_pi_codemode_settings") {
      const { scope, projectPath, enabled, mode, inlineBudget } = (args as { request: CodemodeSettings & { scope: string; projectPath: string | null } }).request;
      await new Promise((resolve) => setTimeout(resolve, 60));
      if (codemodeSaveFailures-- > 0) throw new Error("Fixture Codemode save failed");
      if (scope === "project" && projectPath !== fixtureProjectPath) throw new Error("Fixture Codemode project path missing");
      if (scope === "project") projectCodemode = { enabled, mode, inlineBudget };
      else globalCodemode = { enabled, mode, inlineBudget };
      codemodeSaveScope = scope;
      codemodeSaves++;
      action = name;
      return undefined as T;
    }
    if (name === "list_skills") return installedSkills as T;
    if (name === "list_installed_workflows") return [{ slug: "fullstack-saas", name: "Full-Stack SaaS", category: "Developer Tools", skillCount: 5, mcpCount: 4, installedAt: "2026-09-29" }] as T;
    if (name === "save_mcp_server") {
      const request = (args as { request: { name: string; config: Record<string, unknown> } }).request;
      installedServers = [...installedServers, request];
      action = "save_mcp_server";
      return undefined as T;
    }
    if (name === "delete_mcp_server") {
      const request = (args as { request: { name: string } }).request;
      installedServers = installedServers.filter((server) => server.name !== request.name);
      action = "delete_mcp_server";
      return undefined as T;
    }
    if (name === "pi_mcp_login" || name === "pi_mcp_logout") {
      action = name;
      return undefined as T;
    }
    if (name === "delete_skill") {
      const request = (args as { request: { name: string } }).request;
      installedSkills = installedSkills.filter((skill) => skill.name !== request.name);
      action = "delete_skill";
      return undefined as T;
    }
    if (name === "save_skill") {
      const request = (args as { request: { name: string; description: string } }).request;
      installedSkills = [...installedSkills, { name: request.name, description: request.description, path: `skills/${request.name}/SKILL.md` }];
      action = "save_skill";
      return undefined as T;
    }
    if (name === "install_agentic_mcp" || name === "install_agentic_skill") { action = name; return "installed" as T; }
    if (name === "install_agentic_workflow") {
      action = name;
      return { skills: [], mcp: [], skipped: [], failures: [] } as T;
    }
    if (name === "agentic_workflow_detail") return { slug: "fullstack-saas", name: "Full-Stack SaaS", description: "Ship a SaaS from scratch.", category: "Developer Tools", level: "Intermediate", components: [{ kind: "skill", slug: "design", name: "Design", url: "" }, { kind: "mcp", slug: "database", name: "Database", url: "" }], steps: [{ name: "Plan", text: "Plan the project" }], kickoffPrompt: "Build a SaaS", sourceUrl: "https://agenticskills.io/workflows/fullstack-saas" } as T;
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
    if (name === "pi_auth_status") return { credentials: new URLSearchParams(location.search).has("subscription")
      ? ["openai-codex", "anthropic"].map((provider) => ({ provider, authType: "oauth", expires: Date.now() + 86400000 })) : [], activeLogin: null, activeLoginProvider: null } as T;
    if (name === "pi_auth_provider_models") return ((args as { request: { providerId: string } }).request.providerId === "openai-codex" ? subscriptionModels : []) as T;
    if (name === "pi_auth_model_settings") return subscriptionDefaults as T;
    if (name === "pi_auth_model_selection") {
      if (selectionReadFailures-- > 0) throw new Error("Fixture selection read failed");
      return { modelIds: chosenModelIds } as T;
    }
    if (name === "pi_auth_save_model_selection") {
      if (selectionSaveFailures-- > 0) throw new Error("Fixture selection save failed");
      const request = (args as { request: { modelIds: string[] | null } }).request;
      chosenModelIds = request.modelIds;
      action = "save_model_selection";
      return { modelIds: chosenModelIds } as T;
    }
    if (name === "pi_auth_set_model_defaults") {
      const request = (args as { request: { modelId: string; thinkingLevel: string | null } }).request;
      subscriptionDefaults = { defaultProvider: "openai-codex", defaultModel: request.modelId, modelThinkingLevels: request.thinkingLevel ? { [request.modelId]: request.thinkingLevel } : {} };
      action = "save_model_defaults";
      return subscriptionDefaults as T;
    }
    if (name === "pi_auth_save_model_limits") {
      const request = (args as { request: { modelId: string; contextWindow: number | null; maxTokens: number | null } }).request;
      const original = catalogModels.find((model) => model.id === request.modelId)!;
      subscriptionModels = subscriptionModels.map((model) => model.id === request.modelId
        ? { ...model, contextWindow: request.contextWindow ?? original.contextWindow, maxTokens: request.maxTokens ?? original.maxTokens } : model);
      action = "save_model_limits";
      return subscriptionModels as T;
    }
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
  <output aria-label="模型变更类型">{modelChangeReason}</output>
  <output aria-label="Provider 保存次数">{saves}</output>
  <output aria-label="持久化模型名称">{provider?.models.find((model) => model.id === "fixture-a")?.name ?? ""}</output>
  <output aria-label="成员保存次数">{selectionWrites}</output>
  <output aria-label="最近成员快照">{lastSelection}</output>
  <output aria-label="模型探测次数">{modelProbes}</output>
  <output aria-label="订阅筛选快照">{chosenModelIds === null ? "all" : chosenModelIds.join(",")}</output>
  <output aria-label="操作结果">{action}</output>
  <output aria-label="读取范围">{listedScope}</output>
  <output aria-label="Codemode 读取范围">{codemodeReadScope}</output>
  <output aria-label="Codemode 保存范围">{codemodeSaveScope}</output>
  <output aria-label="Codemode 保存次数">{codemodeSaves}</output>
  <output aria-label="Codemode 忙碌">{String(codemodeBusy)}</output>
  <output aria-label="目录请求">{catalogCalls.join(",")}</output>
  <output aria-label="待完成目录请求">{pendingCatalogRequests}</output>
  {#if error}<span role="alert">{error}</span>{/if}
</nav>
<main class="workspace settings-view">
  <PiSettings {category} {settings} onCategoryChange={(next) => { category = next; }}
    onChangeSettings={(next) => { settings = next; changes++; }}
    onEditorSaved={() => {}} onClose={() => { action = "返回工作区"; }}
    runtimes={[{ id: "pi", name: "Pi", currentVersion: "1.0.0", source: "managed", available: true, installed: true }, { id: "dsh", name: "DSH", currentVersion: null, source: "managed", available: false, installed: false }]}
    updates={[{ id: "pi", name: "Pi", currentVersion: "1.0.0", latestVersion: "1.1.0", updateAvailable: true, installable: true, canRollback: true, stale: false, error: null, note: null }]}
    busyRuntime={null} runtimeOperation={null} runtimeProgress={null} isCheckingUpdates={false}
    onCancelRuntime={() => { action = "取消组件操作"; }}
    onCheckUpdates={() => { action = "检查组件更新"; }}
    onUpdateRuntime={() => { action = "安装组件"; }}
    onUninstallRuntime={() => { action = "卸载组件"; }}
    runningPiCount={1}
    dshRunning={true}
    closeBlocked={codemodeBusy}
  >
    {#snippet models()}<PiProviderSettings embedded invokeCommand={command} confirm={async () => true} onClose={() => {}} onError={reportError}
      codexTransport={settings.codexTransport} onCodexTransportChange={(codexTransport) => { settings = { ...settings, codexTransport }; changes++; }} />{/snippet}
    {#snippet codemode()}<PiCodemodeSettings projectPath={fixtureProjectPath} invokeCommand={command} onError={reportError} onBusyChange={(busy) => { codemodeBusy = busy; }} />{/snippet}
    {#snippet extensions()}<PiMarketplace embedded projectPath={fixtureProjectPath} invokeCommand={command} confirm={async () => true} onClose={() => {}} onError={(cause) => { error = String(cause); }} />{/snippet}
    {#snippet mcp()}<PiMcpSkillsSettings mode="mcp" projectPath={fixtureProjectPath} invokeCommand={command} onError={(cause) => { error = String(cause); }} onBusyChange={() => {}} />{/snippet}
    {#snippet skills()}<PiMcpSkillsSettings mode="skills" projectPath={fixtureProjectPath} invokeCommand={command} onError={(cause) => { error = String(cause); }} onBusyChange={() => {}} />{/snippet}
    {#snippet workflows()}<PiMcpSkillsSettings mode="workflows" projectPath={fixtureProjectPath} invokeCommand={command} onError={(cause) => { error = String(cause); }} onBusyChange={() => {}} />{/snippet}
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
