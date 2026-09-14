<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { Plus, RefreshCw, Trash2 } from "@lucide/svelte";
  import { onMount } from "svelte";

  interface McpServerEntry {
    name: string;
    config: Record<string, unknown>;
  }

  interface SkillEntry {
    name: string;
    description: string;
    path: string;
  }

  interface Props {
    mode: "mcp" | "skills";
    confirm: (title: string, message: string, confirmLabel?: string) => Promise<boolean>;
    onError: (error: unknown) => void;
    projectPath?: string | null;
    invokeCommand?: typeof nativeInvoke;
  }

  let { mode, confirm, onError, projectPath = null, invokeCommand = nativeInvoke }: Props = $props();
  const invoke = <T,>(command: string, args?: Parameters<typeof nativeInvoke>[1]) => invokeCommand<T>(command, args);

  let scope = $state<"global" | "project">("global");
  let servers = $state<McpServerEntry[]>([]);
  let skills = $state<SkillEntry[]>([]);
  let loading = $state(false);
  let busy = $state(false);
  let statusMessage = $state("");

  let serverName = $state("");
  let serverConfig = $state("");
  let skillName = $state("");
  let skillDescription = $state("");
  let skillContent = $state("");

  const canUseProjectScope = $derived(projectPath !== null);
  $effect(() => {
    if (!canUseProjectScope && scope === "project") scope = "global";
  });

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
    return "未配置传输方式";
  }

  async function addServer() {
    const name = serverName.trim();
    if (!name) {
      onError("MCP 服务名称不能为空");
      return;
    }
    let config: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(serverConfig);
      if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
        onError("MCP 配置必须是 JSON 对象");
        return;
      }
      config = parsed as Record<string, unknown>;
    } catch {
      onError("MCP 配置不是有效的 JSON");
      return;
    }
    busy = true;
    try {
      await invoke("save_mcp_server", { request: { name, config, scope, projectPath } });
      serverName = "";
      serverConfig = "";
      statusMessage = `MCP 服务 ${name} 已保存到${scope === "project" ? "项目" : "全局"}配置`;
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }

  async function removeServer(entry: McpServerEntry) {
    if (!(await confirm("删除 MCP 服务", `删除 MCP 服务 “${entry.name}” 吗？`, "删除"))) return;
    busy = true;
    try {
      await invoke("delete_mcp_server", { request: { name: entry.name, scope, projectPath } });
      statusMessage = `MCP 服务 ${entry.name} 已删除`;
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
      onError("Skill 名称不能为空");
      return;
    }
    if (!skillDescription.trim()) {
      onError("Skill 描述不能为空");
      return;
    }
    if (!skillContent.trim()) {
      onError("Skill 内容不能为空");
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
      statusMessage = `Skill ${name} 已保存到${scope === "project" ? "项目" : "全局"}配置`;
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }

  async function removeSkill(entry: SkillEntry) {
    if (!(await confirm("删除 Skill", `删除 Skill “${entry.name}” 及其目录吗？`, "删除"))) return;
    busy = true;
    try {
      await invoke("delete_skill", { request: { name: entry.name, scope, projectPath } });
      statusMessage = `Skill ${entry.name} 已删除`;
      await refresh();
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }
</script>

<section class="mcp-skills-page" aria-label={mode === "mcp" ? "MCP 服务设置" : "Skills 技能设置"}>
  <div class="scope-switch" role="tablist" aria-label="配置范围">
    <button type="button" class:active={scope === "global"} aria-pressed={scope === "global"}
      onclick={() => { scope = "global"; void refresh(); }}>全局</button>
    <button type="button" class:active={scope === "project"} aria-pressed={scope === "project"}
      disabled={!canUseProjectScope} title={canUseProjectScope ? "" : "请先选择一个项目"}
      onclick={() => { scope = "project"; void refresh(); }}>项目</button>
  </div>
  {#if mode === "mcp"}
    <section class="settings-group">
      <div class="group-header">
        <h3>MCP 服务</h3>
        <button type="button" class="icon-action" aria-label="刷新 MCP 服务列表" title="刷新" disabled={loading} onclick={() => void refresh()}>
          <RefreshCw size={14} />
        </button>
      </div>
      {#if loading}
        <p class="muted" role="status">正在读取 MCP 服务…</p>
      {:else if servers.length === 0}
        <p class="muted" role="status">尚未安装 MCP 服务</p>
      {:else}
        <ul class="entry-list">
          {#each servers as entry (entry.name)}
            <li>
              <div class="entry-main">
                <strong>{entry.name}</strong>
                <small>{serverSummary(entry.config)}</small>
              </div>
              <button type="button" class="danger-action" aria-label={`删除 MCP 服务 ${entry.name}`} title="删除" disabled={busy} onclick={() => void removeServer(entry)}>
                <Trash2 size={14} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="add-form">
        <label>MCP 服务名称<input bind:value={serverName} placeholder="context7" autocomplete="off" /></label>
        <label>配置（JSON）<textarea bind:value={serverConfig} rows="5" placeholder={'{ "url": "https://mcp.example.com/mcp" }'} spellcheck="false"></textarea></label>
        <button type="button" class="primary-action" disabled={busy} onclick={() => void addServer()}>
          <Plus size={14} />添加 MCP 服务
        </button>
      </div>
    </section>
  {:else}
    <section class="settings-group">
      <div class="group-header">
        <h3>Skills 技能</h3>
        <button type="button" class="icon-action" aria-label="刷新 Skills 列表" title="刷新" disabled={loading} onclick={() => void refresh()}>
          <RefreshCw size={14} />
        </button>
      </div>
      {#if loading}
        <p class="muted" role="status">正在读取 Skills…</p>
      {:else if skills.length === 0}
        <p class="muted" role="status">尚未安装 Skill</p>
      {:else}
        <ul class="entry-list">
          {#each skills as entry (entry.name)}
            <li>
              <div class="entry-main">
                <strong>{entry.name}</strong>
                <small>{entry.description || entry.path}</small>
              </div>
              <button type="button" class="danger-action" aria-label={`删除 Skill ${entry.name}`} title="删除" disabled={busy} onclick={() => void removeSkill(entry)}>
                <Trash2 size={14} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="add-form">
        <label>Skill 名称<input bind:value={skillName} placeholder="my-skill" autocomplete="off" /></label>
        <label>触发描述<input bind:value={skillDescription} placeholder="何时使用该技能…" autocomplete="off" /></label>
        <label>SKILL.md 内容<textarea bind:value={skillContent} rows="6" placeholder="---&#10;name: my-skill&#10;description: 何时使用该技能…&#10;---&#10;&#10;技能指令正文…" spellcheck="false"></textarea></label>
        <button type="button" class="primary-action" disabled={busy} onclick={() => void addSkill()}>
          <Plus size={14} />添加 Skill
        </button>
      </div>
    </section>
  {/if}
  {#if statusMessage}<p class="status" role="status">{statusMessage}</p>{/if}
</section>

<style>
  .mcp-skills-page { display: grid; gap: 16px; }
  .scope-switch { display: inline-flex; gap: 2px; justify-self: start; padding: 2px; border: 1px solid var(--border-strong); border-radius: 5px; background: var(--page-bg); }
  .scope-switch button { min-width: 64px; padding: 4px 10px; border: 0; border-radius: 3px; color: var(--text-muted); background: transparent; font-size: 12px; cursor: pointer; }
  .scope-switch button.active { color: var(--accent-ink); background: var(--accent); font-weight: 700; }
  .scope-switch button:disabled { opacity: .4; cursor: default; }
  .group-header { display: flex; align-items: center; justify-content: space-between; }
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
  .add-form { display: grid; gap: 8px; margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border); }
  .add-form label { display: grid; gap: 4px; font-size: 11px; color: var(--text-muted); }
  .add-form input, .add-form textarea { min-width: 0; padding: 6px 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; }
  .add-form textarea { font-family: var(--code-font); font-size: 12px; resize: vertical; }
  .primary-action { display: inline-flex; align-items: center; justify-content: center; gap: 5px; justify-self: start; min-height: 30px; padding: 0 12px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface-hover); color: var(--text); cursor: pointer; }
  .primary-action:hover:not(:disabled) { border-color: var(--accent); color: var(--text-strong); }
  .status { margin: 0; font-size: 12px; color: var(--accent); }
  button:disabled { opacity: .45; cursor: default; }
</style>
