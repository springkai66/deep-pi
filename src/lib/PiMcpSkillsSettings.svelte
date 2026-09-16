<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { Download, Plus, RefreshCw, Search, Store, Trash2 } from "@lucide/svelte";
  import { onDestroy, onMount } from "svelte";
  import { t } from "$lib/i18n.svelte";

  interface McpServerEntry {
    name: string;
    config: Record<string, unknown>;
  }

  interface SkillEntry {
    name: string;
    description: string;
    path: string;
  }

  /** agenticskills.io 技能条目（后端 search_agentic_skills 返回）。 */
  interface AgenticSkillEntry {
    slug: string;
    name: string;
    description?: string | null;
    longDescription?: string | null;
    author?: string | null;
    category?: string | null;
    tags?: string[];
    platforms?: string[];
    installs?: number | null;
    quality?: string | number | null;
    license?: string | null;
    lastUpdated?: string | null;
    githubUrl?: string | null;
    skillMdUrl?: string | null;
    featured?: boolean;
  }

  interface AgenticSkillDetail {
    slug: string;
    name: string;
    description?: string | null;
    longDescription?: string | null;
    author?: string | null;
    authorUrl?: string | null;
    tags?: string[];
    platforms?: string[];
    license?: string | null;
    quality?: string | number | null;
    lastUpdated?: string | null;
    githubUrl?: string | null;
    skillMdUrl?: string | null;
    installCommand?: string | null;
    sourceUrl?: string | null;
  }

  /** agenticskills.io MCP 服务器条目（后端 search_agentic_mcp 返回）。 */
  interface AgenticMcpEntry {
    slug: string;
    name: string;
    description?: string | null;
    longDescription?: string | null;
    author?: string | null;
    category?: string | null;
    transport?: string[];
    official?: boolean;
    requiresApiKey?: boolean;
    popularity?: number | null;
    websiteUrl?: string | null;
    configSource?: string | null;
    tags?: string[];
    featured?: boolean;
    snippets?: AgenticMcpSnippet[];
  }

  interface AgenticMcpSnippet {
    label: string;
    file?: string | null;
    code: string;
  }

  interface AgenticMcpDetail {
    slug: string;
    name: string;
    description?: string | null;
    longDescription?: string | null;
    author?: string | null;
    authorUrl?: string | null;
    category?: string | null;
    official?: boolean;
    trustLevel?: string | null;
    transport?: string[];
    requiresApiKey?: boolean;
    websiteUrl?: string | null;
    configSource?: string | null;
    popularity?: number | null;
    tags?: string[];
    snippets?: AgenticMcpSnippet[];
    sourceUrl?: string | null;
  }

  interface Props {
    mode: "mcp" | "skills";
    confirm: (title: string, message: string, confirmLabel?: string) => Promise<boolean>;
    onError: (error: unknown) => void;
    projectPath?: string | null;
    onBusyChange?: (busy: boolean) => void;
    invokeCommand?: typeof nativeInvoke;
  }

  let { mode, confirm, onError, projectPath = null, onBusyChange = () => {}, invokeCommand = nativeInvoke }: Props = $props();
  const invoke = <T,>(command: string, args?: Parameters<typeof nativeInvoke>[1]) => invokeCommand<T>(command, args);

  let scope = $state<"global" | "project">("global");
  let tab = $state<"installed" | "market">("installed");
  let servers = $state<McpServerEntry[]>([]);
  let skills = $state<SkillEntry[]>([]);
  let loading = $state(false);
  let busy = $state(false);
  let statusMessage = $state("");

  // 市场状态（MCP 与 Skills 各自独立）。
  // 目录来自 agenticskills.io：进入市场 tab 时拉一次全量，输入框只在本地过滤。
  let mcpQuery = $state("");
  let mcpEntries = $state<AgenticMcpEntry[]>([]);
  let mcpLoading = $state(false);
  let mcpSearched = $state(false);
  let busyMcpEntry = $state<string | null>(null);
  let mcpDetailSlug = $state<string | null>(null);
  let mcpDetail = $state<AgenticMcpDetail | null>(null);
  let mcpDetailLoading = $state<string | null>(null);
  /** 分类筛选：null = 全部分类。 */
  let mcpCategory = $state<string | null>(null);

  let skillQuery = $state("");
  let skillEntries = $state<AgenticSkillEntry[]>([]);
  let skillLoading = $state(false);
  let skillSearched = $state(false);
  let busySkillEntry = $state<string | null>(null);
  let skillDetailSlug = $state<string | null>(null);
  let skillDetail = $state<AgenticSkillDetail | null>(null);
  let skillDetailLoading = $state<string | null>(null);
  /** 分类筛选：null = 全部分类。 */
  let skillCategory = $state<string | null>(null);

  let serverName = $state("");
  let serverConfig = $state("");
  let skillName = $state("");
  let skillDescription = $state("");
  let skillContent = $state("");

  const canUseProjectScope = $derived(projectPath !== null);
  $effect(() => {
    if (!canUseProjectScope && scope === "project") scope = "global";
  });
  const componentBusy = $derived(
    busy ||
      busyMcpEntry !== null ||
      busySkillEntry !== null ||
      mcpDetailLoading !== null ||
      skillDetailLoading !== null,
  );
  $effect(() => onBusyChange(componentBusy));

  /** 已加载的全量目录按关键词 + 分类本地过滤，输入时不打后端。 */
  const filteredMcpEntries = $derived(
    mcpEntries.filter(
      (entry) =>
        (mcpCategory === null || entry.category === mcpCategory) &&
        matchesKeyword(mcpQuery, entry.name, entry.slug, entry.description, entry.author, entry.category),
    ),
  );
  const filteredSkillEntries = $derived(
    skillEntries.filter(
      (entry) =>
        (skillCategory === null || entry.category === skillCategory) &&
        matchesKeyword(skillQuery, entry.name, entry.slug, entry.description, entry.author, entry.category),
    ),
  );

  /** 分类筛选选项：当前已加载条目里出现过的分类，按条目数从多到少、同数量按名称排序。 */
  const mcpCategoryOptions = $derived(categoryOptions(mcpEntries));
  const skillCategoryOptions = $derived(categoryOptions(skillEntries));

  // 详情面板的派生值：展开哪一条就渲染哪一条；条目自带字段优先，缺字段才回落到详情接口。
  const skillDetailEntry = $derived(skillEntries.find((entry) => entry.slug === skillDetailSlug) ?? null);
  const skillDetailView = $derived(mergeSkillDetail(skillDetailEntry, skillDetail));
  const skillDetailLongDescription = $derived(skillDetailView?.longDescription || skillDetailView?.description || "");
  const skillDetailTags = $derived(skillDetailView?.tags ?? []);
  const skillDetailPlatforms = $derived(skillDetailView?.platforms ?? []);
  const skillDetailLicense = $derived(skillDetailView?.license ?? "");
  const skillDetailUpdated = $derived(skillDetailView?.lastUpdated ?? "");
  const skillDetailSiteUrl = $derived(skillSiteUrl(skillDetailView));
  const skillDetailSkillMdUrl = $derived(skillDetailView?.skillMdUrl ?? "");

  const mcpDetailEntry = $derived(mcpEntries.find((entry) => entry.slug === mcpDetailSlug) ?? null);
  const mcpDetailView = $derived(mergeMcpDetail(mcpDetailEntry, mcpDetail));
  const mcpDetailLongDescription = $derived(mcpDetailView?.longDescription || mcpDetailView?.description || "");
  const mcpDetailAuthor = $derived(mcpDetailView?.author ?? "");
  const mcpDetailTrustLevel = $derived(mcpDetailView?.trustLevel ?? "");
  const mcpDetailConfigSource = $derived(mcpDetailView?.configSource ?? "");
  const mcpDetailTags = $derived(mcpDetailView?.tags ?? []);
  const mcpDetailSnippets = $derived(mcpDetailView?.snippets ?? []);
  const mcpDetailSiteUrl = $derived(mcpSiteUrl(mcpDetailView));
  const mcpDetailWebsiteUrl = $derived(mcpDetailView?.websiteUrl ?? "");

  onDestroy(() => onBusyChange(false));

  async function refresh() {
    loading = true;
    try {
      if (mode === "mcp") {
        servers = await invoke<McpServerEntry[]>("list_mcp_servers", { scope, projectPath });
      } else {
        skills = await invoke<SkillEntry[]>("list_skills", { scope, projectPath });
      }
    } catch (error) {
      onError(error);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
  });

  function serverSummary(config: Record<string, unknown>): string {
    if (typeof config.url === "string" && config.url) return String(config.url);
    if (typeof config.command === "string" && config.command) {
      const args = Array.isArray(config.args) ? config.args.map((argument) => String(argument)).join(" ") : "";
      return args ? `${config.command} ${args}` : String(config.command);
    }
    return t("未配置传输方式");
  }

  async function addServer() {
    const name = serverName.trim();
    if (!name) {
      onError(t("MCP 服务名称不能为空"));
      return;
    }
    let config: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(serverConfig);
      if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
        onError(t("MCP 配置必须是 JSON 对象"));
        return;
      }
      config = parsed as Record<string, unknown>;
    } catch {
      onError(t("MCP 配置不是有效的 JSON"));
      return;
    }
    busy = true;
    try {
      await invoke("save_mcp_server", { request: { name, config, scope, projectPath } });
      serverName = "";
      serverConfig = "";
      statusMessage = t("MCP 服务 {name} 已保存到{scope}配置", {
        name,
        scope: scope === "project" ? t("项目") : t("全局"),
      });
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }

  async function removeServer(entry: McpServerEntry) {
    if (!(await confirm(t("删除 MCP 服务"), t("删除 MCP 服务 “{name}” 吗？", { name: entry.name }), t("删除")))) return;
    busy = true;
    try {
      await invoke("delete_mcp_server", { request: { name: entry.name, scope, projectPath } });
      statusMessage = t("MCP 服务 {name} 已删除", { name: entry.name });
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }

  async function addSkill() {
    const name = skillName.trim();
    if (!name) {
      onError(t("Skill 名称不能为空"));
      return;
    }
    if (!skillDescription.trim()) {
      onError(t("Skill 描述不能为空"));
      return;
    }
    if (!skillContent.trim()) {
      onError(t("Skill 内容不能为空"));
      return;
    }
    busy = true;
    try {
      await invoke("save_skill", {
        request: { name, description: skillDescription.trim(), content: skillContent, scope, projectPath },
      });
      skillName = "";
      skillDescription = "";
      skillContent = "";
      statusMessage = t("Skill {name} 已保存到{scope}配置", {
        name,
        scope: scope === "project" ? t("项目") : t("全局"),
      });
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }

  async function removeSkill(entry: SkillEntry) {
    if (!(await confirm(t("删除 Skill"), t("删除 Skill “{name}” 及其目录吗？", { name: entry.name }), t("删除")))) return;
    busy = true;
    try {
      await invoke("delete_skill", { request: { name: entry.name, scope, projectPath } });
      statusMessage = t("Skill {name} 已删除", { name: entry.name });
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }

  // ---------- 市场目录（agenticskills.io；后端负责抓取与解析） ----------

  /** 站点根地址：详情里缺少 sourceUrl/websiteUrl 时用它兜底，不引入额外依赖。 */
  const AGENTIC_SKILLS_SITE = "https://agenticskills.io";

  /** 关键词本地过滤：命中 名称 / slug / 描述 / 作者（大小写不敏感），空关键词不过滤。 */
  function matchesKeyword(keyword: string, ...values: (string | null | undefined)[]): boolean {
    const needle = keyword.trim().toLowerCase();
    if (!needle) return true;
    return values.some((value) => (value ?? "").toLowerCase().includes(needle));
  }

  /** 分类筛选选项：当前已加载条目里出现过的分类，按条目数从多到少排序，同数量按名称排序。 */
  function categoryOptions(entries: { category?: string | null }[]): { category: string; count: number }[] {
    const counts = new Map<string, number>();
    for (const entry of entries) {
      const category = (entry.category ?? "").trim();
      if (!category) continue;
      counts.set(category, (counts.get(category) ?? 0) + 1);
    }
    return [...counts.entries()]
      .map(([category, count]) => ({ category, count }))
      .sort((a, b) => b.count - a.count || a.category.localeCompare(b.category, "en"));
  }

  /** 条目自带详情字段：有任意一项就直接渲染，缺字段时才请求详情接口。 */
  function hasLocalSkillDetail(entry: AgenticSkillEntry): boolean {
    return Boolean(
      entry.longDescription ||
        entry.license ||
        entry.lastUpdated ||
        (entry.platforms?.length ?? 0) > 0 ||
        (entry.tags?.length ?? 0) > 0,
    );
  }

  function hasLocalMcpDetail(entry: AgenticMcpEntry): boolean {
    return Boolean(
      entry.longDescription ||
        entry.configSource ||
        entry.websiteUrl ||
        (entry.snippets?.length ?? 0) > 0,
    );
  }

  /** 列表行最多展示的标签数。 */
  function topTags(tags: string[] | null | undefined, limit = 3): string[] {
    return (tags ?? []).slice(0, limit);
  }

  /** 数字指标用 K/M 缩写（安装量、热度）；字符串（如质量等级）原样显示；缺失返回空串。 */
  function formatAmount(value: number | string | null | undefined): string {
    if (typeof value === "number") return formatDownloads(value);
    if (value === null || value === undefined) return "";
    return String(value);
  }

  function skillSiteUrl(detail: AgenticSkillDetail | null | undefined): string {
    if (!detail) return AGENTIC_SKILLS_SITE;
    return detail.sourceUrl || detail.githubUrl || `${AGENTIC_SKILLS_SITE}/skills/${detail.slug}`;
  }

  function mcpSiteUrl(detail: AgenticMcpDetail | null | undefined): string {
    if (!detail) return AGENTIC_SKILLS_SITE;
    return detail.sourceUrl || detail.websiteUrl || `${AGENTIC_SKILLS_SITE}/mcp/${detail.slug}`;
  }

  /** 详情面板数据：条目自带字段优先，缺失字段才回落到详情接口数据。 */
  function mergeSkillDetail(entry: AgenticSkillEntry | null, detail: AgenticSkillDetail | null): AgenticSkillDetail | null {
    if (!entry && !detail) return null;
    return {
      slug: entry?.slug ?? detail?.slug ?? "",
      name: entry?.name ?? detail?.name ?? "",
      description: entry?.description ?? detail?.description ?? null,
      longDescription: entry?.longDescription ?? detail?.longDescription ?? null,
      author: entry?.author ?? detail?.author ?? null,
      authorUrl: detail?.authorUrl ?? null,
      tags: entry?.tags?.length ? entry.tags : (detail?.tags ?? []),
      platforms: entry?.platforms?.length ? entry.platforms : (detail?.platforms ?? []),
      license: entry?.license ?? detail?.license ?? null,
      quality: entry?.quality ?? detail?.quality ?? null,
      lastUpdated: entry?.lastUpdated ?? detail?.lastUpdated ?? null,
      githubUrl: entry?.githubUrl ?? detail?.githubUrl ?? null,
      skillMdUrl: entry?.skillMdUrl ?? detail?.skillMdUrl ?? null,
      installCommand: detail?.installCommand ?? null,
      sourceUrl: detail?.sourceUrl ?? null,
    };
  }

  function mergeMcpDetail(entry: AgenticMcpEntry | null, detail: AgenticMcpDetail | null): AgenticMcpDetail | null {
    if (!entry && !detail) return null;
    return {
      slug: entry?.slug ?? detail?.slug ?? "",
      name: entry?.name ?? detail?.name ?? "",
      description: entry?.description ?? detail?.description ?? null,
      longDescription: entry?.longDescription ?? detail?.longDescription ?? null,
      author: entry?.author ?? detail?.author ?? null,
      authorUrl: detail?.authorUrl ?? null,
      category: entry?.category ?? detail?.category ?? null,
      official: entry?.official ?? detail?.official ?? false,
      trustLevel: detail?.trustLevel ?? null,
      transport: entry?.transport?.length ? entry.transport : (detail?.transport ?? []),
      requiresApiKey: entry?.requiresApiKey ?? detail?.requiresApiKey ?? false,
      websiteUrl: entry?.websiteUrl ?? detail?.websiteUrl ?? null,
      configSource: entry?.configSource ?? detail?.configSource ?? null,
      popularity: entry?.popularity ?? detail?.popularity ?? null,
      tags: entry?.tags?.length ? entry.tags : (detail?.tags ?? []),
      snippets: entry?.snippets?.length ? entry.snippets : (detail?.snippets ?? []),
      sourceUrl: detail?.sourceUrl ?? null,
    };
  }

  /** 用系统浏览器打开站点链接；opener 插件不可用时回落到 window.open。 */
  async function openSiteUrl(url: string | null | undefined) {
    if (!url) return;
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  }

  /** 首次进入市场 tab 时按需拉一次全量目录（query 为空串）。 */
  function ensureMarketLoaded() {
    if (mode === "mcp") {
      if (!mcpSearched && !mcpLoading) void loadMcpMarket();
      return;
    }
    if (!skillSearched && !skillLoading) void loadSkillMarket();
  }

  async function loadMcpMarket() {
    if (mcpLoading) return;
    mcpLoading = true;
    mcpSearched = true;
    try {
      mcpEntries = await invoke<AgenticMcpEntry[]>("search_agentic_mcp", { request: { query: "" } });
      if (mcpCategory !== null && !mcpEntries.some((entry) => entry.category === mcpCategory)) mcpCategory = null;
      mcpDetailSlug = null;
      mcpDetail = null;
    } catch (error) {
      onError(error);
    } finally {
      mcpLoading = false;
    }
  }

  async function loadSkillMarket() {
    if (skillLoading) return;
    skillLoading = true;
    skillSearched = true;
    try {
      skillEntries = await invoke<AgenticSkillEntry[]>("search_agentic_skills", { request: { query: "" } });
      if (skillCategory !== null && !skillEntries.some((entry) => entry.category === skillCategory)) skillCategory = null;
      skillDetailSlug = null;
      skillDetail = null;
    } catch (error) {
      onError(error);
    } finally {
      skillLoading = false;
    }
  }

  async function toggleMcpDetail(entry: AgenticMcpEntry) {
    if (mcpDetailSlug === entry.slug) {
      mcpDetailSlug = null;
      mcpDetail = null;
      return;
    }
    mcpDetailSlug = entry.slug;
    mcpDetail = null;
    // 条目自带详情字段时直接展开，不打后端；缺字段才请求详情接口兜底。
    if (hasLocalMcpDetail(entry)) return;
    mcpDetailLoading = entry.slug;
    try {
      const detail = await invoke<AgenticMcpDetail>("agentic_mcp_detail", { request: { slug: entry.slug } });
      if (mcpDetailSlug === entry.slug) mcpDetail = detail;
    } catch (error) {
      if (mcpDetailSlug === entry.slug) mcpDetailSlug = null;
      onError(error);
    } finally {
      if (mcpDetailLoading === entry.slug) mcpDetailLoading = null;
    }
  }

  async function toggleSkillDetail(entry: AgenticSkillEntry) {
    if (skillDetailSlug === entry.slug) {
      skillDetailSlug = null;
      skillDetail = null;
      return;
    }
    skillDetailSlug = entry.slug;
    skillDetail = null;
    // 条目自带详情字段时直接展开，不打后端；缺字段才请求详情接口兜底。
    if (hasLocalSkillDetail(entry)) return;
    skillDetailLoading = entry.slug;
    try {
      const detail = await invoke<AgenticSkillDetail>("agentic_skill_detail", { request: { slug: entry.slug } });
      if (skillDetailSlug === entry.slug) skillDetail = detail;
    } catch (error) {
      if (skillDetailSlug === entry.slug) skillDetailSlug = null;
      onError(error);
    } finally {
      if (skillDetailLoading === entry.slug) skillDetailLoading = null;
    }
  }

  async function installAgenticMcp(entry: AgenticMcpEntry) {
    if (busyMcpEntry) return;
    busyMcpEntry = entry.slug;
    try {
      const installed = await invoke<string>("install_agentic_mcp", { request: { slug: entry.slug, scope, projectPath } });
      statusMessage = t("MCP 服务 {name} 已添加{detail}；新服务在重启 Pi 任务后生效", {
        name: installed || entry.name,
        detail: t("（来自 agenticskills.io）"),
      });
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busyMcpEntry = null;
    }
  }

  async function installAgenticSkill(entry: AgenticSkillEntry) {
    if (busySkillEntry) return;
    busySkillEntry = entry.slug;
    try {
      const installed = await invoke<string>("install_agentic_skill", { request: { slug: entry.slug, scope, projectPath } });
      statusMessage = t("已安装技能 {name}", { name: installed || entry.name });
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busySkillEntry = null;
    }
  }

  function formatDownloads(value: number) {
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
    return String(value);
  }
</script>

<section class="mcp-skills-page" aria-label={mode === "mcp" ? t("MCP 服务设置") : t("Skills 技能设置")}>
  <div class="page-controls">
    <div class="scope-switch" role="tablist" aria-label={t("配置范围")}>
      <button type="button" class:active={scope === "global"} aria-pressed={scope === "global"}
        onclick={() => { scope = "global"; void refresh(); }}>{t("全局")}</button>
      <button type="button" class:active={scope === "project"} aria-pressed={scope === "project"}
        disabled={!canUseProjectScope} title={canUseProjectScope ? "" : t("请先选择一个项目")}
        onclick={() => { scope = "project"; void refresh(); }}>{t("项目")}</button>
    </div>
    <div class="scope-switch tab-switch" role="tablist" aria-label={t("视图")}>
      <button type="button" class:active={tab === "installed"} aria-pressed={tab === "installed"}
        onclick={() => { tab = "installed"; void refresh(); }}>{t("已安装")}</button>
      <button type="button" class:active={tab === "market"} aria-pressed={tab === "market"} onclick={() => { tab = "market"; ensureMarketLoaded(); }}>
        {t("市场")}
      </button>
    </div>
  </div>

  {#if tab === "installed"}
    {#if mode === "mcp"}
      <section class="settings-group">
        <div class="group-header">
          <h3>{t("MCP 服务")}</h3>
          <button type="button" class="icon-action" aria-label={t("刷新 MCP 服务列表")} title={t("刷新")} disabled={loading} onclick={() => void refresh()}>
            <RefreshCw size={14} />
          </button>
        </div>
        {#if loading}
          <p class="muted" role="status">{t("正在读取 MCP 服务…")}</p>
        {:else if servers.length === 0}
          <p class="muted" role="status">{t("尚未安装 MCP 服务")}</p>
        {:else}
          <ul class="entry-list">
            {#each servers as entry (entry.name)}
              <li>
                <div class="entry-main">
                  <strong>{entry.name}</strong>
                  <small>{serverSummary(entry.config)}</small>
                </div>
                <button type="button" class="danger-action" aria-label={t("删除 MCP 服务 {name}", { name: entry.name })} title={t("删除")} disabled={busy} onclick={() => void removeServer(entry)}>
                  <Trash2 size={14} />
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        <div class="add-form">
          <label>{t("MCP 服务名称")}<input bind:value={serverName} placeholder="context7" autocomplete="off" /></label>
          <label>{t("配置（JSON）")}<textarea bind:value={serverConfig} rows="5" placeholder={'{ "url": "https://mcp.example.com/mcp" }'} spellcheck="false"></textarea></label>
          <button type="button" class="primary-action" disabled={busy} onclick={() => void addServer()}>
            <Plus size={14} />{t("添加 MCP 服务")}
          </button>
        </div>
      </section>
    {:else}
      <section class="settings-group">
        <div class="group-header">
          <h3>{t("Skills 技能")}</h3>
          <button type="button" class="icon-action" aria-label={t("刷新 Skills 列表")} title={t("刷新")} disabled={loading} onclick={() => void refresh()}>
            <RefreshCw size={14} />
          </button>
        </div>
        {#if loading}
          <p class="muted" role="status">{t("正在读取 Skills…")}</p>
        {:else if skills.length === 0}
          <p class="muted" role="status">{t("尚未安装 Skill")}</p>
        {:else}
          <ul class="entry-list">
            {#each skills as entry (entry.name)}
              <li>
                <div class="entry-main">
                  <strong>{entry.name}</strong>
                  <small>{entry.description || entry.path}</small>
                </div>
                <button type="button" class="danger-action" aria-label={t("删除 Skill {name}", { name: entry.name })} title={t("删除")} disabled={busy} onclick={() => void removeSkill(entry)}>
                  <Trash2 size={14} />
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        <div class="add-form">
          <label>{t("Skill 名称")}<input bind:value={skillName} placeholder="my-skill" autocomplete="off" /></label>
          <label>{t("触发描述")}<input bind:value={skillDescription} placeholder={t("何时使用该技能…")} autocomplete="off" /></label>
          <label>{t("SKILL.md 内容")}<textarea bind:value={skillContent} rows="6" placeholder={`---\nname: my-skill\ndescription: ${t("何时使用该技能…")}\n---\n\n${t("技能指令正文…")}`} spellcheck="false"></textarea></label>
          <button type="button" class="primary-action" disabled={busy} onclick={() => void addSkill()}>
            <Plus size={14} />{t("添加 Skill")}
          </button>
        </div>
      </section>
    {/if}
  {:else if mode === "mcp"}
    <section class="settings-group">
      <div class="group-header">
        <h3>{t("MCP 市场")}</h3>
        <span class="muted">{t("市场来源：agenticskills.io")}</span>
      </div>
      <form class="market-search" onsubmit={(event) => { event.preventDefault(); void loadMcpMarket(); }}>
        <Search size={14} />
        <input bind:value={mcpQuery} placeholder={t("按名称、作者或分类筛选 MCP 服务…")} aria-label={t("筛选 MCP 服务")} autocomplete="off" />
        <button type="submit" class="primary-action compact" disabled={mcpLoading || busyMcpEntry !== null} aria-label={t("刷新市场")} title={t("刷新市场")}>
          {#if mcpLoading}<span class="spin"><RefreshCw size={13} /></span>{:else}<RefreshCw size={13} />{/if}
          {t("刷新市场")}
        </button>
      </form>
      {#if mcpEntries.length > 0}
        <div class="category-filter" role="group" aria-label={t("分类筛选")}>
          <button type="button" class="category-chip" aria-pressed={mcpCategory === null} onclick={() => (mcpCategory = null)}>
            {t("全部分类")}
          </button>
          {#each mcpCategoryOptions as option (option.category)}
            <button type="button" class="category-chip" aria-pressed={mcpCategory === option.category}
              onclick={() => (mcpCategory = mcpCategory === option.category ? null : option.category)}>
              {option.category}
            </button>
          {/each}
        </div>
      {/if}
      {#if mcpLoading && mcpEntries.length === 0}
        <p class="muted" role="status">{t("正在加载 MCP 目录…")}</p>
      {:else if filteredMcpEntries.length === 0}
        <p class="muted" role="status">{t("没有匹配的 MCP 服务")}</p>
      {:else}
        <ul class="entry-list market-list">
          {#each filteredMcpEntries as entry (entry.slug)}
            <li>
              <div class="entry-main">
                <strong>{entry.name}</strong>
                <small>{entry.description || entry.slug}</small>
                <span class="market-meta">
                  {#if entry.author}<span>{t("作者：{author}", { author: entry.author })}</span>{/if}
                  {#if entry.category}<span class="category-chip" title={t("分类：{category}", { category: entry.category })}>{entry.category}</span>{/if}
                  {#if (entry.transport ?? []).length}<span>{t("传输：{transport}", { transport: (entry.transport ?? []).join(" · ") })}</span>{/if}
                  {#if formatAmount(entry.popularity)}<span class="downloads">{t("热度 {count}", { count: formatAmount(entry.popularity) })}</span>{/if}
                  {#if entry.official}<span class="market-flag">{t("官方")}</span>{/if}
                  {#if entry.requiresApiKey}<span class="market-flag">{t("需要 API Key")}</span>{/if}
                </span>
              </div>
              <span class="market-source">agenticskills.io</span>
              <button type="button" class="secondary-action" disabled={mcpDetailLoading !== null} onclick={() => void toggleMcpDetail(entry)}>
                {mcpDetailLoading === entry.slug ? t("加载中…") : mcpDetailSlug === entry.slug ? t("收起") : t("详情")}
              </button>
              <button type="button" class="primary-action compact" disabled={busyMcpEntry !== null} onclick={() => void installAgenticMcp(entry)}>
                {#if busyMcpEntry === entry.slug}<span class="spin"><RefreshCw size={12} /></span>{:else}<Download size={12} />{/if}
                {t("安装")}
              </button>
              {#if mcpDetailSlug === entry.slug}
                <div class="market-detail">
                  {#if mcpDetailLoading === entry.slug}
                    <p class="muted" role="status">{t("正在加载详情…")}</p>
                  {:else if mcpDetailView}
                    {#if mcpDetailLongDescription}<p class="detail-text">{mcpDetailLongDescription}</p>{/if}
                    <div class="detail-row">
                      {#if mcpDetailAuthor}<span>{t("作者：{author}", { author: mcpDetailAuthor })}</span>{/if}
                      {#if mcpDetailTrustLevel}<span>{t("信任等级：{level}", { level: mcpDetailTrustLevel })}</span>{/if}
                      {#if mcpDetailConfigSource}<span>{t("配置来源：{source}", { source: mcpDetailConfigSource })}</span>{/if}
                      {#if mcpDetailTags.length}<span>{topTags(mcpDetailTags, 6).join(" · ")}</span>{/if}
                    </div>
                    {#if mcpDetailSnippets.length}
                      <small>{t("配置片段")}</small>
                      {#each mcpDetailSnippets as snippet, index (`${snippet.label}-${index}`)}
                        <small>{snippet.label}{snippet.file ? ` · ${snippet.file}` : ""}</small>
                        <pre class="detail-pre">{snippet.code}</pre>
                      {/each}
                    {/if}
                    <div class="detail-actions">
                      <button type="button" class="link-action" onclick={() => void openSiteUrl(mcpDetailSiteUrl)}>{t("在站点打开")}</button>
                      {#if mcpDetailWebsiteUrl}
                        <button type="button" class="link-action" onclick={() => void openSiteUrl(mcpDetailWebsiteUrl)}>{t("打开官网")}</button>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
        <p class="muted" role="status">{t("共 {total} 条，匹配 {shown} 条", { total: mcpEntries.length, shown: filteredMcpEntries.length })}</p>
      {/if}
      <p class="muted" role="status">{t("筛选目录后点击「安装」，新服务在重启 Pi 任务后生效。")}</p>
    </section>
  {:else}
    <section class="settings-group">
      <div class="group-header">
        <h3>{t("Skills 市场")}</h3>
        <span class="muted">{t("市场来源：agenticskills.io")}</span>
      </div>
      <form class="market-search" onsubmit={(event) => { event.preventDefault(); void loadSkillMarket(); }}>
        <Search size={14} />
        <input bind:value={skillQuery} placeholder={t("按名称、作者或关键词筛选技能…")} aria-label={t("筛选技能")} autocomplete="off" />
        <button type="submit" class="primary-action compact" disabled={skillLoading || busySkillEntry !== null} aria-label={t("刷新市场")} title={t("刷新市场")}>
          {#if skillLoading}<span class="spin"><RefreshCw size={13} /></span>{:else}<RefreshCw size={13} />{/if}
          {t("刷新市场")}
        </button>
      </form>
      {#if skillEntries.length > 0}
        <div class="category-filter" role="group" aria-label={t("分类筛选")}>
          <button type="button" class="category-chip" aria-pressed={skillCategory === null} onclick={() => (skillCategory = null)}>
            {t("全部分类")}
          </button>
          {#each skillCategoryOptions as option (option.category)}
            <button type="button" class="category-chip" aria-pressed={skillCategory === option.category}
              onclick={() => (skillCategory = skillCategory === option.category ? null : option.category)}>
              {option.category}
            </button>
          {/each}
        </div>
      {/if}
      {#if skillLoading && skillEntries.length === 0}
        <p class="muted" role="status">{t("正在加载 Skills 目录…")}</p>
      {:else if filteredSkillEntries.length === 0}
        <p class="muted" role="status">{t("没有匹配的技能")}</p>
      {:else}
        <ul class="entry-list market-list">
          {#each filteredSkillEntries as entry (entry.slug)}
            {@const tags = topTags(entry.tags)}
            <li>
              <div class="entry-main">
                <strong>{entry.name}</strong>
                <small>{entry.description || entry.slug}</small>
                <span class="market-meta">
                  {#if entry.author}<span>{t("作者：{author}", { author: entry.author })}</span>{/if}
                  {#if entry.category}<span class="category-chip" title={t("分类：{category}", { category: entry.category })}>{entry.category}</span>{/if}
                  {#if formatAmount(entry.installs)}<span class="downloads">{t("安装量 {count}", { count: formatAmount(entry.installs) })}</span>{/if}
                  {#if formatAmount(entry.quality)}<span>{t("质量 {level}", { level: formatAmount(entry.quality) })}</span>{/if}
                  {#each tags as tag (tag)}<span class="tag">{tag}</span>{/each}
                </span>
              </div>
              <span class="market-source">agenticskills.io</span>
              <button type="button" class="secondary-action" disabled={skillDetailLoading !== null} onclick={() => void toggleSkillDetail(entry)}>
                {skillDetailLoading === entry.slug ? t("加载中…") : skillDetailSlug === entry.slug ? t("收起") : t("详情")}
              </button>
              <button type="button" class="primary-action compact" disabled={busySkillEntry !== null} onclick={() => void installAgenticSkill(entry)}>
                {#if busySkillEntry === entry.slug}<span class="spin"><RefreshCw size={12} /></span>{:else}<Store size={12} />{/if}
                {t("安装")}
              </button>
              {#if skillDetailSlug === entry.slug}
                <div class="market-detail">
                  {#if skillDetailLoading === entry.slug}
                    <p class="muted" role="status">{t("正在加载详情…")}</p>
                  {:else if skillDetailView}
                    {#if skillDetailLongDescription}<p class="detail-text">{skillDetailLongDescription}</p>{/if}
                    <div class="detail-row">
                      {#if skillDetailLicense}<span>{t("许可证：{license}", { license: skillDetailLicense })}</span>{/if}
                      {#if skillDetailPlatforms.length}<span>{t("平台：{platforms}", { platforms: skillDetailPlatforms.join(" · ") })}</span>{/if}
                      {#if skillDetailUpdated}<span>{t("最近更新：{date}", { date: skillDetailUpdated })}</span>{/if}
                    </div>
                    {#if topTags(skillDetailTags, 6).length}
                      <span class="market-meta">{#each topTags(skillDetailTags, 6) as tag (tag)}<span class="tag">{tag}</span>{/each}</span>
                    {/if}
                    <div class="detail-actions">
                      <button type="button" class="link-action" onclick={() => void openSiteUrl(skillDetailSiteUrl)}>{t("在站点打开")}</button>
                      {#if skillDetailSkillMdUrl}
                        <button type="button" class="link-action" onclick={() => void openSiteUrl(skillDetailSkillMdUrl)}>{t("打开 SKILL.md")}</button>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
        <p class="muted" role="status">{t("共 {total} 条，匹配 {shown} 条", { total: skillEntries.length, shown: filteredSkillEntries.length })}</p>
      {/if}
      <p class="muted" role="status">{t("技能来自 agenticskills.io，安装后重启 Pi 任务生效。")}</p>
    </section>
  {/if}
  {#if statusMessage}<p class="status" role="status">{statusMessage}</p>{/if}
</section>

<style>
  .mcp-skills-page { display: grid; gap: 16px; }
  .page-controls { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; }
  .scope-switch { display: inline-flex; gap: 2px; justify-self: start; padding: 2px; border: 1px solid var(--border-strong); border-radius: 5px; background: var(--page-bg); }
  .scope-switch button { min-width: 64px; padding: 4px 10px; border: 0; border-radius: 3px; color: var(--text-muted); background: transparent; font-size: 12px; cursor: pointer; }
  .scope-switch button.active { color: var(--accent-ink); background: var(--accent); font-weight: 700; }
  .scope-switch button:disabled { opacity: .4; cursor: default; }
  .group-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  h3 { margin: 0; font-size: 13px; font-weight: 650; color: var(--text-strong); }
  .icon-action, .danger-action { display: grid; place-items: center; width: 28px; height: 28px; border: 1px solid transparent; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; }
  .icon-action:hover:not(:disabled) { border-color: var(--border-strong); color: var(--text); background: var(--surface-hover); }
  .danger-action { color: #d88989; }
  .danger-action:hover:not(:disabled) { border-color: #74423e; color: #ffd2ce; background: #3b201e; }
  .entry-list { list-style: none; margin: 8px 0 0; padding: 0; display: grid; gap: 6px; }
  .entry-list li { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); }
  .entry-main { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .entry-main strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--text-strong); }
  .entry-main small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; color: var(--text-muted); }
  .downloads { flex-shrink: 0; color: var(--text-muted); font-size: 10px; }
  .add-form { display: grid; gap: 8px; margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border); }
  .add-form label { display: grid; gap: 4px; font-size: 11px; color: var(--text-muted); }
  .add-form input, .add-form textarea { min-width: 0; padding: 6px 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; }
  .add-form textarea { font-family: var(--code-font); font-size: 12px; resize: vertical; }
  .primary-action { display: inline-flex; align-items: center; justify-content: center; gap: 5px; justify-self: start; min-height: 30px; padding: 0 12px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface-hover); color: var(--text); cursor: pointer; }
  .primary-action:hover:not(:disabled) { border-color: var(--accent); color: var(--text-strong); }
  .primary-action.compact { min-height: 26px; padding: 0 10px; font-size: 11px; flex-shrink: 0; }
  .market-search { display: flex; align-items: center; gap: 8px; margin-top: 10px; padding: 2px 4px 2px 10px; border: 1px solid var(--border-strong); border-radius: 5px; background: var(--surface); color: var(--text-muted); }
  .market-search input { flex: 1; min-width: 0; height: 32px; border: 0; outline: 0; background: transparent; color: var(--text); font: inherit; font-size: 12px; }
  .market-search .primary-action { justify-self: auto; }
  .status { margin: 0; font-size: 12px; color: var(--accent); }
  .spin { display: inline-grid; animation: mcp-spin 0.8s linear infinite; }
  @keyframes mcp-spin { to { transform: rotate(360deg); } }
  button:disabled { opacity: .45; cursor: default; }
  /* ---------- 市场列表（agenticskills.io） ---------- */
  .market-list li { flex-wrap: wrap; }
  .market-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; font-size: 10px; color: var(--text-muted); }
  .market-flag { flex-shrink: 0; padding: 1px 5px; border: 1px solid var(--border-strong); border-radius: 3px; color: var(--text); font-size: 10px; }
  .category-chip { flex-shrink: 0; display: inline-flex; align-items: center; padding: 1px 5px; border: 1px solid var(--border-strong); border-radius: 3px; color: var(--text-muted); font-family: var(--code-font); font-size: 10px; letter-spacing: .04em; text-transform: uppercase; }
  button.category-chip { background: transparent; cursor: pointer; }
  button.category-chip:hover { border-color: var(--accent); color: var(--text-strong); }
  .category-chip[aria-pressed="true"] { border-color: var(--accent); background: var(--accent); color: var(--accent-ink); font-weight: 700; }
  .category-filter { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin-top: 8px; min-width: 0; max-width: 100%; overflow-x: auto; }
  .market-source { flex-shrink: 0; padding: 1px 5px; border: 1px solid var(--border); border-radius: 3px; color: var(--text-muted); font-size: 10px; }
  .tag { padding: 1px 5px; border-radius: 3px; background: var(--surface-hover); color: var(--text-muted); font-size: 10px; }
  .secondary-action { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; min-height: 26px; padding: 0 10px; border: 1px solid var(--border); border-radius: 4px; background: transparent; color: var(--text-muted); font-size: 11px; cursor: pointer; }
  .secondary-action:hover:not(:disabled) { border-color: var(--border-strong); color: var(--text); background: var(--surface-hover); }
  .market-detail { flex: 1 1 100%; min-width: 0; display: grid; gap: 6px; margin-top: 4px; padding-top: 8px; border-top: 1px solid var(--border); }
  .detail-text { margin: 0; font-size: 11px; line-height: 1.5; color: var(--text-muted); white-space: pre-wrap; }
  .detail-row { display: flex; flex-wrap: wrap; gap: 10px; font-size: 11px; color: var(--text-muted); }
  .detail-actions { display: flex; flex-wrap: wrap; gap: 10px; }
  .link-action { padding: 0; border: 0; background: transparent; color: var(--accent); font: inherit; font-size: 11px; text-decoration: underline; cursor: pointer; }
  .detail-pre { max-height: 220px; margin: 0; padding: 8px 10px; overflow: auto; border: 1px solid var(--border); border-radius: 4px; background: var(--page-bg); color: var(--text); font-family: var(--code-font); font-size: 11px; white-space: pre-wrap; word-break: break-word; }
</style>
