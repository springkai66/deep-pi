<script lang="ts">
  import { ArrowLeft, Monitor, Palette, Server, Download, PackageOpen, Puzzle, Sparkles, Workflow, Wrench, Globe, Upload, Trash2 } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { onMount } from "svelte";
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { CODE_FONT_OPTIONS, FONT_SIZE_RANGE, type AppSettings, type ExternalEditor } from "./settings";
  import {
    BUILT_IN_THEMES,
    DEFAULT_THEME_ID,
    findBuiltInTheme,
    parseThemeFile,
    resolveTheme,
    serializeTheme,
    themeFileName,
  } from "./theme";
  import RuntimeSettings, { type RuntimeSettingsProps } from "./RuntimeSettings.svelte";
  import { SETTINGS_CATEGORIES, nextSettingsCategory, parseTaskLimit, settingsGroups, type SettingsCategory } from "./settings-navigation";
  import ExternalEditorSettings from "./ExternalEditorSettings.svelte";
  import DiagnosticsPanel from "./DiagnosticsPanel.svelte";
  import PiMcpSkillsSettings from "./PiMcpSkillsSettings.svelte";
  import { t, tm, getLocale } from "$lib/i18n.svelte";
  import { appMessageText } from "./app-messages";
  import { LOCALES, LOCALE_LABELS } from "./locale";
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
    /** 可选：+page.svelte 传入时优先渲染；未传入时由本组件内置渲染（沿用上次的项目范围）。 */
    workflows?: Snippet;
    dsh: Snippet;
    closeBlocked?: boolean;
    saving?: boolean;
    onDiagnosticsBusy: (busy: boolean) => void;
    confirmDiagnosticsClear: () => Promise<boolean>;
    /** 工作区布局（单任务 / 双列 / 网格）；由 +page.svelte 持有状态。 */
    layout?: "single" | "split" | "grid";
    onChangeLayout?: (next: "single" | "split" | "grid") => void;
  }
  let { category, onCategoryChange, onChangeSettings, onEditorSaved, onClose, models, extensions, mcp, skills, workflows, dsh, closeBlocked = false, saving = false, onDiagnosticsBusy, confirmDiagnosticsClear, layout = "single", onChangeLayout = () => {}, ...runtime }: Props = $props();
  const icons = { general: Monitor, appearance: Palette, models: Server, extensions: PackageOpen, mcp: Puzzle, skills: Sparkles, workflows: Workflow, dsh: Globe, runtime: Download, advanced: Wrench };
  let modelsVisited = $state(false);
  let extensionsVisited = $state(false);
  let mcpVisited = $state(false);
  let skillsVisited = $state(false);
  let workflowsVisited = $state(false);
  let dshVisited = $state(false);
  let advancedVisited = $state(false);
  /** 工作流市场的 busy 与错误：面板在本组件内兜底渲染时的本地状态。 */
  let workflowsBusy = $state(false);
  let workflowsError = $state("");
  let taskLimitError = $state("");
  let systemFonts = $state<string[]>([]);
  onMount(() => {
    void nativeInvoke<string[]>("list_system_fonts")
      .then((fonts) => { systemFonts = fonts; })
      .catch(() => { /* 字体枚举失败时保留“系统默认”选项 */ });
  });

  const activeTheme = $derived(resolveTheme(runtime.settings.theme, runtime.settings.customThemes ?? []));
  const themeOptions = $derived([...BUILT_IN_THEMES, ...(runtime.settings.customThemes ?? [])]);
  /** 下拉中当前选中的主题；旧配置里的未知 id 由 resolveTheme 回落到默认主题。 */
  const selectedTheme = $derived(themeOptions.find((option) => option.id === runtime.settings.theme) ?? activeTheme);
  /** 选中的主题若是用户导入的，则允许删除。 */
  const selectedCustomTheme = $derived((runtime.settings.customThemes ?? []).find((theme) => theme.id === runtime.settings.theme));
  let themeBusy = $state(false);
  let themeNotice = $state("");
  let themeError = $state("");

  /** 导入主题数量上限，与 Rust 侧 MAX_CUSTOM_THEMES 保持一致。 */
  const MAX_CUSTOM_THEMES = 32;

  function selectTheme(id: string) {
    themeNotice = "";
    themeError = "";
    updateSettings({ theme: id });
  }

  async function exportTheme() {
    if (themeBusy) return;
    themeBusy = true;
    themeNotice = "";
    themeError = "";
    try {
      const ok = await nativeInvoke<boolean>("theme_export", {
        theme: JSON.parse(serializeTheme(activeTheme)),
        suggestedName: themeFileName(activeTheme),
        // 原生对话框的标题与过滤器名由这里传入，后端不持有任何文案
        dialogTitle: appMessageText("theme.export_dialog_title", getLocale()),
        filterName: appMessageText("theme.file_filter", getLocale()),
      });
      if (ok) themeNotice = t("已导出主题「{name}」", { name: activeTheme.name });
    } catch (error) {
      themeError = tm(String(error));
    } finally {
      themeBusy = false;
    }
  }

  async function importTheme() {
    if (themeBusy) return;
    themeBusy = true;
    themeNotice = "";
    themeError = "";
    try {
      const text = await nativeInvoke<string | null>("theme_import", {
        dialogTitle: appMessageText("theme.import_dialog_title", getLocale()),
        filterName: appMessageText("theme.file_filter", getLocale()),
      });
      if (text === null) return;
      const parsed = parseThemeFile(text);
      if (!parsed.ok) {
        themeError = parsed.error;
        return;
      }
      const theme = parsed.theme;
      if (findBuiltInTheme(theme.id)) {
        themeError = t("主题 id「{id}」与内置主题冲突，请修改主题文件里的 id 后重试", { id: theme.id });
        return;
      }
      const others = (runtime.settings.customThemes ?? []).filter((item) => item.id !== theme.id);
      if (others.length >= MAX_CUSTOM_THEMES) {
        themeError = t("导入主题已达上限（{limit} 个），请先删除不再使用的主题", { limit: MAX_CUSTOM_THEMES });
        return;
      }
      updateSettings({ customThemes: [...others, theme], theme: theme.id });
      themeNotice = t("已导入并应用主题「{name}」", { name: theme.name });
    } catch (error) {
      themeError = tm(String(error));
    } finally {
      themeBusy = false;
    }
  }

  function removeTheme(id: string) {
    const next = (runtime.settings.customThemes ?? []).filter((item) => item.id !== id);
    updateSettings({
      customThemes: next,
      theme: runtime.settings.theme === id ? DEFAULT_THEME_ID : runtime.settings.theme,
    });
    themeNotice = "";
    themeError = "";
  }

  function setFontSize(field: "appFontSize" | "sessionFontSize", raw: string) {
    const value = Number(raw);
    if (!Number.isFinite(value)) return;
    updateSettings({ [field]: Math.round(Math.min(FONT_SIZE_RANGE.max, Math.max(FONT_SIZE_RANGE.min, value))) });
  }
  $effect(() => {
    if (category === "models") modelsVisited = true;
    if (category === "extensions") extensionsVisited = true;
    if (category === "mcp") mcpVisited = true;
    if (category === "skills") skillsVisited = true;
    if (category === "workflows") workflowsVisited = true;
    if (category === "dsh") dshVisited = true;
    if (category === "advanced") advancedVisited = true;
  });
  const groups = $derived(settingsGroups());
  const title = $derived(t(SETTINGS_CATEGORIES.find((item) => item.id === category)?.label ?? "设置"));

  function updateSettings(patch: Partial<AppSettings>) { onChangeSettings({ ...runtime.settings, ...patch }); }
  function changeLimit(input: HTMLInputElement) {
    const value = parseTaskLimit(input.value);
    taskLimitError = value === null ? t("同时运行任务数必须为 1 到 16 的整数") : "";
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

<section class="settings-page" aria-label={t("设置")}>
  <header class="settings-header">
    <button class="quiet-button icon-button" type="button" aria-label={t("返回工作区")} title={closeBlocked || workflowsBusy ? t("请先完成或取消当前操作") : t("返回工作区")} disabled={closeBlocked || workflowsBusy} onclick={onClose}><ArrowLeft size={17} /></button>
    <h1>{t("设置")}</h1>
    {#if saving}<span class="muted" role="status">{t("正在保存…")}</span>{/if}
  </header>
  <div class="settings-layout">
    <nav class="settings-navigation" aria-label={t("设置分类")}>
      {#each groups as group (group.label)}
        <div class="settings-nav-group">
          <div class="settings-group-label" aria-hidden="true">{t(group.label)}</div>
          {#each group.categories as item (item.id)}
            {@const Icon = icons[item.id]}
            <button id={`settings-nav-${item.id}`} type="button" aria-current={category === item.id ? "page" : undefined}
              class:active={category === item.id} onclick={() => onCategoryChange(item.id)} onkeydown={(event) => navigate(event, item.id)}>
              <Icon size={16} /><span>{t(item.label)}</span>
            </button>
          {/each}
        </div>
      {/each}
    </nav>
    <div class="settings-body">
      <h2>{title}</h2>
      <div class="settings-panel" hidden={category !== "general"}>
        <section class="settings-group" aria-labelledby="language-heading">
          <h3 id="language-heading">{t("语言")}</h3>
          <label class="setting-control"><strong>{t("界面语言")}</strong>
            <select value={runtime.settings.language} aria-label={t("界面语言")}
              onchange={(event) => updateSettings({ language: event.currentTarget.value as AppSettings["language"] })}>
              {#each LOCALES as locale}
                <option value={locale}>{LOCALE_LABELS[locale]}</option>
              {/each}
            </select>
          </label>
          <p class="muted">{t("切换后立即生效。英文译文在逐步补齐，尚未覆盖的文案会回落显示简体中文。")}</p>
        </section>
        <section class="settings-group" aria-labelledby="general-behavior-heading">
          <h3 id="general-behavior-heading">{t("应用行为")}</h3>
          <label class="setting-control"><strong>{t("关闭窗口")}</strong>
            <select value={runtime.settings.closeBehavior} aria-label={t("关闭窗口行为")}
              onchange={(event) => updateSettings({ closeBehavior: event.currentTarget.value as AppSettings["closeBehavior"] })}>
              <option value="ask">{t("首次询问")}</option><option value="minimize">{t("最小化")}</option><option value="exit">{t("退出应用")}</option>
            </select>
          </label>
          <label class="setting-control"><strong>{t("同时运行任务数")}</strong>
            <input type="number" min="1" max="16" step="1" aria-label={t("同时运行任务数")} aria-invalid={!!taskLimitError}
              aria-describedby={taskLimitError ? "task-limit-error" : undefined}
              value={runtime.settings.maxConcurrentTasks} oninput={(event) => changeLimit(event.currentTarget)} />
          </label>
          {#if taskLimitError}<p id="task-limit-error" role="alert">{taskLimitError}</p>{/if}
        </section>
      </div>
      <div class="settings-panel" hidden={category !== "appearance"}>
        <section class="settings-group" aria-labelledby="layout-heading">
          <h3 id="layout-heading">{t("布局")}</h3>
          <p class="muted">{t("工作区里任务的排布方式；顶栏不再保留布局按钮。")}</p>
          <label class="setting-control"><strong>{t("任务布局")}</strong>
            <select value={layout} aria-label={t("任务布局")} onchange={(event) => onChangeLayout(event.currentTarget.value as "single" | "split" | "grid")}>
              <option value="single">{t("单任务")}</option>
              <option value="split">{t("双列")}</option>
              <option value="grid">{t("网格")}</option>
            </select>
          </label>
        </section>
        <section class="settings-group" aria-labelledby="theme-heading">
          <div class="settings-group-header">
            <h3 id="theme-heading">{t("主题")}</h3>
            <div class="theme-actions">
              <button type="button" class="quiet-button" disabled={themeBusy} onclick={() => void importTheme()}>
                <Upload size={13} />{t("导入主题")}
              </button>
              <button type="button" class="quiet-button" disabled={themeBusy} onclick={() => void exportTheme()}>
                <Download size={13} />{t("导出当前主题")}
              </button>
            </div>
          </div>
          <p class="muted">{t("主题以 ")}<code>.deeppi-theme.json</code>{t(" 文件分发，包含配色与字体令牌；可导出分享，也可导入他人的主题。")}</p>
          <label class="setting-control"><strong>{t("主题")}</strong>
            <select value={runtime.settings.theme} aria-label={t("主题")} onchange={(event) => selectTheme(event.currentTarget.value)}>
              {#each themeOptions as option (option.id)}
                <option value={option.id}>{t(option.name)}{#if !findBuiltInTheme(option.id)}{t("（自定义）")}{/if}</option>
              {/each}
            </select>
          </label>
          <div class="theme-preview">
            <span class="theme-swatches" aria-hidden="true">
              <span style={`background:${selectedTheme.colors.pageBg}`}></span>
              <span style={`background:${selectedTheme.colors.surface}`}></span>
              <span style={`background:${selectedTheme.colors.accent}`}></span>
            </span>
            <span class="theme-copy">
              <strong>{t(selectedTheme.name)}</strong>
              <small>{t(selectedTheme.description ?? selectedTheme.id)}</small>
            </span>
            {#if selectedCustomTheme}
              <button type="button" class="quiet-button" disabled={themeBusy} title={t("删除导入的主题")}
                onclick={() => removeTheme(runtime.settings.theme)}><Trash2 size={13} />{t("删除所选自定义主题")}</button>
            {/if}
          </div>
          {#if themeNotice}<p class="theme-status" role="status">{themeNotice}</p>{/if}
          {#if themeError}<p class="theme-error" role="alert">{tm(themeError)}</p>{/if}
        </section>
        <section class="settings-group" aria-labelledby="appearance-heading">
          <h3 id="appearance-heading">{t("字体")}</h3>
          <label class="setting-control"><strong>{t("颜色模式")}</strong>
            <select value={runtime.settings.colorMode} aria-label={t("颜色模式")} onchange={(event) => updateSettings({ colorMode: event.currentTarget.value as AppSettings["colorMode"] })}>
              <option value="system">{t("跟随系统")}</option><option value="light">{t("浅色")}</option><option value="dark">{t("深色")}</option>
            </select>
          </label>
          <label class="setting-control"><strong>{t("应用程序字体")}</strong>
            <select value={runtime.settings.appFontName} aria-label={t("应用程序字体")}
              onchange={(event) => updateSettings({ appFontName: event.currentTarget.value })}>
              <option value="">{t("系统默认")}</option>
              {#each systemFonts as font}<option value={font}>{font}</option>{/each}
            </select>
          </label>
          <label class="setting-control"><strong>{t("应用字体大小（px）")}</strong>
            <input type="number" min={FONT_SIZE_RANGE.min} max={FONT_SIZE_RANGE.max} step="1" aria-label={t("应用字体大小")}
              value={runtime.settings.appFontSize}
              oninput={(event) => setFontSize("appFontSize", event.currentTarget.value)} />
          </label>
          <label class="setting-control"><strong>{t("会话窗口字体")}</strong>
            <select value={runtime.settings.sessionFontName} aria-label={t("会话窗口字体")}
              onchange={(event) => updateSettings({ sessionFontName: event.currentTarget.value })}>
              <option value="">{t("系统默认")}</option>
              {#each systemFonts as font}<option value={font}>{font}</option>{/each}
            </select>
          </label>
          <label class="setting-control"><strong>{t("会话字体大小（px）")}</strong>
            <input type="number" min={FONT_SIZE_RANGE.min} max={FONT_SIZE_RANGE.max} step="1" aria-label={t("会话字体大小")}
              value={runtime.settings.sessionFontSize}
              oninput={(event) => setFontSize("sessionFontSize", event.currentTarget.value)} />
          </label>
          <label class="setting-control"><strong>{t("代码字体")}</strong>
            <select value={runtime.settings.codeFont} aria-label={t("代码字体")} onchange={(event) => updateSettings({ codeFont: event.currentTarget.value as AppSettings["codeFont"] })}>
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
      <div class="settings-panel embedded-panel" hidden={category !== "dsh"}>{#if dshVisited}{@render dsh()}{/if}</div>
      <div class="settings-panel embedded-panel" hidden={category !== "workflows"}>
        {#if workflowsVisited}
          {#if workflows}
            {@render workflows()}
          {:else}
            <PiMcpSkillsSettings
              mode="workflows"
              onError={(error) => { workflowsError = tm(String(error)); }}
              projectPath={runtime.settings.lastProject ?? null}
              onBusyChange={(busy) => { workflowsBusy = busy; }}
            />
            {#if workflowsError}<p class="panel-error" role="alert">{tm(workflowsError)}</p>{/if}
          {/if}
        {/if}
      </div>
      <div class="settings-panel" hidden={category !== "advanced"}>
        <ExternalEditorSettings editor={runtime.settings.externalEditor} onSaved={onEditorSaved} />
        {#if advancedVisited}<DiagnosticsPanel onBusyChange={onDiagnosticsBusy} confirmClear={confirmDiagnosticsClear} />{/if}
        <section class="settings-group" aria-labelledby="diagnostics-heading">
          <h3 id="diagnostics-heading">{t("运行环境")}</h3>
          {#if runtime.runtimes.length === 0}<p class="muted" role="status">{t("尚未读取运行环境")}</p>
          {:else}
            <dl class="environment">
              {#each runtime.runtimes as component (component.id)}
                <div><dt>{component.name}</dt><dd>{component.currentVersion ?? t("未安装")} · {component.available ? t("可用") : t("不可用")} · {component.source}</dd></div>
              {/each}
            </dl>
          {/if}
          {#if runtime.appUpdate.error}<p role="alert">{tm(runtime.appUpdate.error)}</p>{/if}
          {#each runtime.updates.filter((update) => update.error) as update (update.id)}<p role="alert">{update.name}: {tm(update.error ?? "")}</p>{/each}
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
  .settings-navigation { padding: 12px 8px; border-right: 1px solid var(--border); background: var(--surface-alt); overflow-y: auto; display: grid; gap: 2px; align-content: start; }
  .settings-group-label { padding: 10px 10px 4px; font-size: 10px; font-weight: 700; letter-spacing: .04em; color: var(--text-muted); opacity: .75; user-select: none; }
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
  .theme-actions { display: flex; flex-wrap: wrap; gap: 6px; }
  .theme-preview { display: flex; align-items: center; gap: 10px; margin-top: 8px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); }
  .theme-swatches { display: flex; flex-shrink: 0; border-radius: 5px; overflow: hidden; border: 1px solid var(--border-strong); }
  .theme-swatches span { width: 14px; height: 26px; }
  .theme-copy { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .theme-copy strong { font-size: 12px; color: var(--text-strong); }
  .theme-copy small { font-size: 11px; color: var(--text-muted); overflow-wrap: anywhere; }
  .theme-status { margin: 8px 0 0; font-size: 12px; color: var(--accent); overflow-wrap: anywhere; }
  .theme-error { margin: 8px 0 0; font-size: 12px; color: var(--status-failed); overflow-wrap: anywhere; }
  .panel-error { margin: 8px 0 0; font-size: 12px; color: var(--status-failed); overflow-wrap: anywhere; }
</style>
