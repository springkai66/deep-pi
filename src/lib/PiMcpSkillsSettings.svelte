<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { Download, Plus, RefreshCw, Search, Store, Trash2 } from "@lucide/svelte";
  import { onDestroy, onMount } from "svelte";
  import { createOperationRunner, type OperationState } from "./operation";
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

  interface McpRegistryEntry {
    name: string;
    description: string;
    version: string | null;
    repository: string | null;
    remoteUrl: string | null;
    package: string | null;
  }

  interface PiPackage {
    name: string;
    description: string;
    types: string[];
    downloads: number;
    publishedAt: number;
    path: string;
  }

  interface PackageMetadata {
    version: string;
    license: string | null;
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
  const operations = createOperationRunner(invoke, (state: OperationState | null) => { operationState = state; });

  let scope = $state<"global" | "project">("global");
  let tab = $state<"installed" | "market">("installed");
  let servers = $state<McpServerEntry[]>([]);
  let skills = $state<SkillEntry[]>([]);
  let loading = $state(false);
  let busy = $state(false);
  let statusMessage = $state("");
  let operationState = $state<OperationState | null>(null);

  // 市场搜索状态（MCP 与 Skills 各自独立）。
  let mcpQuery = $state("");
  let mcpEntries = $state<McpRegistryEntry[]>([]);
  let mcpLoading = $state(false);
  let mcpSearched = $state(false);
  let busyMcpEntry = $state<string | null>(null);

  let skillQuery = $state("");
  let skillPackages = $state<PiPackage[]>([]);
  let skillLoading = $state(false);
  let skillSearched = $state(false);
  let busySkillPackage = $state<string | null>(null);

  let serverName = $state("");
  let serverConfig = $state("");
  let skillName = $state("");
  let skillDescription = $state("");
  let skillContent = $state("");

  const canUseProjectScope = $derived(projectPath !== null);
  $effect(() => {
    if (!canUseProjectScope && scope === "project") scope = "global";
  });
  const componentBusy = $derived(busy || busyMcpEntry !== null || busySkillPackage !== null);
  $effect(() => onBusyChange(componentBusy));
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

  // ---------- MCP 市场（官方 registry.modelcontextprotocol.io） ----------

  function shortServerName(registryName: string): string {
    const segment = registryName.includes("/") ? registryName.split("/").pop() ?? registryName : registryName;
    const cleaned = segment.replace(/[^a-zA-Z0-9_-]+/g, "-").replace(/^[-_]+|[-_]+$/g, "");
    if (!cleaned) return "mcp-server";
    return cleaned.length > 64 ? cleaned.slice(0, 64) : cleaned;
  }

  function mcpConfigFromEntry(entry: McpRegistryEntry): Record<string, unknown> | null {
    if (entry.remoteUrl) return { url: entry.remoteUrl };
    if (entry.package && !entry.package.includes("://")) {
      return { command: "npx", args: ["-y", entry.package] };
    }
    return null;
  }

  async function searchMcpMarket() {
    mcpLoading = true;
    mcpSearched = true;
    try {
      mcpEntries = await invoke<McpRegistryEntry[]>("search_mcp_registry", { request: { query: mcpQuery.trim() } });
    } catch (error) {
      onError(error);
    } finally {
      mcpLoading = false;
    }
  }

  async function installMcpEntry(entry: McpRegistryEntry) {
    if (busyMcpEntry) return;
    const config = mcpConfigFromEntry(entry);
    if (!config) {
      onError(t("该条目没有可自动写入的传输配置，请手动添加"));
      return;
    }
    const name = shortServerName(entry.name);
    busyMcpEntry = entry.name;
    try {
      if (servers.some((server) => server.name === name)) {
        if (!(await confirm(t("覆盖 MCP 服务"), t("已存在同名 MCP 服务 “{name}”，覆盖其配置吗？", { name }), t("覆盖")))) return;
      } else if (
        !(await confirm(
          t("添加 MCP 服务"),
          t("把 “{name}” 添加到{scope} MCP 配置吗？", { name, scope: scope === "project" ? t("项目") : t("全局") }),
          t("添加"),
        ))
      ) {
        return;
      }
      await invoke("save_mcp_server", { request: { name, config, scope, projectPath } });
      statusMessage = t("MCP 服务 {name} 已添加{detail}；新服务在重启 Pi 任务后生效", {
        name,
        detail: entry.remoteUrl ? t("（远程传输）") : t("（本地命令 {package}）", { package: String(entry.package) }),
      });
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busyMcpEntry = null;
    }
  }

  // ---------- Skills 市场（pi.dev 官方目录，skill 类型） ----------

  async function searchSkillMarket() {
    skillLoading = true;
    skillSearched = true;
    try {
      skillPackages = await invoke<PiPackage[]>("search_pi_packages", {
        request: { query: skillQuery.trim(), types: ["skill"] },
      });
    } catch (error) {
      onError(error);
    } finally {
      skillLoading = false;
    }
  }

  async function installSkillPackage(pkg: PiPackage) {
    if (busySkillPackage) return;
    busySkillPackage = pkg.name;
    try {
      const metadata = await invoke<PackageMetadata>("pi_package_metadata", { request: { name: pkg.name } });
      if (
        !(await confirm(
          t("安装 Skill Package"),
          t("安装 {name}@{version} 吗？安装会运行 Pi 包管理器，期间需要停止所有 Pi 任务。", {
            name: pkg.name,
            version: metadata.version,
          }),
          t("安装"),
        ))
      ) {
        return;
      }
      await operations.run("package_operation", {
        operation: "install",
        spec: `npm:${pkg.name}@${metadata.version}`,
        scope,
        projectPath,
        environment: "managed",
      });
      statusMessage = t("{name} 已安装；技能在重启 Pi 任务后生效", { name: pkg.name });
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busySkillPackage = null;
    }
  }

  function formatDownloads(value: number) {
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
    return String(value);
  }

  function installSummary(entry: McpRegistryEntry): string {
    if (entry.remoteUrl) return t("远程 · {url}", { url: entry.remoteUrl });
    if (entry.package) return t("本地 · {package}", { package: entry.package });
    return t("无可用传输配置");
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
      <button type="button" class:active={tab === "market"} aria-pressed={tab === "market"} onclick={() => { tab = "market"; }}>
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
        <h3>{t("MCP 市场官方目录")}</h3>
        <span class="muted">registry.modelcontextprotocol.io</span>
      </div>
      <form class="market-search" onsubmit={(event) => { event.preventDefault(); void searchMcpMarket(); }}>
        <Search size={14} />
        <input bind:value={mcpQuery} placeholder={t("搜索 MCP 服务（如 context7、fetch）")} aria-label={t("搜索 MCP 市场")} autocomplete="off" />
        <button type="submit" class="primary-action" disabled={mcpLoading || busyMcpEntry !== null} aria-label={t("搜索")}>
          {#if mcpLoading}<span class="spin"><RefreshCw size={13} /></span>{:else}<Search size={13} />{/if}
          {t("搜索")}
        </button>
      </form>
      {#if mcpLoading}
        <p class="muted" role="status">{t("正在加载 MCP 目录…")}</p>
      {:else if mcpSearched && mcpEntries.length === 0}
        <p class="muted" role="status">{t("没有匹配的 MCP 服务")}</p>
      {:else if mcpEntries.length === 0}
        <p class="muted" role="status">{t("输入关键词搜索官方 MCP 注册表，选择后一键写入 MCP 配置。")}</p>
      {:else}
        <ul class="entry-list market-list">
          {#each mcpEntries as entry (entry.name)}
            <li>
              <div class="entry-main">
                <strong>{entry.name}{entry.version ? ` · v${entry.version}` : ""}</strong>
                <small>{entry.description || installSummary(entry)}</small>
              </div>
              <button type="button" class="primary-action compact" disabled={busyMcpEntry !== null} title={installSummary(entry)}
                onclick={() => void installMcpEntry(entry)}>
                {#if busyMcpEntry === entry.name}<span class="spin"><RefreshCw size={12} /></span>{:else}<Download size={12} />{/if}
                {t("添加")}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {:else}
    <section class="settings-group">
      <div class="group-header">
        <h3>{t("Skills 市场")}</h3>
        <span class="muted">{t("pi.dev 官方目录 · skill 类型")}</span>
      </div>
      <form class="market-search" onsubmit={(event) => { event.preventDefault(); void searchSkillMarket(); }}>
        <Search size={14} />
        <input bind:value={skillQuery} placeholder={t("搜索 Skill 包")} aria-label={t("搜索 Skills 市场")} autocomplete="off" />
        <button type="submit" class="primary-action" disabled={skillLoading || busySkillPackage !== null} aria-label={t("搜索")}>
          {#if skillLoading}<span class="spin"><RefreshCw size={13} /></span>{:else}<Search size={13} />{/if}
          {t("搜索")}
        </button>
      </form>
      {#if skillLoading}
        <p class="muted" role="status">{t("正在加载 Skills 目录…")}</p>
      {:else if skillSearched && skillPackages.length === 0}
        <p class="muted" role="status">{t("没有匹配的 Skill 包")}</p>
      {:else if skillPackages.length === 0}
        <p class="muted" role="status">{t("输入关键词搜索 pi.dev 官方目录中的 Skill 包；安装会运行 Pi 包管理器。")}</p>
      {:else}
        <ul class="entry-list market-list">
          {#each skillPackages as pkg (pkg.name)}
            <li>
              <div class="entry-main">
                <strong>{pkg.name}</strong>
                <small>{pkg.description || pkg.types.join(" · ")}</small>
              </div>
              <span class="downloads">{t("{count}/月", { count: formatDownloads(pkg.downloads) })}</span>
              <button type="button" class="primary-action compact" disabled={busySkillPackage !== null} onclick={() => void installSkillPackage(pkg)}>
                {#if busySkillPackage === pkg.name}<span class="spin"><RefreshCw size={12} /></span>{:else}<Store size={12} />{/if}
                {t("安装")}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <p class="muted" role="status">{t("安装由 Pi 包管理器执行，需要先停止所有 Pi 任务；安装失败会自动回滚。")}</p>
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
</style>
