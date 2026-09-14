<script lang="ts">
  import { ArrowLeft, Monitor, Palette, Server, Download, PackageOpen, Puzzle, Sparkles, Wrench } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { CODE_FONT_OPTIONS, UI_FONT_OPTIONS, type AppSettings, type ExternalEditor } from "./settings";
  import { SETTINGS_CATEGORIES, nextSettingsCategory, parseTaskLimit, type SettingsCategory } from "./settings-navigation";
  import RuntimeSettings, { type RuntimeSettingsProps } from "./RuntimeSettings.svelte";
  import ExternalEditorSettings from "./ExternalEditorSettings.svelte";
  import DiagnosticsPanel from "./DiagnosticsPanel.svelte";
  import "./settings-controls.css";

  interface Props extends RuntimeSettingsProps {
    category: SettingsCategory;
    onCategoryChange: (category: SettingsCategory) => void;
    onChangeSettings: (settings: AppSettings) => void;
    onEditorSaved: (editor: ExternalEditor | null) => void;
    onClose: () => void;
    models: Snippet;
    extensions: Snippet;
    mcp: Snippet;
    skills: Snippet;
    closeBlocked?: boolean;
    saving?: boolean;
    onDiagnosticsBusy: (busy: boolean) => void;
    confirmDiagnosticsClear: () => Promise<boolean>;
  }
  let { category, onCategoryChange, onChangeSettings, onEditorSaved, onClose, models, extensions, mcp, skills, closeBlocked = false, saving = false, onDiagnosticsBusy, confirmDiagnosticsClear, ...runtime }: Props = $props();
  const icons = { general: Monitor, appearance: Palette, models: Server, runtime: Download, extensions: PackageOpen, mcp: Puzzle, skills: Sparkles, advanced: Wrench };
  let modelsVisited = $state(false);
  let extensionsVisited = $state(false);
  let mcpVisited = $state(false);
  let skillsVisited = $state(false);
  let advancedVisited = $state(false);
  let taskLimitError = $state("");
  $effect(() => {
    if (category === "models") modelsVisited = true;
    if (category === "extensions") extensionsVisited = true;
    if (category === "mcp") mcpVisited = true;
    if (category === "skills") skillsVisited = true;
    if (category === "advanced") advancedVisited = true;
  });
  const title = $derived(SETTINGS_CATEGORIES.find((item) => item.id === category)?.label ?? "设置");

  function updateSettings(patch: Partial<AppSettings>) { onChangeSettings({ ...runtime.settings, ...patch }); }
  function changeLimit(input: HTMLInputElement) {
    const value = parseTaskLimit(input.value);
    taskLimitError = value === null ? "同时运行任务数必须为 1 到 16 的整数" : "";
    if (value !== null) updateSettings({ maxConcurrentTasks: value });
  }
  function navigate(event: KeyboardEvent, current: SettingsCategory) {
    if (event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return;
    const next = nextSettingsCategory(current, event.key);
    if (!next) return;
    event.preventDefault();
    onCategoryChange(next);
    document.getElementById(`settings-nav-${next}`)?.focus();
  }
</script>

<section class="settings-page" aria-label="设置">
  <header class="settings-header">
    <button class="quiet-button icon-button" type="button" aria-label="返回工作区" title={closeBlocked ? "请先完成或取消当前操作" : "返回工作区"} disabled={closeBlocked} onclick={onClose}><ArrowLeft size={17} /></button>
    <h1>设置</h1>
    {#if saving}<span class="muted" role="status">正在保存…</span>{/if}
  </header>
  <div class="settings-layout">
    <nav class="settings-navigation" aria-label="设置分类">
      {#each SETTINGS_CATEGORIES as item}
        {@const Icon = icons[item.id]}
        <button id={`settings-nav-${item.id}`} type="button" aria-current={category === item.id ? "page" : undefined}
          class:active={category === item.id} onclick={() => onCategoryChange(item.id)} onkeydown={(event) => navigate(event, item.id)}>
          <Icon size={16} /><span>{item.label}</span>
        </button>
      {/each}
    </nav>
    <div class="settings-body">
      <h2>{title}</h2>
      <div class="settings-panel" hidden={category !== "general"}>
        <section class="settings-group" aria-labelledby="general-behavior-heading">
          <h3 id="general-behavior-heading">应用行为</h3>
          <label class="setting-control"><strong>关闭窗口</strong>
            <select value={runtime.settings.closeBehavior} aria-label="关闭窗口行为"
              onchange={(event) => updateSettings({ closeBehavior: event.currentTarget.value as AppSettings["closeBehavior"] })}>
              <option value="ask">首次询问</option><option value="minimize">最小化</option><option value="exit">退出应用</option>
            </select>
          </label>
          <label class="setting-control"><strong>同时运行任务数</strong>
            <input type="number" min="1" max="16" step="1" aria-label="同时运行任务数" aria-invalid={!!taskLimitError}
              aria-describedby={taskLimitError ? "task-limit-error" : undefined}
              value={runtime.settings.maxConcurrentTasks} oninput={(event) => changeLimit(event.currentTarget)} />
          </label>
          {#if taskLimitError}<p id="task-limit-error" role="alert">{taskLimitError}</p>{/if}
        </section>
      </div>
      <div class="settings-panel" hidden={category !== "appearance"}>
        <section class="settings-group" aria-labelledby="appearance-heading">
          <h3 id="appearance-heading">主题与字体</h3>
          <label class="setting-control"><strong>颜色模式</strong>
            <select value={runtime.settings.colorMode} aria-label="颜色模式" onchange={(event) => updateSettings({ colorMode: event.currentTarget.value as AppSettings["colorMode"] })}>
              <option value="system">跟随系统</option><option value="light">浅色</option><option value="dark">深色</option>
            </select>
          </label>
          <label class="setting-control"><strong>主题风格</strong>
            <select value={runtime.settings.theme} aria-label="主题风格" onchange={(event) => updateSettings({ theme: event.currentTarget.value as AppSettings["theme"] })}>
              <option value="win11">Windows 11</option><option value="winxp">Windows XP</option>
            </select>
          </label>
          {#each [{ key: "appFont", label: "应用字体" }, { key: "textFont", label: "设置文本字体" }] as field}
            <label class="setting-control"><strong>{field.label}</strong>
              <select value={runtime.settings[field.key as "appFont" | "textFont"]} aria-label={field.label}
                onchange={(event) => updateSettings({ [field.key]: event.currentTarget.value })}>
                {#each UI_FONT_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}
              </select>
            </label>
          {/each}
          <label class="setting-control"><strong>代码字体</strong>
            <select value={runtime.settings.codeFont} aria-label="代码字体" onchange={(event) => updateSettings({ codeFont: event.currentTarget.value as AppSettings["codeFont"] })}>
              {#each CODE_FONT_OPTIONS as option}<option value={option.value}>{option.label}</option>{/each}
            </select>
          </label>
        </section>
      </div>
      <div class="settings-panel embedded-panel" hidden={category !== "models"}>{#if modelsVisited}{@render models()}{/if}</div>
      <div class="settings-panel" hidden={category !== "runtime"}><RuntimeSettings {...runtime} /></div>
      <div class="settings-panel embedded-panel" hidden={category !== "extensions"}>{#if extensionsVisited}{@render extensions()}{/if}</div>
      <div class="settings-panel embedded-panel" hidden={category !== "mcp"}>{#if mcpVisited}{@render mcp()}{/if}</div>
      <div class="settings-panel embedded-panel" hidden={category !== "skills"}>{#if skillsVisited}{@render skills()}{/if}</div>
      <div class="settings-panel" hidden={category !== "advanced"}>
        <ExternalEditorSettings editor={runtime.settings.externalEditor} onSaved={onEditorSaved} />
        {#if advancedVisited}<DiagnosticsPanel onBusyChange={onDiagnosticsBusy} confirmClear={confirmDiagnosticsClear} />{/if}
        <section class="settings-group" aria-labelledby="diagnostics-heading">
          <h3 id="diagnostics-heading">运行环境</h3>
          {#if runtime.runtimes.length === 0}<p class="muted" role="status">尚未读取运行环境</p>
          {:else}
            <dl class="environment">
              {#each runtime.runtimes as component}
                <div><dt>{component.name}</dt><dd>{component.currentVersion ?? "未安装"} · {component.available ? "可用" : "不可用"} · {component.source}</dd></div>
              {/each}
            </dl>
          {/if}
          {#if runtime.appUpdate.error}<p role="alert">{runtime.appUpdate.error}</p>{/if}
          {#each runtime.updates.filter((update) => update.error) as update}<p role="alert">{update.name}: {update.error}</p>{/each}
        </section>
      </div>
    </div>
  </div>
</section>

<style>
  .settings-page { height: 100%; min-height: 0; display: flex; flex-direction: column; color: var(--text); font-family: var(--text-font); }
  .settings-header { min-height: 52px; flex-shrink: 0; display: flex; align-items: center; gap: 12px; padding: 8px 16px; border-bottom: 1px solid var(--border); }
  h1 { margin: 0; font-size: 16px; font-weight: 650; color: var(--text-strong); }
  h2 { margin: 0; padding: 0 0 16px; font-size: 18px; font-weight: 600; color: var(--text-strong); }
  .settings-layout { display: grid; grid-template-columns: 180px minmax(0, 1fr); flex: 1; min-height: 0; }
  .settings-navigation { padding: 12px 8px; border-right: 1px solid var(--border); background: var(--surface-alt); overflow-y: auto; }
  .settings-navigation button { display: flex; align-items: center; gap: 10px; min-height: 36px; width: 100%; padding: 8px 10px; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); text-align: left; cursor: pointer; }
  .settings-navigation button span { min-width: 0; overflow-wrap: anywhere; }
  .settings-navigation button:hover { background: var(--surface-hover); color: var(--text); }
  .settings-navigation button.active { color: var(--text-strong); background: var(--surface-raised); box-shadow: inset 2px 0 var(--accent); }
  .settings-body { min-width: 0; min-height: 0; overflow: auto; padding: 24px; container-type: inline-size; }
  .settings-panel { max-width: 920px; }
  .embedded-panel { max-width: none; min-height: 500px; height: calc(100% - 40px); }
  .settings-panel[hidden] { display: none; }
  .environment { margin: 0; }
  .environment div { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; padding: 12px 0; border-bottom: 1px solid var(--border); }
  dt { font-weight: 600; }
  dd { margin: 0; color: var(--text-muted); overflow-wrap: anywhere; }
  @media (max-width: 760px) {
    .settings-layout { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }
    .settings-navigation { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 4px; border-right: 0; border-bottom: 1px solid var(--border); }
    .settings-navigation button { font-size: 12px; gap: 6px; padding: 8px; }
    .settings-body { padding: 16px; }
  }
  @media (max-width: 400px) { .settings-navigation { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
