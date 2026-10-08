<script lang="ts">
  import { ArrowLeft, Search, Monitor, Palette, Server, Download, PackageOpen, Puzzle, Sparkles, Workflow, Wrench, Globe, Network, Upload, Trash2 } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { onMount } from "svelte";
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { CODE_FONT_OPTIONS, CHAT_DETAIL_LEVELS, FONT_SIZE_RANGE, cssAppFontFamily, cssCodeFontFamily, appFontPreviewStack, codeFontPreviewStack, type AppSettings, type ExternalEditor } from "./settings";
  import FontSelect from "./FontSelect.svelte";
  import {
    BUILT_IN_THEMES,
    DEFAULT_THEME_ID,
    findBuiltInTheme,
    parseThemeFile,
    resolveTheme,
    serializeTheme,
    themeFileName,
    themeTransparencyRange,
  } from "./theme";
  import RuntimeSettings, { type RuntimeSettingsProps } from "./RuntimeSettings.svelte";
  import { SETTINGS_CATEGORIES, filterSettingsCategories, nextSettingsCategory, parseTaskLimit, settingsGroups, type SettingsCategory } from "./settings-navigation";
  import ExternalEditorSettings from "./ExternalEditorSettings.svelte";
  import { RING_RADIUS_RANGE } from "./pet-task-ring";
  import PiMcpSkillsSettings from "./PiMcpSkillsSettings.svelte";
  import { t, tm, getLocale } from "$lib/i18n.svelte";
  import { appMessageText } from "./app-messages";
  import { LOCALES, LOCALE_LABELS } from "./locale";
  import "./settings-controls.css";

  interface Props extends Omit<RuntimeSettingsProps, "view"> {
    category: SettingsCategory;
    onCategoryChange: (category: SettingsCategory) => void;
    onChangeSettings: (settings: AppSettings) => void;
    onEditorSaved: (editor: ExternalEditor | null) => void;
    onClose: () => void;
    models: Snippet;
    codemode: Snippet;
    extensions: Snippet;
    mcp: Snippet;
    skills: Snippet;
    /** 可选：+page.svelte 传入时优先渲染；未传入时由本组件内置渲染（沿用上次的项目范围）。 */
    workflows?: Snippet;
    dsh: Snippet;
    closeBlocked?: boolean;
    saving?: boolean;
  }
  let { category, onCategoryChange, onChangeSettings, onEditorSaved, onClose, models, codemode, extensions, mcp, skills, workflows, dsh, closeBlocked = false, saving = false, ...runtime }: Props = $props();
  const icons = { general: Monitor, appearance: Palette, pi: Server, models: Server, extensions: PackageOpen, mcp: Puzzle, skills: Sparkles, workflows: Workflow, dsh: Globe, network: Network, advanced: Wrench };
  let search = $state("");
  let modelsVisited = $state(false);
  let piVisited = $state(false);
  let extensionsVisited = $state(false);
  let mcpVisited = $state(false);
  let skillsVisited = $state(false);
  let workflowsVisited = $state(false);
  let dshVisited = $state(false);
  /** 工作流市场的 busy 与错误：面板在本组件内兜底渲染时的本地状态。 */
  let workflowsBusy = $state(false);
  let workflowsError = $state("");
  let taskLimitError = $state("");
  /// 代理连通性测试结果（设置页内展示；不依赖保存防抖）。
  let proxyTesting = $state(false);
  let proxyTestResult = $state("");

  async function testProxy() {
    if (proxyTesting) return;
    proxyTesting = true;
    proxyTestResult = "";
    try {
      const result = await nativeInvoke<string>("proxy_test", {
        mode: runtime.settings.proxyMode,
        url: runtime.settings.proxyUrl,
      });
      proxyTestResult = t("连接成功（{result}）", { result });
    } catch (cause) {
      proxyTestResult = t("连接失败：{error}", { error: String(cause) });
    } finally {
      proxyTesting = false;
    }
  }
  let systemFonts = $state<string[]>([]);
  /// 应用版本号（tauri.conf.json 的 version，安装/更新后随之变化）。
  let appVersion = $state("");
  onMount(() => {
    void nativeInvoke<string[]>("list_system_fonts")
      .then((fonts) => { systemFonts = fonts; })
      .catch(() => { /* 字体枚举失败时保留“系统默认”选项 */ });
    void getVersion()
      .then((version) => { appVersion = version; })
      .catch(() => { /* 版本号读取失败时留空，不影响设置页其余功能 */ });
  });

  const activeTheme = $derived(resolveTheme(runtime.settings.theme, runtime.settings.customThemes ?? []));
  const effectiveAppFontSize = $derived(runtime.settings.appFontSize ?? activeTheme.typography?.appFontSize ?? 13);
  const effectiveSessionFontSize = $derived(runtime.settings.sessionFontSize ?? activeTheme.typography?.sessionFontSize ?? 13);
  const themeOptions = $derived([...BUILT_IN_THEMES, ...(runtime.settings.customThemes ?? [])]);
  /** 字体选项：每个选项带预览字体栈，下拉里以对应字体渲染（原生 select 做不到）。 */
  const appFontOptions = $derived([
    { value: "", label: t("跟随主题"), previewFamily: cssAppFontFamily("", activeTheme.typography?.appFont) },
    ...systemFonts.map((font) => ({ value: font, label: font, previewFamily: appFontPreviewStack(font) })),
  ]);
  const codeFontOptions = $derived.by(() => {
    const themeFont = activeTheme.typography?.codeFont;
    return [
      { value: "", label: t("跟随主题"), previewFamily: cssCodeFontFamily("", themeFont) },
      ...CODE_FONT_OPTIONS.map((option) => ({ value: option.value, label: option.label, previewFamily: cssCodeFontFamily(option.value, themeFont) })),
      ...systemFonts.map((font) => ({ value: font, label: font, previewFamily: codeFontPreviewStack(font) })),
    ];
  });
  /** 下拉中当前选中的主题；旧配置里的未知 id 由 resolveTheme 回落到默认主题。 */
  const selectedTheme = $derived(themeOptions.find((option) => option.id === runtime.settings.theme) ?? activeTheme);
  const transparencyRange = $derived(themeTransparencyRange(selectedTheme));
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
    const selected = resolveTheme(id, runtime.settings.customThemes ?? []);
    updateSettings({
      theme: id,
      themeTransparency: selected.effects?.surfaceMaterial === "glass"
        ? themeTransparencyRange(selected).default
        : runtime.settings.themeTransparency,
    });
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
      updateSettings({
        customThemes: [...others, theme],
        theme: theme.id,
        themeTransparency: theme.effects?.surfaceMaterial === "glass"
          ? themeTransparencyRange(theme).default
          : runtime.settings.themeTransparency,
      });
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
    if (category === "pi") piVisited = true;
    if (category === "extensions") extensionsVisited = true;
    if (category === "mcp") mcpVisited = true;
    if (category === "skills") skillsVisited = true;
    if (category === "workflows") workflowsVisited = true;
    if (category === "dsh") dshVisited = true;
  });
  const matches = $derived(filterSettingsCategories(search, t));
  const groups = $derived(settingsGroups(SETTINGS_CATEGORIES.filter((item) => matches.includes(item.id))));
  const title = $derived(t(SETTINGS_CATEGORIES.find((item) => item.id === category)?.label ?? "设置"));

  function updateSettings(patch: Partial<AppSettings>) { onChangeSettings({ ...runtime.settings, ...patch }); }
  function changeLimit(input: HTMLInputElement) {
    const value = parseTaskLimit(input.value);
    taskLimitError = value === null ? t("同时运行任务数必须为 1 到 16 的整数") : "";
    if (value !== null) updateSettings({ maxConcurrentTasks: value });
  }
  // —— 通知与桌宠：形象预览 / 本地图片 ——
  type PetAppearance = {
    kind: "image";
    mime: string;
    dataBase64: string;
    isDefault: boolean;
  };
  let petAppearance = $state<PetAppearance | null>(null);
  let petNotice = $state("");
  let petImageSrc = $derived(
    petAppearance ? `data:${petAppearance.mime};base64,${petAppearance.dataBase64}` : "",
  );

  async function refreshPetAppearance() {
    try {
      petAppearance = await nativeInvoke<PetAppearance>("get_pet_appearance");
      petNotice = "";
    } catch (cause) {
      petNotice = tm(String(cause));
    }
  }

  async function choosePetImage() {
    try {
      const picked = await openFileDialog({
        directory: false,
        multiple: false,
        title: t("选择桌宠形象图片"),
        filters: [{ name: t("图片"), extensions: ["png", "jpg", "jpeg", "webp", "svg", "gif"] }],
      });
      if (typeof picked !== "string") return;
      await nativeInvoke("set_pet_image", { path: picked });
      await refreshPetAppearance();
    } catch (cause) {
      petNotice = tm(String(cause));
    }
  }

  async function resetPetImage() {
    try {
      await nativeInvoke("reset_pet_image");
      await refreshPetAppearance();
    } catch (cause) {
      petNotice = tm(String(cause));
    }
  }

  function navigate(event: KeyboardEvent, current: SettingsCategory) {
    if (event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return;
    const next = nextSettingsCategory(current, event.key, matches);
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
      <div class="settings-search">
        <Search size={15} aria-hidden="true" />
        <input type="search" bind:value={search} aria-label={t("搜索设置")}
          placeholder={t("搜索设置")}
          onkeydown={(event) => {
            if (event.key === "Escape" && search) {
              event.stopPropagation();
              search = "";
            } else if (event.key === "ArrowDown" && matches.length) {
              event.preventDefault();
              document.getElementById(`settings-nav-${matches[0]}`)?.focus();
            }
          }} />
      </div>
      {#each groups as group (group.label)}
        <div class="settings-nav-group">
          <div class="settings-group-label">{t(group.label)}</div>
          {#each group.categories as item, index (item.id)}
            {#if item.subgroup && item.subgroup !== group.categories[index - 1]?.subgroup}
              <div class="settings-subgroup-label">{t(item.subgroup)}</div>
            {/if}
            {@const Icon = icons[item.id]}
            <button id={`settings-nav-${item.id}`} type="button" aria-current={category === item.id ? "page" : undefined}
              class:active={category === item.id} onclick={() => onCategoryChange(item.id)} onkeydown={(event) => navigate(event, item.id)}>
              <Icon size={16} /><span>{t(item.label)}</span>
            </button>
          {/each}
        </div>
      {/each}
      {#if search && matches.length === 0}<p class="settings-no-results" role="status">{t("没有匹配的设置")}</p>{/if}
    </nav>
    <div class="settings-body" class:has-embedded={category === "models" || category === "extensions" || category === "mcp" || category === "skills" || category === "dsh" || category === "workflows"}>
      <h2 id="settings-panel-title">{title}</h2>
      <div class="settings-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "general"}>
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
          <p class="muted">{t("仅统计 AI 正在执行任务的会话；空闲打开的会话不占额度，TUI 终端会话始终计入。")}</p>
          {#if taskLimitError}<p id="task-limit-error" role="alert">{taskLimitError}</p>{/if}
        </section>
        <section class="settings-group" aria-labelledby="terminal-heading">
          <h3 id="terminal-heading">{t("命令终端")}</h3>
          <label class="setting-control"><strong>{t("默认 Shell")}</strong>
            <select value={runtime.settings.terminalShell} aria-label={t("默认 Shell")}
              onchange={(event) => updateSettings({ terminalShell: event.currentTarget.value as AppSettings["terminalShell"] })}>
              <option value="powershell">PowerShell</option>
              <option value="pwsh">PowerShell 7</option>
              <option value="bash">Bash</option>
              <option value="cmd">{t("命令提示符")}</option>
            </select>
          </label>
          <p class="muted">{t("只影响之后打开的命令终端；Pi 终端仍用于 Pi TUI 会话。")}</p>
        </section>
        <ExternalEditorSettings editor={runtime.settings.externalEditor} onSaved={onEditorSaved} />
        <section class="settings-group" aria-labelledby="notification-heading">
          <h3 id="notification-heading">{t("通知与桌宠")}</h3>
          <label class="setting-control setting-check">
            <input type="checkbox" checked={runtime.settings.notifyOnTaskComplete} aria-label={t("任务完成通知")}
              onchange={(event) => updateSettings({ notifyOnTaskComplete: event.currentTarget.checked })} />
            <strong>{t("任务完成通知")}</strong>
          </label>
          <p class="muted">{t("任务完成或失败时发送系统通知；应用切到后台（最小化/失焦）也会提示。")}</p>
          <label class="setting-control setting-check">
            <input type="checkbox" checked={runtime.settings.petEnabled} aria-label={t("显示桌宠")}
              onchange={(event) => updateSettings({ petEnabled: event.currentTarget.checked })} />
            <strong>{t("显示桌宠")}</strong>
          </label>
          <p class="muted">{t("勾选立即显示、取消立即关闭；下次启动也按此设置。桌宠常驻桌面，随任务状态做动作。")}</p>
          <label class="setting-control setting-check">
            <input type="checkbox" checked={runtime.settings.petAlwaysOnTop} aria-label={t("桌宠置顶显示")}
              onchange={(event) => updateSettings({ petAlwaysOnTop: event.currentTarget.checked })} />
            <strong>{t("桌宠置顶显示")}</strong>
          </label>
          <p class="muted">{t("桌宠浮窗显示在最上方，不被其他窗口遮挡；关闭后可被覆盖。")}</p>
          <label class="setting-control"><strong>{t("任务环半径")}</strong>
            <input type="range" min={RING_RADIUS_RANGE.min} max={RING_RADIUS_RANGE.max} step={RING_RADIUS_RANGE.step}
              aria-label={t("任务环半径")} value={runtime.settings.petTaskRingRadius}
              oninput={(event) => updateSettings({ petTaskRingRadius: Number(event.currentTarget.value) })} />
            <span class="muted">{runtime.settings.petTaskRingRadius}px</span>
          </label>
          <p class="muted">{t("气泡环绕桌宠的距离；调近时气泡会自动收窄，任务多到放不下才会略微外扩。")}</p>
          <div class="setting-control pet-appearance">
            <strong>{t("桌宠形象")}</strong>
            <div class="pet-preview-row">
              <span class="pet-preview" aria-hidden="true">
                {#if petAppearance}
                  <img src={petImageSrc} alt="" />
                {/if}
              </span>
              <div class="pet-actions">
                <button type="button" class="quiet-button" onclick={() => void choosePetImage()}>{t("选择图片…")}</button>
                <button type="button" class="quiet-button" disabled={!petAppearance || petAppearance.isDefault}
                  onclick={() => void resetPetImage()}>{t("恢复默认")}</button>
              </div>
            </div>
            {#if petNotice}<p class="pet-notice" role="alert">{petNotice}</p>{/if}
          </div>
        </section>
      </div>
      <div class="settings-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "appearance"}>
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
          <label class="setting-control"><strong>{t("颜色模式")}</strong>
            <select value={runtime.settings.colorMode} aria-label={t("颜色模式")} onchange={(event) => updateSettings({ colorMode: event.currentTarget.value as AppSettings["colorMode"] })}>
              <option value="system">{t("跟随系统")}</option><option value="light">{t("浅色")}</option><option value="dark">{t("深色")}</option>
            </select>
          </label>
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

          {#if selectedTheme.effects?.surfaceMaterial === "glass"}
            <label class="setting-control theme-transparency-control">
              <span class="setting-copy">
                <strong>{t("主题透明度")}</strong>
                <small>{t("0% 为不透明，数值越高越透明。")}</small>
              </span>
              <span class="theme-transparency-slider">
                <input type="range" min={transparencyRange.min} max={transparencyRange.max} step="1"
                  value={runtime.settings.themeTransparency} aria-label={t("主题透明度")}
                  oninput={(event) => updateSettings({ themeTransparency: Number(event.currentTarget.value) })} />
                <output>{runtime.settings.themeTransparency}%</output>
              </span>
            </label>
          {/if}
        </section>
        <section class="settings-group" aria-labelledby="appearance-heading">
          <h3 id="appearance-heading">{t("字体")}</h3>
          <div class="setting-control"><strong>{t("应用程序字体")}</strong>
            <FontSelect value={runtime.settings.appFontName} ariaLabel={t("应用程序字体")}
              options={appFontOptions} onchange={(value) => updateSettings({ appFontName: value })} />
          </div>
          <label class="setting-control"><strong>{t("应用字体大小（px）")}</strong>
            <input type="number" min={FONT_SIZE_RANGE.min} max={FONT_SIZE_RANGE.max} step="1" aria-label={t("应用字体大小")}
              value={effectiveAppFontSize}
              oninput={(event) => setFontSize("appFontSize", event.currentTarget.value)} />
          </label>
          <div class="setting-control"><strong>{t("会话窗口字体")}</strong>
            <FontSelect value={runtime.settings.sessionFontName} ariaLabel={t("会话窗口字体")}
              options={appFontOptions} onchange={(value) => updateSettings({ sessionFontName: value })} />
          </div>
          <label class="setting-control"><strong>{t("会话字体大小（px）")}</strong>
            <input type="number" min={FONT_SIZE_RANGE.min} max={FONT_SIZE_RANGE.max} step="1" aria-label={t("会话字体大小")}
              value={effectiveSessionFontSize}
              oninput={(event) => setFontSize("sessionFontSize", event.currentTarget.value)} />
          </label>
          <div class="setting-control"><strong>{t("代码字体")}</strong>
            <FontSelect value={runtime.settings.codeFont} ariaLabel={t("代码字体")}
              options={codeFontOptions} onchange={(value) => updateSettings({ codeFont: value as AppSettings["codeFont"] })} />
          </div>
        </section>
        <section class="settings-group" aria-labelledby="chat-detail-heading">
          <h3 id="chat-detail-heading">{t("会话内容")}</h3>
          <label class="setting-control"><strong>{t("对话内容显示")}</strong>
            <select value={runtime.settings.chatDetailLevel} aria-label={t("对话内容显示")}
              onchange={(event) => updateSettings({ chatDetailLevel: event.currentTarget.value as AppSettings["chatDetailLevel"] })}>
              {#each CHAT_DETAIL_LEVELS as option}<option value={option.value}>{t(option.label)}</option>{/each}
            </select>
          </label>
          <p class="muted">{t("简洁模式显示回复和必要运行状态；完整模式显示思考、工具调用及输出等过程内容。")}</p>
        </section>
      </div>
      <div class="settings-panel embedded-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "models"}>{#if modelsVisited}{@render models()}{/if}</div>
      <div class="settings-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "pi"}>
        <RuntimeSettings {...runtime} view="pi" />
        {#if piVisited}{@render codemode()}{/if}
      </div>
      <div class="settings-panel embedded-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "extensions"}>{#if extensionsVisited}{@render extensions()}{/if}</div>
      <div class="settings-panel embedded-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "mcp"}>{#if mcpVisited}{@render mcp()}{/if}</div>
      <div class="settings-panel embedded-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "skills"}>{#if skillsVisited}{@render skills()}{/if}</div>
      <div class="settings-panel embedded-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "dsh"}>{#if dshVisited}<RuntimeSettings {...runtime} view="dsh" />{@render dsh()}{/if}</div>
      <div class="settings-panel embedded-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "workflows"}>
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
      <div class="settings-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "network"}>
        <section class="settings-group" aria-labelledby="proxy-heading">
          <h3 id="proxy-heading">{t("网络代理")}</h3>
          <label class="setting-control"><strong>{t("代理模式")}</strong>
            <select value={runtime.settings.proxyMode} aria-label={t("代理模式")}
              onchange={(event) => updateSettings({ proxyMode: event.currentTarget.value as AppSettings["proxyMode"] })}>
              <option value="system">{t("跟随系统")}</option>
              <option value="manual">{t("手动设置")}</option>
            </select>
          </label>
          <p class="muted">{t("跟随系统使用 Windows 系统代理；若 Windows 系统代理未开启（包括 TUN 透明接管流量），则不强制设置应用层代理，按操作系统路由连接。需要指定代理时选择「手动设置」。")}</p>
          {#if runtime.settings.proxyMode === "manual"}
            <label class="setting-control"><strong>{t("代理地址")}</strong>
              <input type="text" value={runtime.settings.proxyUrl} placeholder="http://127.0.0.1:7890"
                autocomplete="off" spellcheck="false" aria-label={t("代理地址")}
                oninput={(event) => updateSettings({ proxyUrl: event.currentTarget.value })} />
            </label>
            <label class="setting-control"><strong>{t("例外地址")}</strong>
              <input type="text" value={runtime.settings.proxyNoProxy} placeholder="localhost,127.0.0.1"
                autocomplete="off" spellcheck="false" aria-label={t("例外地址")}
                oninput={(event) => updateSettings({ proxyNoProxy: event.currentTarget.value })} />
            </label>
          {/if}
          <div class="setting-control">
            <strong>{t("连通性测试")}</strong>
            <div class="proxy-test">
              <button type="button" class="quiet-button" disabled={proxyTesting} onclick={() => void testProxy()}>
                {#if proxyTesting}{t("测试中…")}{:else}{t("测试连接")}{/if}
              </button>
              {#if proxyTestResult}<span class="proxy-state" role="status">{proxyTestResult}</span>{/if}
            </div>
          </div>
        </section>
      </div>
      <div class="settings-panel" role="region" aria-labelledby="settings-panel-title" hidden={category !== "advanced"}>
        <section class="settings-group" aria-labelledby="runtime-about-heading">
          <h3 id="runtime-about-heading">{t("运行环境与关于")}</h3>
          {#if runtime.runtimes.length === 0}<p class="muted" role="status">{t("尚未读取运行环境")}</p>
          {:else}
            <dl class="environment">
              {#each runtime.runtimes as component (component.id)}
                <div><dt>{component.name}</dt><dd>{component.currentVersion ?? t("未安装")} · {component.available ? t("可用") : t("不可用")} · {component.source}</dd></div>
              {/each}
            </dl>
          {/if}
          {#each runtime.updates.filter((update) => update.error) as update (update.id)}<p role="alert">{update.name}: {tm(update.error ?? "")}</p>{/each}
          <p class="muted">DeepPi {appVersion}</p>
        </section>
      </div>
    </div>
  </div>
</section>

<style>
  .settings-page {
    height: 100%; min-height: 0; display: flex; flex-direction: column;
    color: var(--text); font-family: var(--text-font);
    background: var(--page-bg);
  }
  .settings-header {
    min-height: 64px; flex-shrink: 0; display: flex; align-items: center; gap: 12px;
    padding: 12px 22px; border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    box-shadow: 0 1px 0 rgb(255 255 255 / 2%); backdrop-filter: blur(14px);
  }
  .settings-header h1 { margin: 0; color: var(--text-strong); font-size: 17px; font-weight: 700; letter-spacing: -.018em; }
  .settings-header .muted {
    display: inline-flex; align-items: center; min-height: 24px; margin-left: 2px; padding: 3px 9px;
    border: 1px solid color-mix(in srgb, var(--accent) 24%, var(--border)); border-radius: 999px;
    color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, transparent); font-size: 11px;
  }
  h2 {
    display: flex; align-items: center; gap: 14px; margin: 0; padding: 0 0 20px;
    color: var(--text-strong); font-size: 22px; font-weight: 700; letter-spacing: -.025em;
  }
  h2::after { content: ""; flex: 1; height: 1px; background: linear-gradient(90deg, var(--border-strong), transparent); }
  .settings-layout { display: grid; grid-template-columns: 224px minmax(0, 1fr); flex: 1; min-height: 0; }
  .settings-navigation {
    padding: 14px 10px 18px; border-right: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface-alt) 86%, var(--page-bg));
    overflow-y: auto; display: grid; gap: 3px; align-content: start;
  }
  .settings-search {
    display: flex; align-items: center; gap: 8px; margin: 2px 3px 12px; padding: 0 10px; min-height: 38px;
    border: 1px solid var(--border-strong); border-radius: 9px; background: var(--surface);
    color: var(--text-muted); box-shadow: 0 5px 16px rgb(0 0 0 / 8%); transition: border-color .15s ease, box-shadow .15s ease;
  }
  .settings-search:focus-within { border-color: color-mix(in srgb, var(--accent) 64%, var(--border-strong)); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 12%, transparent), 0 5px 16px rgb(0 0 0 / 8%); outline: 0; }
  .settings-search input { width: 100%; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--text); font: inherit; font-size: 12px; }
  .settings-group-label { padding: 13px 10px 5px; color: var(--text-muted); font-size: 10px; font-weight: 750; letter-spacing: .085em; user-select: none; }
  .settings-subgroup-label { padding: 11px 10px 4px 21px; color: var(--text-subtle); font-size: 11px; font-weight: 650; }
  .settings-no-results { margin: 4px 3px; padding: 10px; border: 1px dashed var(--border-strong); border-radius: 8px; color: var(--text-muted); background: color-mix(in srgb, var(--surface) 65%, transparent); font-size: 12px; }
  .settings-navigation button {
    display: flex; align-items: center; gap: 10px; min-height: 39px; width: 100%; padding: 8px 11px;
    border: 1px solid transparent; border-radius: 8px; background: transparent; color: var(--text-muted);
    text-align: left; cursor: pointer; transition: color .15s ease, background .15s ease, border-color .15s ease, transform .15s ease;
  }
  .settings-navigation button span { min-width: 0; overflow-wrap: anywhere; }
  .settings-navigation button:hover { border-color: var(--border); background: var(--surface-hover); color: var(--text); transform: translateX(1px); }
  .settings-navigation button.active {
    border-color: color-mix(in srgb, var(--accent) 28%, var(--border)); color: var(--text-strong);
    background: var(--surface-raised);
    box-shadow: inset 3px 0 var(--accent), 0 4px 12px rgb(0 0 0 / 8%);
  }
  .settings-navigation button:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .settings-body {
    display: flex; flex-direction: column; min-width: 0; min-height: 0; overflow: auto;
    padding: 28px clamp(18px, 4vw, 48px); container-type: inline-size; scrollbar-gutter: stable;
  }
  .settings-panel { width: min(100%, 960px); max-width: 960px; }
  .settings-body > .settings-panel:not(.embedded-panel) > .settings-group {
    margin: 0 0 20px; padding: 0 0 16px; border: 0; border-radius: 0;
    background: transparent; box-shadow: none;
  }
  .settings-body > .settings-panel:not(.embedded-panel) > .settings-group:last-child { margin-bottom: 0; }
  .settings-body.has-embedded { overflow: hidden; }
  .embedded-panel {
    flex: 1; min-height: 0; max-width: none; overflow: auto; padding: 0 0 20px;
    border: 0; border-radius: 0; background: transparent; box-shadow: none;
  }
  .environment { margin: 0; }
  .environment div { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; padding: 13px 0; border-bottom: 1px solid var(--border); }
  .embedded-panel :global(.settings-group) { margin-bottom: 20px; border: 0; border-radius: 0; background: transparent; box-shadow: none; }
  dt { color: var(--text-strong); font-weight: 650; }
  dd { margin: 0; color: var(--text-muted); overflow-wrap: anywhere; }
  @media (max-width: 760px) {
    .settings-header { min-height: 58px; padding: 10px 14px; }
    .settings-layout { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, auto) minmax(0, 1fr); }
    .settings-navigation { display: block; max-height: min(38vh, 260px); padding: 10px 8px 12px; border-right: 0; border-bottom: 1px solid var(--border); }
    .settings-nav-group { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 4px; }
    .settings-group-label, .settings-subgroup-label { grid-column: 1 / -1; }
    .settings-navigation button { min-height: 40px; font-size: 12px; gap: 6px; padding: 8px; }
    .settings-body { padding: 20px 14px; }
    h2 { padding-bottom: 16px; font-size: 20px; }
    .settings-body > .settings-panel:not(.embedded-panel) > .settings-group { padding: 0 0 12px; border-radius: 0; }
  }
  @media (max-width: 400px) { .settings-nav-group { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  .theme-actions { display: flex; flex-wrap: wrap; gap: 7px; }
  .theme-preview { display: flex; align-items: center; gap: 12px; margin-top: 10px; padding: 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-alt); }
  .theme-swatches { display: flex; flex-shrink: 0; border-radius: 7px; overflow: hidden; border: 1px solid var(--border-strong); box-shadow: 0 3px 10px rgb(0 0 0 / 12%); }
  .theme-swatches span { width: 17px; height: 32px; }
  .theme-copy { flex: 1; min-width: 0; display: grid; gap: 3px; }
  .theme-copy strong { font-size: 12px; color: var(--text-strong); }
  .theme-copy small { font-size: 11px; color: var(--text-muted); overflow-wrap: anywhere; }
  .theme-status { margin: 10px 0 0; font-size: 12px; color: var(--accent); overflow-wrap: anywhere; }
  .theme-error { margin: 10px 0 0; font-size: 12px; color: var(--status-failed); overflow-wrap: anywhere; }
  .panel-error { margin: 10px 0 0; font-size: 12px; color: var(--status-failed); overflow-wrap: anywhere; }
  .proxy-test { display: flex; align-items: center; justify-content: flex-end; gap: 10px; min-width: 0; }
  .proxy-state { min-width: 0; overflow-wrap: anywhere; color: var(--text-muted); font-size: 12px; }
  /* —— 通知与桌宠 —— */
  .setting-check { display: flex; align-items: center; justify-content: flex-start; gap: 9px; }
  .setting-check input[type="checkbox"] { width: 15px; height: 15px; flex-shrink: 0; accent-color: var(--accent); cursor: pointer; }
  .pet-preview-row { display: flex; align-items: center; gap: 12px; margin: 10px 0; }
  .pet-preview {
    display: inline-flex; align-items: center; justify-content: center;
    width: 68px; height: 68px; flex-shrink: 0; border: 1px dashed var(--border-strong);
    border-radius: 12px; overflow: hidden; background: var(--surface-alt); box-shadow: inset 0 0 0 4px color-mix(in srgb, var(--surface) 65%, transparent);
  }
  .pet-preview img { width: 100%; height: 100%; object-fit: contain; }
  .pet-actions { display: flex; flex-wrap: wrap; gap: 7px; }
  .pet-notice { color: var(--status-failed); font-size: 12px; margin: 6px 0 0; }
  .theme-transparency-control { align-items: center; }
  .theme-transparency-slider { display: flex; align-items: center; gap: 10px; width: min(280px, 100%); }
  .theme-transparency-slider input { flex: 1; min-width: 100px; accent-color: var(--accent); cursor: pointer; }
  .theme-transparency-slider output { min-width: 40px; color: var(--text-muted); font-variant-numeric: tabular-nums; text-align: right; }
</style>
