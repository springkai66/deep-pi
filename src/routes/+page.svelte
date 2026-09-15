<script lang="ts">
  import { createDialogQueue } from "$lib/dialog";
  import { createWindowCloseHandler } from "$lib/window-close";
  import { loadAppStartup } from "$lib/startup";
  let gitIndexBusy = $state(false);
  import { createOperationRunner, type OperationState } from "$lib/operation";
  import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
  import { createProjectWatch, type ProjectWatchEvent } from "$lib/project-watch";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
  import { Webview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    Bot,
    FolderPlus,
    LayoutGrid,
    PanelLeft,
    PanelRight,
    PanelTop,
    Plus,
    RotateCcw,
    Settings2,
    Globe,
    GitBranch,
  } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import AppMenu from "$lib/AppMenu.svelte";
  import FileSidebar from "$lib/FileSidebar.svelte";
  import { createFileWorkspace, type FileDocument, type FileSaveResult } from "$lib/file-workspace";
  import type { FilePreview } from "$lib/files";
  import GitSidebar from "$lib/GitSidebar.svelte";
  import GitDiffView from "$lib/GitDiffView.svelte";
  import type { GitDiffSelection } from "$lib/git-diff";
  import type { GitDiffArea } from "$lib/git-status";
  import ChatPane from "$lib/ChatPane.svelte";
  import { hostCommandEnabled, hostShortcutDecision, shortcutLabel, shortcutAria, type HostCommand, type HostShortcutContext } from "$lib/shortcuts";
  import { createTaskModeSwitcher, type InteractionMode } from "$lib/task-mode";
  import { SplitSquareHorizontal, SplitSquareVertical } from "@lucide/svelte";
  import AppDialog from "$lib/AppDialog.svelte";
  import { checkAppUpdate, installAppUpdate, type AppUpdateState, type Update } from "$lib/app-update";
  import type { DialogRequest, DialogValue } from "$lib/dialog";
  import type { Project } from "$lib/project";
  import PiMarketplace from "$lib/PiMarketplace.svelte";
  import PiMcpSkillsSettings from "$lib/PiMcpSkillsSettings.svelte";
  import PiProviderSettings from "$lib/PiProviderSettings.svelte";
  import PiSettings from "$lib/PiSettings.svelte";
  import type { SettingsCategory } from "$lib/settings-navigation";
  import { createSettingsSaver } from "$lib/settings-save";
  import TaskSidebar from "$lib/TaskSidebar.svelte";
  import TaskTabs from "$lib/TaskTabs.svelte";
  import type { RuntimeComponent, RuntimeUpdate } from "$lib/runtime";
  import { SNOOZE_DURATION_MS } from "$lib/runtime";
  import {
    DEFAULT_APP_SETTINGS,
    cssFontFamily,
    isLightColorMode,
    type AppSettings,
    type CloseBehavior,
  } from "$lib/settings";
  import type { Task, TaskStatus } from "$lib/task";
  import {
    changePaneCapacity,
    hasProjectConflict,
    openPane,
    splitPane,
  } from "$lib/workspace";

  type LayoutMode = "single" | "split" | "grid";

  interface TaskStatusUpdate {
    taskId: string;
    runId: string;
    status: TaskStatus;
  }

  interface DshHostStatus {
    running: boolean;
    url: string | null;
    error: string | null;
  }

  let tasks = $state<Task[]>([]);
  let projects = $state<Project[]>([]);
  let runtimes = $state<RuntimeComponent[]>([]);
  let updates = $state<RuntimeUpdate[]>([]);
  let isCheckingUpdates = $state(false);
  let busyRuntime = $state<string | null>(null);
  let restartBusy = $state<string | null>(null);
  let runtimeOperation = $state<OperationState | null>(null);
  let runtimeProgress = $state<{ phase: string; percent: number | null } | null>(null);
  const runtimeRunner = createOperationRunner(invoke, (state) => {
    runtimeOperation = state;
    if (!state) runtimeProgress = null;
  });
  let appUpdate = $state<AppUpdateState>({
    status: "idle",
    version: null,
    notes: null,
    error: null,
  });
  let pendingAppUpdate: Update | null = null;
  let settings = $state<AppSettings>({ ...DEFAULT_APP_SETTINGS });
  let settingsSaving = $state(false);
  const settingsSaver = createSettingsSaver(
    (next) => invoke("save_settings", { settings: next }), showError, (busy) => { settingsSaving = busy; },
  );
  let terminalTaskIds = $state<string[]>([]);
  let paneTaskIds = $state<string[]>([]);
  let activeTaskId = $state<string | null>(null);
  let selectedProjectId = $state<string | null>(null);
  let isStarting = $state(false);
  let startupPending = $state(true);
  let startupFailure = $state("");
  let pageDisposed = false;
  let errorMessage = $state("");
  let layout = $state<LayoutMode>("single");
  let view = $state<"workspace" | "settings">("workspace");
  let settingsCategory = $state<SettingsCategory>("general");
  let diagnosticsBusy = $state(false);
  let settingsPackageBusy = $state(false);
  let activeAgent = $state<"pi" | "dsh">("pi");
  let isDshStarting = $state(false);
  let dshHostError = $state<string | null>(null);
  let dialogRequest = $state<DialogRequest | null>(null);
  const dialogs = createDialogQueue((request) => { dialogRequest = request; });
  let switchingTasks = $state(new Set<string>());
  const modeSwitcher = createTaskModeSwitcher({
    confirm: (task, target) => confirmDialog(
      target === "rpc" ? "切换到对话模式" : "切换到终端兼容模式",
      `停止“${task.title}”的当前进程，并使用同一个 Pi 会话继续吗？当前未完成的响应可能中断。`,
    ),
    stop: (task) => invoke(task.interactionMode === "rpc" ? "stop_rpc_task" : "stop_pi_run", {
      taskId: task.id, runId: task.runId,
    }),
    restart: (task, mode) => requestTaskRestart(task, mode),
    busyChanged: (id, busy) => {
      const next = new Set(switchingTasks);
      if (busy) next.add(id); else next.delete(id);
      switchingTasks = next;
    },
  });
  // Assigned through `bind:this` on the workspace element below; oxlint does not
  // see template bindings, so the assignment is suppressed for this declaration.
  // eslint-disable-next-line no-unassigned-vars
  let workspace: HTMLElement;
  let dshWebview = $state.raw<Webview | null>(null);
  let sidebarVisible = $state(true);
  let gitVisible = $state(false);
  const showGit = $derived(gitVisible && activeAgent === "pi" && view === "workspace");
  let filesVisible = $state(true);
  const showFiles = $derived(activeAgent === "pi" && view === "workspace" && filesVisible);
  let fileSidebarVisited = $state(false);
  $effect(() => {
    if (showFiles) fileSidebarVisited = true;
  });
  let fileSearchFocusToken = $state(0);
  let filesRefreshToken = $state(0);
  let gitWatchRefreshToken = $state(0);
  let watchRetry = $state(0);
  let watchError = $state("");
  $effect(() => {
    const projectId = selectedProjectId;
    void watchRetry;
    watchError = "";
    if (!projectId || !isTauri()) return;
    const watch = createProjectWatch(projectId, {
      start: (watchId, callback) => {
        const channel = new Channel<ProjectWatchEvent>();
        channel.onmessage = callback;
        return invoke("start_project_watch", { projectId, watchId, channel });
      },
      close: (watchId) => invoke("stop_project_watch", { projectId, watchId }),
      ping: (watchId) => invoke("ping_project_watch", { projectId, watchId }),
      changed: (files, git) => { if (files) filesRefreshToken++; if (git) gitWatchRefreshToken++; },
      status: (error) => { watchError = error; },
    });
    void watch.start();
    return () => { void watch.dispose(); };
  });
  let composerFocusToken = $state(0);
  let openedFile = $state<{ projectId: string; path: string; line?: number; column?: number } | null>(null);
  let fileDocuments = $state.raw<FileDocument[]>([]);
  let editorModule = $state.raw<Promise<typeof import("$lib/FileEditor.svelte")> | null>(null);
  let fileEditor = $state<{ save(asNew?: boolean): Promise<void> }>();
  let closingWindow = $state(false);
  let recoveryBusy = $state(false);
  let recoveryProject = $state<{ id: string; name: string } | null>(null);
  let recoveryModule = $state.raw<Promise<typeof import("$lib/FileRecoveryPanel.svelte")> | null>(null);
  const fileWorkspace = createFileWorkspace({
    read: (projectId, relativePath) => invoke<FilePreview>("read_project_file", { projectId, relativePath }),
    save: (projectId, relativePath, content, expectedVersion) => invoke<FileSaveResult>("save_project_file", { projectId, relativePath, content, expectedVersion }),
    chooseClose: async (documents) => {
      const result = await dialogs.request({
        kind: "choice", title: "未保存的文件",
        message: documents.map((doc) => `${projects.find((project) => project.id === doc.projectId)?.name ?? doc.projectId} / ${doc.path}`).join("\n"),
        choices: [{ value: "save", label: "保存后继续" }, { value: "discard", label: "放弃未保存的修改" }],
      });
      return result === "save" || result === "discard" ? result : null;
    },
    changed: (documents) => { fileDocuments = documents; },
    saved: (projectId) => { if (projectId === selectedProjectId) filesRefreshToken++; },
  });
  const fileSaving = $derived(fileDocuments.some((doc) => doc.saving || doc.locked));
  let openedDiff = $state<GitDiffSelection | null>(null);
  const inspectingFile = $derived(!!openedFile || !!openedDiff);
  let searchFocusToken = $state(0);
  let menuOpen = $state(false);
  let dshVisibility = Promise.resolve();
  const showSidebar = $derived(activeAgent === "pi" && view === "workspace" && sidebarVisible);
  const activeTask = $derived(tasks.find((task) => task.id === activeTaskId));
  const runningPiCount = $derived(tasks.filter((task) => task.agent === "pi" && ["running", "waiting"].includes(task.status)).length);
  const dshRunning = $derived(dshWebview !== null && !dshHostError);
  let terminalModule = $state.raw<Promise<typeof import("$lib/TerminalPane.svelte")> | null>(null);
  function loadTerminalModule() {
    terminalModule = import("$lib/TerminalPane.svelte");
    void terminalModule.catch(() => {});
  }
  $effect(() => {
    if (allTerminalTasks.some((task) => task.interactionMode !== "rpc") && !terminalModule) {
      loadTerminalModule();
    }
  });

  // Serialize visibility requests because native child Webviews cover HTML menus.
  $effect(() => {
    const child = dshWebview;
    const visible = activeAgent === "dsh" && !menuOpen && !dialogRequest && !closingWindow && !recoveryProject;
    dshVisibility = dshVisibility.then(async () => {
      if (child) {
        if (visible) await child.show();
        else await child.hide();
      }
    }).catch((error) => { console.error("DSH visibility:", error); });
  });

  async function focusTaskSearch() {
    await showPi();
    sidebarVisible = true;
    await tick();
    searchFocusToken++;
  }

  async function focusFileSearch() {
    await showPi();
    filesVisible = true;
    await tick();
    fileSearchFocusToken++;
  }

  function openFile(path: string, line?: number, column?: number) {
    openedDiff = null;
    if (selectedProjectId) {
      if (!editorModule) editorModule = import("$lib/FileEditor.svelte");
      openedFile = { projectId: selectedProjectId, path, line, column };
      void fileWorkspace.open(selectedProjectId, path).catch(showError);
    }
  }

  function openGitDiff(path: string, area: GitDiffArea) {
    if (!selectedProjectId) return;
    openedFile = null;
    openedDiff = { projectId: selectedProjectId, path, area };
    if (window.innerWidth <= 1100) gitVisible = false;
  }

  function shortcutContext(): HostShortcutContext {
    const fileId = openedFile ? fileWorkspace.id(openedFile.projectId, openedFile.path) : null;
    return {
      pi: activeAgent === "pi", workspace: view === "workspace", rpc: activeTask?.interactionMode === "rpc",
      fileOpen: !!openedFile, fileReady: !!fileEditor && !!fileId && fileDocuments.some((doc) => doc.id === fileId && !!doc.state),
      fileBusy: fileSaving, navigationLocked: view === "settings" && (settingsPackageBusy || diagnosticsBusy),
      modal: !!dialogRequest, recovery: !!recoveryProject, closing: closingWindow,
    };
  }

  function runHostCommand(command: HostCommand) {
    if (!hostCommandEnabled(command, shortcutContext())) return;
    if (command === "save" || command === "saveAs") { void fileEditor?.save(command === "saveAs"); return; }
    if (command === "tasks") void focusTaskSearch();
    else if (command === "files") void focusFileSearch();
    else if (command === "settings") openSettings();
    else if (command === "composer") {
      openedFile = null;
      openedDiff = null;
      void tick().then(() => { composerFocusToken++; });
    }
    else sidebarVisible = !sidebarVisible;
  }

  function handleShortcut(event: KeyboardEvent) {
    if (startupPending || startupFailure) return;
    const decision = hostShortcutDecision(event, shortcutContext(), menuOpen);
    if (decision.consume) { event.preventDefault(); event.stopPropagation(); }
    if (decision.command) runHostCommand(decision.command);
  }

  function shortcutMenu(command: HostCommand) {
    return {
      shortcut: shortcutLabel(command), shortcutAria: shortcutAria(command),
      disabled: !hostCommandEnabled(command, shortcutContext()), action: () => runHostCommand(command),
    };
  }

  const menus = $derived.by(() => [
    { label: "文件", items: [
      { label: "打开项目目录…", action: () => void addProject() },
      { label: "搜索文件", ...shortcutMenu("files") },
      { label: "恢复副本…", disabled: !selectedProjectId || recoveryBusy, action: () => {
        const project = projects.find((candidate) => candidate.id === selectedProjectId);
        if (!project || recoveryBusy || closingWindow) return;
        recoveryModule ??= import("$lib/FileRecoveryPanel.svelte");
        recoveryProject = { id: project.id, name: project.name };
      } },
      { label: "保存文件", ...shortcutMenu("save") },
      { label: "文件另存为…", ...shortcutMenu("saveAs") },
      { label: "新建任务", disabled: !canStart || activeAgent !== "pi", action: () => void startTask() },
      { label: "新建终端任务", disabled: !canStart || activeAgent !== "pi", action: () => void startTask(selectedProjectId, "tui") },
    ] },
    { label: "编辑", items: [
      { label: "搜索任务", ...shortcutMenu("tasks") },
      { label: "重命名当前任务…", disabled: !activeTask || activeAgent !== "pi", action: () => { if (activeTask) void renameTask(activeTask); } },
      { label: "聚焦对话输入", ...shortcutMenu("composer") },
    ] },
    { label: "视图", items: [
      { label: "切换到对话模式", disabled: !activeTask || activeAgent !== "pi" || activeTask.interactionMode === "rpc" || switchingTasks.has(activeTask.id),
        action: () => { if (activeTask) void switchTaskMode(activeTask, "rpc"); } },
      { label: "切换到终端兼容模式", disabled: !activeTask || activeAgent !== "pi" || activeTask.interactionMode !== "rpc" || switchingTasks.has(activeTask.id),
        action: () => { if (activeTask) void switchTaskMode(activeTask, "tui"); } },
      { label: sidebarVisible ? "隐藏项目侧栏" : "显示项目侧栏", ...shortcutMenu("sidebar") },
      { label: filesVisible ? "隐藏文件栏" : "显示文件栏", disabled: activeAgent !== "pi" || view !== "workspace", action: () => { filesVisible = !filesVisible; } },
      { label: gitVisible ? "隐藏 Git 变更栏" : "显示 Git 变更栏", disabled: activeAgent !== "pi" || view !== "workspace", action: () => { gitVisible = !gitVisible; } },
      { label: "Pi 工作区", action: () => void showPi() },
      { label: "DSH 工作区", action: () => void showDsh() },
      { label: "单任务布局", disabled: activeAgent !== "pi", action: () => changeLayout("single") },
      { label: "双列布局", disabled: activeAgent !== "pi", action: () => changeLayout("split") },
      { label: "网格布局", disabled: activeAgent !== "pi", action: () => changeLayout("grid") },
    ] },
    { label: "设置", items: [
      { label: "应用设置", ...shortcutMenu("settings") },
      { label: "外观", action: () => openSettingsCategory("appearance") },
      { label: "模型与凭据", action: () => openSettingsCategory("models") },
      { label: "运行时与更新", action: () => openSettingsCategory("runtime") },
      { label: "Pi 扩展", action: () => openSettingsCategory("extensions") },
      { label: "MCP 服务", action: () => openSettingsCategory("mcp") },
      { label: "Skills 技能", action: () => openSettingsCategory("skills") },
      { label: "高级与诊断", action: () => openSettingsCategory("advanced") },
    ] },
    { label: "帮助", items: [
      { label: "检查应用更新", action: () => { openSettingsCategory("runtime"); void checkDeepPiUpdate(); } },
      { label: "关于 DeepPi", action: () => { void dialogs.request({ kind: "alert", title: "DeepPi", message: "Pi Coding Agent 与 DeepSeek Harness 桌面工作台。", confirmLabel: "关闭" }); } },
    ] },
  ]);

  const piTasks = $derived(tasks.filter((task) => task.agent === "pi"));
  const selectedProject = $derived(
    projects.find((project) => project.id === selectedProjectId),
  );
  const runningCount = $derived(
    piTasks.filter((task) => ["running", "waiting"].includes(task.status)).length,
  );
  const allTerminalTasks = $derived(
    terminalTaskIds
      .map((id) => tasks.find((task) => task.id === id))
      .filter((task): task is Task => task !== undefined),
  );
  const terminalTasks = $derived(
    allTerminalTasks.filter((task) => task.projectId === selectedProjectId),
  );
  const projectTerminalIds = $derived(terminalTasks.map((task) => task.id));
  const paneCapacity = $derived(layout === "single" ? 1 : layout === "split" ? 2 : 4);
  const canStart = $derived(
    !startupPending && !startupFailure && !isStarting && !closingWindow &&
      runningCount < settings.maxConcurrentTasks &&
      selectedProjectId !== null,
  );

  function applyAppearance(next: AppSettings) {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    root.dataset.colorMode = next.colorMode;
    root.dataset.colorScheme = isLightColorMode(next.colorMode) ? "light" : "dark";
    root.dataset.theme = next.theme;
    root.style.setProperty("--app-font", cssFontFamily(next.appFont));
    root.style.setProperty("--text-font", cssFontFamily(next.textFont));
    root.style.setProperty("--code-font", cssFontFamily(next.codeFont));
  }

  $effect(() => {
    applyAppearance(settings);
  });

  async function checkUpdates(force = false) {
    if (isCheckingUpdates) return;
    isCheckingUpdates = true;
    try {
      if (force) await invoke("clear_runtime_update_cache");
      updates = await invoke<RuntimeUpdate[]>("check_runtime_updates");
    } catch (error) {
      showError(error);
    } finally {
      isCheckingUpdates = false;
    }
  }

  async function checkDeepPiUpdate() {
    if (appUpdate.status === "checking" || appUpdate.status === "installing") return;
    appUpdate = { status: "checking", version: null, notes: null, error: null };
    try {
      pendingAppUpdate = await checkAppUpdate();
      appUpdate = pendingAppUpdate
        ? {
            status: "available",
            version: pendingAppUpdate.version,
            notes: pendingAppUpdate.body ?? null,
            error: null,
          }
        : { status: "current", version: null, notes: null, error: null };
    } catch (error) {
      pendingAppUpdate = null;
      appUpdate = {
        status: "error",
        version: null,
        notes: null,
        error: String(error),
      };
    }
  }

  async function installDeepPiUpdate() {
    if (!pendingAppUpdate) return;
    appUpdate = { ...appUpdate, status: "installing", error: null };
    try {
      await installAppUpdate(pendingAppUpdate);
    } catch (error) {
      appUpdate = { ...appUpdate, status: "error", error: String(error) };
    }
  }

  function snoozeRuntime(update: RuntimeUpdate) {
    if (!update.latestVersion) return;
    updateSettings({
      ...settings,
      snoozedUpdates: {
        ...settings.snoozedUpdates,
        [update.id]: Date.now() + SNOOZE_DURATION_MS,
      },
    });
  }

  function skipRuntime(update: RuntimeUpdate) {
    if (!update.latestVersion) return;
    updateSettings({
      ...settings,
      skippedUpdates: {
        ...settings.skippedUpdates,
        [update.id]: update.latestVersion,
      },
    });
  }

  async function installRuntime(update: RuntimeUpdate) {
    if (!update.latestVersion || !update.installable) return;
    await changeRuntime(update, false);
  }

  async function rollbackRuntime(update: RuntimeUpdate) {
    if (!update.canRollback) return;
    await changeRuntime(update, true);
  }

  async function cancelRuntime() {
    try {
      if (!(await runtimeRunner.cancel())) showError("当前操作尚未接受取消，或已进入提交阶段。");
    } catch (error) {
      showError(error);
    }
  }

  async function changeRuntime(update: RuntimeUpdate, rollback: boolean) {
    if (busyRuntime) return;
    busyRuntime = update.id;
    let stoppedDsh = false;
    try {
      const confirmed = rollback
        ? await confirmDialog("回滚组件", `恢复 ${update.name} 的上一版本吗？`, "回滚")
        : await confirmDialog("更新组件", `下载并激活 ${update.name} ${update.latestVersion} 吗？当前运行时会保留。`, "更新");
      if (!confirmed) return;
      const affectsDsh = update.id === "dsh" || update.id === "dshmarket";
      if (affectsDsh) {
        await dshWebview?.close();
        dshWebview = null;
        await invoke("stop_dsh");
        stoppedDsh = true;
      }
      await runtimeRunner.run(rollback ? "rollback_runtime" : "install_runtime", {
        componentId: update.id,
        ...(rollback ? {} : { version: update.latestVersion }),
      });
      runtimes = await invoke<RuntimeComponent[]>("runtime_status");
      await checkUpdates(true);
    } catch (error) {
      showError(error);
    } finally {
      if (stoppedDsh) {
        try { await invoke("start_dsh"); } catch (error) { showError(error); }
      }
      busyRuntime = null;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleShortcut, true);
    if (!isTauri()) {
      startupPending = false;
      return () => {
        window.removeEventListener("keydown", handleShortcut, true);
        dialogs.dispose();
      };
    }
    let disposed = false;
    pageDisposed = false;
    let nativeReady = false;
    let updateTimer: ReturnType<typeof setTimeout> | undefined;
    const initialization = invoke<void>("await_startup").then(() => { nativeReady = true; });
    const startup = loadAppStartup({
      settings: async () => { await initialization; return invoke<AppSettings>("get_settings"); },
      tasks: async () => { await initialization; return invoke<Task[]>("list_tasks"); },
      projects: async () => { await initialization; return invoke<Project[]>("list_projects"); },
      runtimes: async () => { await initialization; return invoke<RuntimeComponent[]>("runtime_status"); },
      applySettings: (value) => { settings = value; },
      applyRuntimes: (value) => { runtimes = value; },
      applyWorkspace: (storedTasks, storedProjects, storedSettings) => {
        tasks = storedTasks;
        projects = storedProjects;
        terminalTaskIds = storedTasks.filter((task) => task.agent === "pi" && task.runId).map((task) => task.id);
        selectedProjectId =
          storedProjects.find((project) => project.path === storedSettings.lastProject)?.id ??
          storedProjects[0]?.id ??
          null;
      },
      error: (scope, error) => {
        if (scope === "workspace") startupFailure = String(error);
        else errorMessage = String(error);
      },
    });
    void startup.ready.then((ready) => {
      if (disposed) return;
      startupPending = false;
      if (ready) {
        // Network update checks are optional background work, not a startup dependency.
        updateTimer = setTimeout(() => {
          void checkUpdates();
          void checkDeepPiUpdate();
        }, 30_000);
      }
    });

    const systemTheme = window.matchMedia("(prefers-color-scheme: light)");
    const handleSystemThemeChange = () => {
      if (settings.colorMode === "system") applyAppearance(settings);
    };
    systemTheme.addEventListener("change", handleSystemThemeChange);

    const appWindow = getCurrentWindow();
    const closeListener = appWindow.onCloseRequested(createWindowCloseHandler({
      behavior: () => nativeReady ? settings.closeBehavior : "exit",
      blocked: () => diagnosticsBusy || recoveryBusy || fileWorkspace.busy() || switchingTasks.size > 0 || gitIndexBusy || settingsPackageBusy || busyRuntime !== null,
      blockedReason: () => diagnosticsBusy ? "诊断操作正在处理，请完成或取消后再关闭窗口" : recoveryBusy || fileWorkspace.busy() ? "项目文件正在处理，请完成后再关闭窗口" : settingsPackageBusy || busyRuntime !== null ? "组件操作正在处理，请完成或取消后再关闭窗口" : gitIndexBusy ? "Git 写入正在处理，请完成后再关闭窗口" : "任务正在切换模式，请完成后再关闭窗口",
      hasActiveTasks: () => tasks.some((task) => ["running", "waiting"].includes(task.status)),
      choose: closeChoiceDialog,
      confirm: () => confirmDialog("退出 DeepPi", "仍有活动任务，停止任务并退出吗？"),
      remember: (behavior) => updateSettings({ ...settings, closeBehavior: behavior }),
      minimize: () => appWindow.minimize(),
      prepareExit: async () => {
        await settingsSaver.flush();
        const release = await fileWorkspace.prepareExit();
        if (!release) return false;
        closingWindow = true;
        return () => { closingWindow = false; release(); };
      },
      stopPi: () => nativeReady ? invoke("stop_all_pi_tasks") : Promise.resolve(),
      stopDsh: () => nativeReady ? invoke("stop_dsh") : Promise.resolve(),
      destroy: () => appWindow.destroy(),
      error: showError,
    }));
    const dshTaskListener = listen<Task[]>("dsh-tasks", ({ payload }) => {
      tasks = [...tasks.filter((task) => task.agent !== "dsh"), ...payload];
    });
    const dshStatusListener = listen<DshHostStatus>("dsh-status", ({ payload }) => {
      if (payload.running) {
        dshHostError = null;
        return;
      }
      if (payload.error) dshHostError = payload.error;
      void dshWebview?.close();
      dshWebview = null;
    });
    const statusListener = listen<TaskStatusUpdate>("task-status", ({ payload }) => {
      const task = tasks.find((candidate) => candidate.id === payload.taskId);
      if (task && task.runId === payload.runId && ["running", "waiting"].includes(task.status)) {
        if (task.status === "running" && payload.status !== "running" && task.projectId === selectedProjectId) filesRefreshToken++;
        task.status = payload.status;
      }
    });
    const rpcExitListener = listen<TaskStatusUpdate>("rpc-task-exit", ({ payload }) => {
      const task = tasks.find((candidate) => candidate.id === payload.taskId);
      if (task?.runId === payload.runId) {
        task.status = payload.status;
        dialogs.cancelScope(`rpc:${task.id}:${task.runId}`);
      }
    });
    const runtimeProgressListener = listen<{ operationId: string; phase: string; percent: number | null }>(
      "runtime-progress",
      ({ payload }) => {
        if (runtimeOperation?.id !== payload.operationId) return;
        runtimeProgress = { phase: payload.phase, percent: payload.percent ?? null };
      },
    );
    const observer = new ResizeObserver(() => void syncDshBounds());
    observer.observe(workspace);

    return () => {
      disposed = true;
      pageDisposed = true;
      startup.dispose();
      clearTimeout(updateTimer);
      window.removeEventListener("keydown", handleShortcut, true);
      observer.disconnect();
      systemTheme.removeEventListener("change", handleSystemThemeChange);
      void closeListener.then((unlisten) => unlisten());
      void dshTaskListener.then((unlisten) => unlisten());
      void dshStatusListener.then((unlisten) => unlisten());
      void statusListener.then((unlisten) => unlisten());
      void rpcExitListener.then((unlisten) => unlisten());
      void runtimeProgressListener.then((unlisten) => unlisten());
      dialogs.dispose();
      void dshWebview?.close();
    };
  });

  function showError(error: unknown) {
    errorMessage = String(error);
    void dialogs.request({
      kind: "alert",
      title: "操作失败",
      message: errorMessage,
      confirmLabel: "知道了",
    });
  }

  function resolveDialog(value: DialogValue) {
    dialogs.resolve(value);
  }

  function confirmDialog(title: string, message: string, confirmLabel = "确认") {
    return dialogs.request({ kind: "confirm", title, message, confirmLabel })
      .then((value) => value === true);
  }

  function closeChoiceDialog() {
    return dialogs.request({
      kind: "choice",
      title: "关闭 DeepPi",
      message: "选择点击窗口关闭按钮时的默认行为。这个选择会记住，也可以在设置中修改。",
      choices: [
        { value: "minimize", label: "最小化" },
        { value: "exit", label: "退出应用" },
      ],
    }).then((value): CloseBehavior | null => value === "minimize" || value === "exit" ? value : null);
  }

  function updateSettings(next: AppSettings) {
    settings = next;
    settingsSaver.enqueue(next);
  }

  function inputDialog(title: string, message: string, initialValue: string) {
    return dialogs.request({
      kind: "input",
      title,
      message,
      initialValue,
      placeholder: "输入任务名称",
      confirmLabel: "保存",
    }).then((value) => typeof value === "string" ? value.trim() : null);
  }

  function applyPaneSelection(selection: { panes: string[]; active: string | null }) {
    paneTaskIds = selection.panes;
    activeTaskId = selection.active;
  }

  function selectProject(project: Project) {
    openedFile = null;
    openedDiff = null;
    selectedProjectId = project.id;
    settings = { ...settings, lastProject: project.path };
    void invoke("touch_project", { projectId: project.id }).catch(showError);
    settingsSaver.enqueue(settings);
    applyPaneSelection(
      changePaneCapacity({
        current: paneTaskIds.filter((id) => projectTerminalIds.includes(id)),
        active: activeTaskId,
        available: terminalTaskIds.filter(
          (id) => tasks.find((task) => task.id === id)?.projectId === project.id,
        ),
        capacity: paneCapacity,
      }),
    );
  }

  async function addProject() {
    try {
      const selection = await open({
        directory: true,
        multiple: false,
        title: "选择 Pi 项目目录",
      });
      const path = Array.isArray(selection) ? selection[0] : selection;
      if (!path) return;

      const project = await invoke<Project>("add_project", { path });
      projects = [
        project,
        ...projects.filter((candidate) => candidate.id !== project.id),
      ];
      selectProject(project);
    } catch (error) {
      showError(error);
    }
  }

  async function removeProject(project: Project) {
    if (recoveryBusy) { showError("恢复副本正在处理，请完成后再移除项目。"); return; }
    if (
      !(await confirmDialog(
        "移除项目",
        `从工作区移除“${project.name}”吗？本机目录、任务和会话不会被删除。`,
        "移除",
      ))
    ) {
      return;
    }
    if (recoveryBusy || !await fileWorkspace.prepareClose(project.id)) return;
    if (recoveryBusy) return;
    try {
      await fileWorkspace.removeProject(project.id, () => invoke("remove_project", { projectId: project.id }));
      projects = projects.filter((candidate) => candidate.id !== project.id);
      if (recoveryProject?.id === project.id) recoveryProject = null;
      if (selectedProjectId !== project.id) return;

      const nextProject = projects[0];
      if (nextProject) {
        selectProject(nextProject);
      } else {
        selectedProjectId = null;
        settings = { ...settings, lastProject: null };
        applyPaneSelection({ panes: [], active: null });
        settingsSaver.enqueue(settings);
      }
    } catch (error) {
      showError(error);
    }
  }

  function openTask(task: Task) {
    if (!canLeaveSettings()) return;
    if (task.agent === "dsh") {
      void showDsh();
      return;
    }
    openedFile = null;
    openedDiff = null;
    view = "workspace";
    if (task.projectId) {
      const project = projects.find((candidate) => candidate.id === task.projectId);
      if (project && selectedProjectId !== project.id) selectProject(project);
    }
    if (!terminalTaskIds.includes(task.id)) {
      showError("该任务没有活动终端，请先重启任务");
      return;
    }
    applyPaneSelection(
      openPane({
        current: paneTaskIds,
        active: activeTaskId,
        requested: task.id,
        available: terminalTaskIds.filter(
          (id) => tasks.find((candidate) => candidate.id === id)?.projectId === task.projectId,
        ),
        capacity: paneCapacity,
      }),
    );
  }

  function changeLayout(next: LayoutMode) {
    layout = next;
    applyPaneSelection(
      changePaneCapacity({
        current: paneTaskIds,
        active: activeTaskId,
        available: projectTerminalIds,
        capacity: next === "single" ? 1 : next === "split" ? 2 : 4,
      }),
    );
  }

  function removeTerminal(taskId: string) {
    terminalTaskIds = terminalTaskIds.filter((id) => id !== taskId);
    applyPaneSelection(
      changePaneCapacity({
        current: paneTaskIds.filter((id) => id !== taskId),
        active: activeTaskId === taskId ? null : activeTaskId,
        available: terminalTaskIds,
        capacity: paneCapacity,
      }),
    );
  }

  /** 右键分割：把目标任务放进新窗格（向右/向下），布局自动升级到双列或四格。 */
  function splitTask(task: Task, direction: "right" | "down") {
    if (!terminalTaskIds.includes(task.id)) {
      showError("该任务没有活动终端，请先重启任务");
      return;
    }
    if (layout === "single") {
      layout = "split";
    } else if (layout === "split" && paneTaskIds.length >= 2) {
      layout = "grid";
    }
    applyPaneSelection(
      splitPane({
        current: paneTaskIds,
        active: activeTaskId,
        requested: task.id,
        available: terminalTaskIds.filter(
          (id) => tasks.find((candidate) => candidate.id === id)?.projectId === task.projectId,
        ),
        capacity: paneCapacity,
        direction,
      }),
    );
  }

  function runtimeTitle(id: RuntimeComponent["id"]): string {
    const component = runtimes.find((runtime) => runtime.id === id);
    return component?.currentVersion
      ? `${component.name} ${component.currentVersion}`
      : `${id} 未安装`;
  }

  async function syncDshBounds() {
    if (!dshWebview || activeAgent !== "dsh") return;
    const bounds = workspace.getBoundingClientRect();
    await dshWebview.setPosition(new LogicalPosition(bounds.left, bounds.top));
    await dshWebview.setSize(
      new LogicalSize(Math.max(1, bounds.width), Math.max(1, bounds.height)),
    );
  }

  async function waitForDshWebview(): Promise<Webview> {
    for (let attempt = 0; attempt < 50; attempt += 1) {
      const webview = await Webview.getByLabel("dsh");
      if (webview) return webview;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error("DSH Webview did not become ready");
  }

  async function showDsh() {
    if (!canLeaveSettings()) return;
    if (isDshStarting) return;
    activeAgent = "dsh";
    view = "workspace";
    await tick();
    if (dshWebview) {
      await dshWebview.show();
      await dshWebview.setFocus();
      await syncDshBounds();
      return;
    }
    isDshStarting = true;
    dshHostError = null;
    errorMessage = "";
    try {
      const url = await invoke<string>("start_dsh");
      const bounds = workspace.getBoundingClientRect();
      await invoke("create_dsh_webview", {
        baseUrl: url,
        x: bounds.left,
        y: bounds.top,
        width: Math.max(1, bounds.width),
        height: Math.max(1, bounds.height),
      });
      dshWebview = await waitForDshWebview();
      if (activeAgent !== "dsh" || menuOpen || dialogRequest) await dshWebview.hide();
      else await dshWebview.setFocus();
      await syncDshBounds();
    } catch (error) {
      activeAgent = "pi";
      dshHostError = String(error);
      showError(`DSH 启动失败：${String(error)}`);
    } finally {
      isDshStarting = false;
    }
  }

  async function showPi() {
    if (!canLeaveSettings()) return;
    activeAgent = "pi";
    view = "workspace";
    await dshWebview?.hide();
  }

  function openSettings() {
    activeAgent = "pi";
    void dshWebview?.hide();
    view = "settings";
  }

  function closeSettings() {
    if (!canLeaveSettings()) return;
    view = "workspace";
  }

  function canLeaveSettings() {
    if (view === "settings" && diagnosticsBusy) {
      showError("诊断操作正在处理，请先完成或取消操作");
      return false;
    }
    if (view === "settings" && settingsPackageBusy) {
      showError("扩展操作正在处理，请先完成或取消操作");
      return false;
    }
    return true;
  }

  function openSettingsCategory(category: SettingsCategory) {
    settingsCategory = category;
    openSettings();
  }

  async function startTask(projectId: string | null = selectedProjectId, mode: "rpc" | "tui" = "rpc") {
    const project = projects.find((candidate) => candidate.id === projectId);
    if (!project) {
      showError("请先添加并选择一个 Pi 项目目录");
      return;
    }
    if (isStarting || runningCount >= settings.maxConcurrentTasks) return;
    isStarting = true;
    errorMessage = "";
    try {
      if (selectedProjectId !== project.id) selectProject(project);
      if (
        hasProjectConflict(project.path, piTasks) &&
        !(await confirmDialog(
          "确认并发写入",
          "该项目已有活动任务，继续可能产生文件冲突。仍要创建任务吗？",
        ))
      ) {
        return;
      }
      if (runningCount >= settings.maxConcurrentTasks) return;
      const task = await invoke<Task>(mode === "rpc" ? "start_rpc_task" : "start_pi_task", {
        request: { projectId: project.id, title: "", rows: 32, cols: 100 },
      });
      tasks.unshift(task);
      terminalTaskIds.push(task.id);
      openTask(task);
    } catch (error) {
      showError(error);
    } finally {
      isStarting = false;
    }
  }

  async function stopTask(task: Task) {
    if (modeSwitcher.isBusy(task.id)) return;
    try {
      if (task.interactionMode === "rpc") {
        await invoke("stop_rpc_task", { taskId: task.id, runId: task.runId });
      } else {
        await invoke("stop_pi_run", { taskId: task.id, runId: task.runId });
      }
      task.status = "cancelled";
    } catch (error) {
      showError(error);
    }
  }

  function markExited(task: Task, exitCode: number | null, error: string | null) {
    if (task.status !== "cancelled") {
      task.status = error || exitCode !== 0 ? "failed" : "completed";
    }
  }

  async function renameTask(task: Task) {
    const title = await inputDialog("重命名任务", "修改任务在项目列表中的显示名称。", task.title);
    if (!title || title === task.title) return;
    try {
      await invoke("rename_task", { taskId: task.id, title });
      task.title = title;
    } catch (error) {
      showError(error);
    }
  }

  async function autoRenameTask(task: Task, title: string) {
    if (!title || title === task.title) return;
    try {
      await invoke("rename_task", { taskId: task.id, title });
      task.title = title;
    } catch (error) {
      showError(error);
    }
  }

  async function closeTask(task: Task) {
    if (!modeSwitcher.isBusy(task.id) && ["running", "waiting"].includes(task.status)) {
      try { await stopTask(task); } catch { /* 停止失败时仍然关闭标签页 */ }
    }
    removeTerminal(task.id);
  }

  function requestTaskRestart(task: Task, mode: InteractionMode) {
    return invoke<Task>(mode === "rpc" ? "start_rpc_task" : "restart_pi_task", {
      request: { taskId: task.id, projectId: task.projectId, title: task.title, rows: 32, cols: 100 },
    });
  }

  function applyTaskRestart(task: Task, restarted: Task) {
    Object.assign(task, restarted);
    if (!terminalTaskIds.includes(task.id)) terminalTaskIds.push(task.id);
    openTask(task);
  }

  async function restartTask(task: Task, mode = task.interactionMode ?? "tui") {
    if (modeSwitcher.isBusy(task.id)) return;
    if (task.piEnvironment && task.piEnvironment !== "managed") {
      showError("该任务来自旧版本机 Pi 环境，当前稳定版仅支持 DeepPi 托管任务，本机会话记录已保留。");
      return;
    }
    try {
      applyTaskRestart(task, await requestTaskRestart(task, mode));
    } catch (error) {
      showError(error);
    }
  }

  async function restartAllPiTasks() {
    if (restartBusy || busyRuntime) return;
    const candidates = tasks.filter((task) => task.agent === "pi" && ["running", "waiting"].includes(task.status));
    if (candidates.length === 0) return;
    const confirmed = await confirmDialog(
      "重启 Pi 任务",
      `重启 ${candidates.length} 个运行中的 Pi 任务吗？会话内容保留，正在切换中的任务将被跳过。`,
      "重启",
    );
    if (!confirmed) return;
    restartBusy = "pi";
    try {
      for (const task of candidates) {
        if (modeSwitcher.isBusy(task.id)) continue;
        await restartTask(task);
      }
    } finally {
      restartBusy = null;
    }
  }

  async function restartDsh() {
    if (restartBusy || busyRuntime || isDshStarting) return;
    restartBusy = "dsh";
    try {
      await dshWebview?.close();
      dshWebview = null;
      await invoke("stop_dsh");
    } catch (error) {
      showError(error);
    }
    restartBusy = null;
    await showDsh();
  }

  async function switchTaskMode(task: Task, mode: InteractionMode) {
    try {
      const restarted = await modeSwitcher.switch(task, mode);
      if (restarted) applyTaskRestart(task, restarted);
    } catch (error) { showError(error); }
  }

  async function archiveTask(task: Task) {
    if (modeSwitcher.isBusy(task.id)) return;
    try {
      await invoke("archive_task", { taskId: task.id });
      task.archivedAt = Date.now();
      removeTerminal(task.id);
    } catch (error) {
      showError(error);
    }
  }

  async function restoreTask(task: Task) {
    try {
      await invoke("restore_task", { taskId: task.id });
      task.archivedAt = null;
    } catch (error) {
      showError(error);
    }
  }

  async function deleteTask(task: Task) {
    if (modeSwitcher.isBusy(task.id)) return;
    if (!(await confirmDialog("删除任务", `永久删除“${task.title}”的 DeepPi 任务记录吗？`))) return;
    try {
      if (task.agent === "pi" && ["queued", "running", "waiting"].includes(task.status)) {
        await stopTask(task);
      }
      await invoke("delete_task", { taskId: task.id });
      tasks = tasks.filter((candidate) => candidate.id !== task.id);
      removeTerminal(task.id);
    } catch (error) {
      showError(error);
    }
  }
</script>

<svelte:head><title>DeepPi</title></svelte:head>

{#if startupPending || startupFailure}
  <section class="startup-status" aria-label="工作区启动" aria-live="polite">
    {#if startupFailure}
      <p role="alert">{startupFailure}</p>
      <button type="button" onclick={() => window.location.reload()}><RotateCcw size={16} />重新加载</button>
    {:else}
      <p role="status">正在加载工作区…</p>
    {/if}
  </section>
{/if}
<div class:dsh-mode={activeAgent === "dsh"} class:sidebar-hidden={!showSidebar} class:files-hidden={!showFiles} class:git-visible={showGit} class="app-shell" inert={closingWindow || startupPending || !!startupFailure} aria-busy={closingWindow || startupPending}>
  <header class="topbar">
    <div class="brand"><Bot size={18} strokeWidth={1.8} /><strong>DeepPi</strong></div>

    <AppMenu {menus} onOpenChange={(open) => { menuOpen = open; }} />

    {#if activeAgent === "pi"}
      <div class="toolbar">
        <button class:active={showSidebar} type="button" aria-label="切换工作区侧栏" title={showSidebar ? "折叠工作区" : "展开工作区"}
          aria-pressed={showSidebar} onclick={() => { if (!canLeaveSettings()) return; view = "workspace"; sidebarVisible = !sidebarVisible; }}><PanelLeft size={16} /></button>
        <button class:active={showFiles} type="button" aria-label="切换文件栏" title={showFiles ? "折叠文件栏" : "展开文件栏"}
          aria-pressed={showFiles} onclick={() => { if (!canLeaveSettings()) return; view = "workspace"; filesVisible = !filesVisible; }}><PanelRight size={16} /></button>
      </div>
    {/if}
  </header>

  <nav class="activity-rail" aria-label="工作区导航">
    <button type="button" class:active={activeAgent === "pi" && view === "workspace"} aria-label="Pi 工作区" title={runtimeTitle("pi")} onclick={showPi}><Bot size={21} /><span>Pi</span></button>
    <button type="button" class:active={activeAgent === "dsh"} aria-label="DSH 工作区" title={runtimeTitle("dsh")} onclick={showDsh}><Globe size={21} /><span>DSH</span></button>
    <button type="button" class="rail-settings" class:active={view !== "workspace"} aria-label="设置" aria-keyshortcuts={shortcutAria("settings")} title={`设置 (${shortcutLabel("settings")})`} onclick={openSettings}><Settings2 size={21} /></button>
  </nav>
  <aside class="project-sidebar" class:panel-hidden={!showSidebar} aria-label="项目侧栏">
    <TaskSidebar
      {projects}
      tasks={piTasks}
      selectedProjectId={selectedProjectId}
      onAddProject={addProject}
      onOpenProject={selectProject}
      onOpen={openTask}
      onRename={renameTask}
      onStop={stopTask}
      onArchive={archiveTask}
      onRestart={restartTask}
      onRestore={restoreTask}
      onDelete={deleteTask}
      onAddSession={(project) => void startTask(project.id)}
      onRemoveProject={removeProject}
      searchFocusToken={showSidebar ? searchFocusToken : 0}
    />
  </aside>

  <main class="workspace" class:settings-view={view === "settings"} bind:this={workspace}>
    {#if activeAgent === "dsh"}
      <div class="dsh-placeholder" aria-live="polite">
        {#if isDshStarting}
          <span>正在启动 DSH</span>
        {:else if dshHostError}
          <p class="dsh-error" role="alert">
            <strong>DSH 启动失败</strong>
            <span>{dshHostError}</span>
            {#if /plugin tree failed to load|does not provide an export/.test(dshHostError)}
              <small>提示：某个 DSH 插件与当前 DSH 版本不兼容。请在 DSH 的插件市场里禁用或卸载刚安装的插件后重启。</small>
            {/if}
          </p>
          <button type="button" class="start-button" onclick={showDsh} disabled={isDshStarting}>
            <RotateCcw size={16} />重启 DSH
          </button>
        {/if}
      </div>
    {:else if view === "settings"}
      <PiSettings
        saving={settingsSaving}
        closeBlocked={settingsPackageBusy || diagnosticsBusy}
        onDiagnosticsBusy={(busy) => { diagnosticsBusy = busy; }}
        confirmDiagnosticsClear={() => confirmDialog("清空诊断记录", "清空本次应用运行的 Pi RPC 诊断记录？不会删除任务和会话。")}
        category={settingsCategory}
        onCategoryChange={(category) => { settingsCategory = category; }}
        settings={settings}
        runtimes={runtimes}
        updates={updates}
        isCheckingUpdates={isCheckingUpdates}
        {busyRuntime}
        {runtimeOperation}
        {runtimeProgress}
        onCancelRuntime={() => void cancelRuntime()}
        {appUpdate}
        onChangeSettings={updateSettings}
        onEditorSaved={(externalEditor) => { settings = { ...settings, externalEditor }; }}
        onCheckUpdates={() => void checkUpdates(true)}
        onCheckAppUpdate={() => void checkDeepPiUpdate()}
        onInstallAppUpdate={() => void installDeepPiUpdate()}
        onUpdateRuntime={(update) => void installRuntime(update)}
        onRollbackRuntime={(update) => void rollbackRuntime(update)}
        onSnoozeRuntime={(update) => snoozeRuntime(update)}
        onSkipRuntime={(update) => skipRuntime(update)}
        onRestartPi={() => void restartAllPiTasks()}
        onRestartDsh={() => void restartDsh()}
        {restartBusy}
        {runningPiCount}
        {dshRunning}
        onClose={closeSettings}
      >
        {#snippet models()}
          <PiProviderSettings embedded confirm={confirmDialog} onClose={closeSettings} onError={showError} />
        {/snippet}
        {#snippet extensions()}
          <PiMarketplace embedded projectPath={selectedProject?.path ?? null} confirm={confirmDialog} onClose={closeSettings} onError={showError} onBusyChange={(busy) => { settingsPackageBusy = busy; }} />
        {/snippet}
        {#snippet mcp()}
          <PiMcpSkillsSettings mode="mcp" confirm={confirmDialog} onError={showError} projectPath={selectedProject?.path ?? null} />
        {/snippet}
        {#snippet skills()}
          <PiMcpSkillsSettings mode="skills" confirm={confirmDialog} onError={showError} projectPath={selectedProject?.path ?? null} />
        {/snippet}
      </PiSettings>
    {:else if !selectedProject}
      <div class="empty-state">
        <Bot size={32} strokeWidth={1.4} />
        <h1>工作区</h1>
        <button type="button" class="start-button" onclick={addProject}>
          <FolderPlus size={16} />添加项目目录
        </button>
      </div>
    {:else if terminalTasks.length === 0}
      <div class="empty-state">
        <Bot size={32} strokeWidth={1.4} />
        <h1>{selectedProject.name}</h1>
        <button type="button" class="start-button" onclick={() => void startTask()} disabled={!canStart}>
          <Plus size={16} />新建任务
        </button>
      </div>
    {:else}
      <TaskTabs
        tasks={terminalTasks}
        {activeTaskId}
        onOpen={openTask}
        onRename={renameTask}
        onStop={stopTask}
        onArchive={archiveTask}
        onRestart={restartTask}
        onRestore={restoreTask}
        onDelete={deleteTask}
        onClose={closeTask}
        onAdd={() => void startTask()}
        onSplit={splitTask}
      />
    {/if}
      <div class="workspace-content" class:panel-hidden={activeAgent !== "pi" || view !== "workspace" || terminalTasks.length === 0 || !selectedProject}>
        <div
          class:single={layout === "single"}
          class:split={layout === "split"}
          class:grid={layout === "grid"}
          class:pane-count-one={paneTaskIds.length === 1}
          class="terminal-grid"
        >
          {#each allTerminalTasks as task (task.id)}
            {#if task.interactionMode === "rpc"}
              <ChatPane
                taskId={task.id} runId={task.runId} title={task.title}
                autoName={/^(Pi Task \d+|Session [0-9a-f]{8})$/.test(task.title)}
                onAutoRename={(title) => void autoRenameTask(task, title)}
                switching={switchingTasks.has(task.id)}
                visible={activeAgent === "pi" && view === "workspace" && !inspectingFile && task.projectId === selectedProjectId && paneTaskIds.includes(task.id)}
                active={task.id === activeTaskId}
                focusToken={composerFocusToken}
                onUseTerminal={() => void switchTaskMode(task, "tui")}
                onDialog={(request) => dialogs.request(request)}
                onCancelDialogs={(scope) => dialogs.cancelScope(scope)}
                onActivity={(busy) => {
                  if (task.status === "running" && !busy && task.projectId === selectedProjectId) filesRefreshToken++;
                  if (["running", "waiting"].includes(task.status)) task.status = busy ? "running" : "waiting";
                }}
              />
            {:else}
            {#if terminalModule}
            {#await terminalModule}
              <section class="terminal-pane" class:panel-hidden={activeAgent !== "pi" || view !== "workspace" || inspectingFile || task.projectId !== selectedProjectId || !paneTaskIds.includes(task.id)}>
                <p role="status">正在加载终端…</p>
              </section>
            {:then terminal}
            <terminal.default
              transitioning={switchingTasks.has(task.id)}
              runId={task.runId}
              taskId={task.id}
              title={task.title}
              status={task.status}
              codeFont={settings.codeFont}
              colorMode={settings.colorMode}
              visible={activeAgent === "pi" && view === "workspace" && !inspectingFile && task.projectId === selectedProjectId && paneTaskIds.includes(task.id)}
              active={task.id === activeTaskId}
              onExit={(exitCode, error, exitedRunId) => { if (task.runId === exitedRunId) markExited(task, exitCode, error); }}
              onUseConversation={() => void switchTaskMode(task, "rpc")}
            />
            {:catch}
              <section class="terminal-pane" class:panel-hidden={activeAgent !== "pi" || view !== "workspace" || inspectingFile || task.projectId !== selectedProjectId || !paneTaskIds.includes(task.id)}>
                <p role="alert">终端组件加载失败</p>
                <button type="button" title="重新加载终端组件" aria-label="重新加载终端组件" onclick={loadTerminalModule}><RotateCcw size={16} /></button>
              </section>
            {/await}
            {/if}
            {/if}
          {/each}
        </div>
      </div>
    {#if openedFile && editorModule}
      {#await editorModule then module}
        <module.default bind:this={fileEditor} documents={fileDocuments} controller={fileWorkspace}
          projectId={openedFile.projectId} path={openedFile.path} line={openedFile.line} column={openedFile.column}
          visible={openedFile.projectId === selectedProjectId && activeAgent === "pi" && view === "workspace"}
          editorConfigured={!!settings.externalEditor} refreshToken={filesRefreshToken}
          onConfigureEditor={() => openSettingsCategory("advanced")}
          onSelect={(path) => openFile(path)} onHide={() => { openedFile = null; }}
          requestDialog={(request) => dialogs.request(request)} />
      {:catch error}
        <p role="alert">编辑器加载失败：{String(error)}</p>
      {/await}
    {/if}
    {#if openedDiff && openedDiff.projectId === selectedProjectId && activeAgent === "pi" && view === "workspace"}
      <GitDiffView selection={openedDiff} refreshToken={filesRefreshToken + gitWatchRefreshToken} onClose={() => { openedDiff = null; }} />
    {/if}
    {#if recoveryProject && recoveryModule}
      {#await recoveryModule then module}
        {#key recoveryProject.id}
          <module.default projectId={recoveryProject.id} projectName={recoveryProject.name}
            onClose={() => { if (!recoveryBusy) recoveryProject = null; }}
            onBusyChange={(busy) => { recoveryBusy = busy; }}
            canOperate={() => !closingWindow && !fileSaving && !fileWorkspace.busy() && !gitIndexBusy}
            onChanged={(projectId) => { if (projectId === selectedProjectId) filesRefreshToken++; }}
            requestDialog={(request) => dialogs.request(request)} />
        {/key}
      {:catch error}
        <section class="recovery-error" role="alert">
          <p>恢复副本界面加载失败：{String(error)}</p>
          <button onclick={() => { recoveryProject = null; recoveryModule = null; }}>关闭</button>
        </section>
      {/await}
    {/if}
  </main>
  <aside class="file-panel" class:panel-hidden={!showFiles} aria-label="文件侧栏">
    <div class="file-project-selector">
      <select aria-label="文件所属项目" value={selectedProjectId ?? ""}
        onchange={(event) => { const project = projects.find((item) => item.id === event.currentTarget.value); if (project) selectProject(project); }}>
        {#if !projects.length}<option value="">未选择项目</option>{/if}
        {#each projects as project (project.id)}<option value={project.id}>{project.name}</option>{/each}
      </select>
      <button type="button" aria-label="添加项目目录" title="添加项目目录" onclick={addProject}><FolderPlus size={16} /></button>
    </div>
    {#if fileSidebarVisited}
      <FileSidebar project={selectedProject} onOpen={openFile}
        visible={showFiles} refreshToken={filesRefreshToken}
        {watchError} onRetryWatch={() => { watchRetry++; }}
        searchFocusToken={showFiles ? fileSearchFocusToken : 0} />
    {/if}
  </aside>
  <GitSidebar projectId={selectedProjectId}
    visible={showGit} refreshToken={filesRefreshToken + gitWatchRefreshToken}
    selected={openedDiff?.projectId === selectedProjectId ? openedDiff : null} onOpen={openGitDiff}
    onBusyChange={(busy) => { gitIndexBusy = busy; }}
    onChanged={(projectId, path) => {
      if (openedDiff?.projectId === projectId && (!path || openedDiff.path === path)) openedDiff = null;
      if (selectedProjectId === projectId) filesRefreshToken++;
    }}
    onClose={() => { gitVisible = false; }} />

  <AppDialog request={dialogRequest} onResolve={resolveDialog} />
</div>

<style>
  .recovery-error { position: absolute; inset: 0; z-index: 15; padding: 16px; background: var(--page-bg); overflow: auto; }
  .recovery-error p { overflow-wrap: anywhere; }
</style>
