<script lang="ts">
  import { createDialogQueue } from "$lib/dialog";
  import { createWindowCloseHandler } from "$lib/window-close";
  import { loadAppStartup } from "$lib/startup";
  import { runBoundedStep } from "$lib/bounded-step";
  // Pi 启动请求的时间上限；见 restartTask 中关于 taskControlBusy 永久持有的说明。
  const PI_START_TIMEOUT_MS = 60_000;
  let gitIndexBusy = $state(false);
  import { createOperationRunner, type OperationState } from "$lib/operation";
  import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
  import { createProjectWatch, type ProjectWatchEvent } from "$lib/project-watch";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
  import { Webview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    Bot,
    ClipboardList,
    Terminal,
    FolderPlus,
    Kanban,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
    Plus,
    RotateCcw,
    Wrench,
    Settings2,
    Globe,
    Download,
    X,
    GitBranch,
  } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import FileSidebar from "$lib/FileSidebar.svelte";
  import { createFileWorkspace, type FileDocument, type FileSaveResult } from "$lib/file-workspace";
  import { pathContainsEntry, type FileEntry, type FilePreview, type FileTreeAction } from "$lib/files";
  import GitSidebar from "$lib/GitSidebar.svelte";
  import GitDiffView from "$lib/GitDiffView.svelte";
  import type { GitDiffSelection } from "$lib/git-diff";
  import type { GitDiffArea } from "$lib/git-status";
  import ChatPane from "$lib/ChatPane.svelte";
  import ShellPane from "$lib/ShellPane.svelte";
  import { hostCommandEnabled, hostShortcutDecision, shortcutLabel, shortcutAria, type HostCommand, type HostShortcutContext } from "$lib/shortcuts";
  import { createTaskModeSwitcher, type InteractionMode } from "$lib/task-mode";
  import { SplitSquareHorizontal, SplitSquareVertical } from "@lucide/svelte";
  import AppDialog from "$lib/AppDialog.svelte";
  import AppToasts from "$lib/AppToasts.svelte";
  import AppUpdateProgressBar from "$lib/AppUpdateProgressBar.svelte";
  import { notices } from "$lib/notices.svelte";
  import { checkAppUpdate, describeUpdateError, installAppUpdate, type AppUpdateProgress, type AppUpdateState, type Update } from "$lib/app-update";
  import type { DialogRequest, DialogValue } from "$lib/dialog";
  import type { Project } from "$lib/project";
  import PiMarketplace from "$lib/PiMarketplace.svelte";
  import PiCodemodeSettings from "$lib/PiCodemodeSettings.svelte";
  import PiMcpSkillsSettings from "$lib/PiMcpSkillsSettings.svelte";
  import PiProviderSettings from "$lib/PiProviderSettings.svelte";
  import { runSensitiveHostSlash, type SensitiveHostSlash } from "$lib/pi-host-slash";
  import { notifyModelsChanged } from "$lib/model-config-sync";
  import PiSettings from "$lib/PiSettings.svelte";
  import DshSettings from "$lib/DshSettings.svelte";
  import DshFailure from "$lib/DshFailure.svelte";
  import type { SettingsCategory } from "$lib/settings-navigation";
  import { createSettingsSaver } from "$lib/settings-save";
  import TaskSidebar from "$lib/TaskSidebar.svelte";
  import TaskTabs from "$lib/TaskTabs.svelte";
  import type { RuntimeComponent, RuntimeUpdate } from "$lib/runtime";
  import {
    DEFAULT_APP_SETTINGS,
    type AppSettings,
    type CloseBehavior,
  } from "$lib/settings";
  import { applyAppearance } from "$lib/appearance";
  import { resolveTheme } from "$lib/theme";
  import { t, tm } from "$lib/i18n.svelte";
  import { BOARD_ACTION_EVENT, type BoardActionRequest } from "$lib/board-actions";
  import { createPetTaskActionHandler, isPetTaskRequest, PET_TASK_ACTION_EVENT, PET_TASK_RESULT_EVENT } from "$lib/pet-task-actions";
  import { createTaskExitNotifier } from "$lib/task-notifications";
  import {
    isPermissionGranted,
    requestPermission,
    sendNotification,
  } from "@tauri-apps/plugin-notification";
  import TaskBoard from "$lib/TaskBoard.svelte";
  import type { Task, TaskStatus } from "$lib/task";
  import {
    changePaneCapacity,
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
  /// 应用更新进度：下载阶段是真实百分比，安装阶段是按时间估算的百分比。
  let appUpdateProgress = $state<AppUpdateProgress | null>(null);
  /// 用户手动关掉更新浮层后不再重复弹出（下次检查或有进度时重置）。
  let appUpdateDismissed = $state(false);
  let settings = $state<AppSettings>({ ...DEFAULT_APP_SETTINGS });
  const activeTheme = $derived(resolveTheme(settings.theme, settings.customThemes ?? []));
  const effectiveSessionFontSize = $derived(settings.sessionFontSize ?? activeTheme.typography?.sessionFontSize ?? 13);
  /// 任务完成系统通知：后台（最小化/失焦）也弹系统级提示，文案按当前语言渲染。
  const taskNotifier = createTaskExitNotifier({
    fetchTasks: () => invoke<Task[]>("list_tasks"),
    isEnabled: () => settings.notifyOnTaskComplete,
    bodyFor: (status) => t(status === "completed" ? "任务已完成" : "任务失败"),
    send: async (input) => {
      let granted = await isPermissionGranted();
      if (!granted) granted = (await requestPermission()) === "granted";
      if (granted) await sendNotification({ title: input.title, body: input.body });
    },
  });
  let settingsSaving = $state(false);
  const settingsSaver = createSettingsSaver(
    (next) => invoke("save_settings", { settings: next }), showError, (busy) => { settingsSaving = busy; },
  );
  let terminalTaskIds = $state<string[]>([]);
  let paneTaskIds = $state<string[]>([]);
  let activeTaskId = $state<string | null>(null);
  let quotaSelection = $state<{ taskId: string; modelKey: string } | null>(null);
  let selectedProjectId = $state<string | null>(null);
  let isStarting = $state(false);
  let startupPending = $state(true);
  let startupFailure = $state("");
  let pageDisposed = false;
  let errorMessage = $state("");
  let layout = $state<LayoutMode>("single");
  let view = $state<"workspace" | "settings">("workspace");
  let settingsOpen = $state(false);
  let shellOpen = $state(false);
  let settingsDialog = $state<HTMLElement>();
  let settingsReturnFocus: HTMLElement | null = null;
  let settingsCategory = $state<SettingsCategory>("general");
  let loginRequest = $state<{ provider: string; serial: number } | null>(null);
  let dshBusy = $state(false);
  let settingsPackageBusy = $state(false);
  let settingsCodemodeBusy = $state(false);
  const settingsBusy = $derived(settingsPackageBusy || settingsCodemodeBusy);
  let activeAgent = $state<"pi" | "dsh">("pi");
  // 把当前工作区告诉后端：桌宠任务环据此只显示该工作流的任务。
  $effect(() => {
    void invoke("set_active_agent", { agent: activeAgent }).catch(() => { /* 后端未就绪时忽略 */ });
  });
  let isDshStarting = $state(false);
  let dshHostError = $state<string | null>(null);
  let dialogRequest = $state<DialogRequest | null>(null);
  const dialogs = createDialogQueue((request) => { dialogRequest = request; });
  let switchingTasks = $state(new Set<string>());
  const modeSwitcher = createTaskModeSwitcher({
    confirm: (task, target) => confirmDialog(
      target === "rpc" ? t("切换到对话模式") : t("切换到终端兼容模式"),
      t("停止“{title}”的当前进程，并使用同一个 Pi 会话继续吗？当前未完成的响应可能中断。", { title: task.title }),
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
  // Pet and main-window actions share locks; native commands still validate runId.
  const taskControlBusy = new Set<string>();
  const handlePetTaskAction = createPetTaskActionHandler({
    loadTask: async (id) => (await invoke<Task[]>("list_tasks")).find((task) => task.id === id),
    isBusy: (id) => taskControlBusy.has(id) || modeSwitcher.isBusy(id),
    busyChanged: (id, busy) => { if (busy) taskControlBusy.add(id); else taskControlBusy.delete(id); },
    stop: (task) => invoke(task.interactionMode === "rpc" ? "stop_rpc_task" : "stop_pi_run", {
      taskId: task.id, runId: task.runId,
    }),
    restart: (task) => {
      if (task.piEnvironment && task.piEnvironment !== "managed") {
        throw new Error(t("该任务来自旧版本机 Pi 环境，当前稳定版仅支持 DeepPi 托管任务，本机会话记录已保留。"));
      }
      return requestTaskRestart(task, task.interactionMode ?? "tui");
    },
    prepareContinue: async () => {
      if (!canLeaveSettings()) throw new Error(t("主窗口正在处理设置操作，请稍后继续"));
      const main = getCurrentWindow();
      if (await main.isMinimized()) await main.unminimize();
      await main.show();
      await main.setFocus();
    },
    updated: (task) => {
      const current = tasks.find((candidate) => candidate.id === task.id);
      if (current) Object.assign(current, task); else tasks.unshift(task);
    },
    open: (task) => {
      if (!canLeaveSettings()) throw new Error(t("主窗口正在处理设置操作，请稍后继续"));
      view = "workspace";
      activeAgent = "pi";
      boardView = false;
      applyTaskRestart(tasks.find((candidate) => candidate.id === task.id) ?? task, task);
    },
    // 桌宠气泡点击「打开」：把主窗口带到前台并定位任务——Pi 进任务面板，
    // DSH 切到 DSH 视图（openTask 内部分流）。
    reveal: async (task) => {
      if (!canLeaveSettings()) throw new Error(t("主窗口正在处理设置操作，请稍后继续"));
      const main = getCurrentWindow();
      if (await main.isMinimized()) await main.unminimize();
      await main.show();
      await main.setFocus();
      openTask(task);
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
  /** 是否用「任务看板」替代流式对话视图（仅 Pi 工作区）。 */
  let boardView = $state(false);
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
  let quitFromSlash: (() => Promise<boolean>) | null = null;
  let recoveryBusy = $state(false);
  let recoveryProject = $state<{ id: string; name: string } | null>(null);
  let recoveryModule = $state.raw<Promise<typeof import("$lib/FileRecoveryPanel.svelte")> | null>(null);
  const fileWorkspace = createFileWorkspace({
    read: (projectId, relativePath) => invoke<FilePreview>("read_project_file", { projectId, relativePath }),
    save: (projectId, relativePath, content, expectedVersion) => invoke<FileSaveResult>("save_project_file", { projectId, relativePath, content, expectedVersion }),
    chooseClose: async (documents) => {
      const result = await dialogs.request({
        kind: "choice", title: t("未保存的文件"),
        message: documents.map((doc) => `${projects.find((project) => project.id === doc.projectId)?.name ?? doc.projectId} / ${doc.path}`).join("\n"),
        choices: [{ value: "save", label: t("保存后继续") }, { value: "discard", label: t("放弃未保存的修改") }],
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
  let dshVisibility = Promise.resolve();
  let sidebarWidth = $state<number | null>(null);
  let filesWidth = $state<number | null>(null);
  let gitWidth = $state<number | null>(null);
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
    const visible = activeAgent === "dsh" && view === "workspace" && !settingsOpen && !dialogRequest && !closingWindow && !recoveryProject;
    dshVisibility = dshVisibility.then(async () => {
      if (child) {
        if (visible) await child.show();
        else await child.hide();
      }
    }).catch((error) => { console.error("DSH visibility:", error); });
  });

  $effect(() => {
    if (settingsOpen) void tick().then(() => settingsDialog?.focus());
  });

  function handleSettingsKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      if ([...settingsDialog?.querySelectorAll(".editor-backdrop") ?? []].some((backdrop) => !backdrop.closest("[hidden]"))) return;
      event.preventDefault();
      closeSettings();
    }
  }

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

  async function fileTreeAction(action: FileTreeAction, entry: FileEntry | null) {
    const projectId = selectedProjectId;
    if (!projectId) return;
    if (action === "open" && entry && !entry.isDirectory) { openFile(entry.path); return; }
    try {
      if (action === "external" && entry && !entry.isDirectory) {
        if (!settings.externalEditor) { openSettingsCategory("advanced"); return; }
        await invoke("open_project_in_editor", { projectId, relativePath: entry.path, line: null, column: null });
        if (fileDocuments.some((doc) => doc.projectId === projectId && doc.path === entry.path))
          notices.push(t("已发送到外部编辑器；内置草稿未自动写入磁盘。"), "info");
        return;
      }
      if (action === "copyPath" && entry) {
        const path = await invoke<string>("project_entry_absolute_path", {
          projectId, relativePath: entry.path, isDirectory: entry.isDirectory,
        });
        await navigator.clipboard.writeText(path);
        return;
      }
      if (action === "newFile" || action === "newFolder") {
        const directory = action === "newFolder";
        const name = await dialogs.request({ kind: "input", title: t(directory ? "新建文件夹" : "新建文件"),
          message: t("输入名称"), initialValue: "", confirmLabel: t("新建") });
        if (typeof name !== "string" || !name.trim()) return;
        const relativePath = entry ? `${entry.path}/${name.trim()}` : name.trim();
        await invoke("create_project_entry", { projectId, relativePath, isDirectory: directory });
        if (!directory && selectedProjectId === projectId) openFile(relativePath);
        if (selectedProjectId === projectId) filesRefreshToken++;
        return;
      }
      if (!entry) return;
      const affected = (path: string) => pathContainsEntry(entry.path, path, entry.isDirectory);
      if (action === "rename") {
        const name = await dialogs.request({ kind: "input", title: t("重命名"), message: t("输入新名称"),
          initialValue: entry.name, confirmLabel: t("重命名") });
        if (typeof name !== "string" || !name.trim() || name.trim() === entry.name) return;
        const renamed = await fileWorkspace.mutatePaths(projectId, affected, () => invoke("rename_project_entry", {
          projectId, relativePath: entry.path, newName: name.trim(),
        }));
        if (!renamed) return;
      } else if (action === "delete") {
        if (!await confirmDialog(t("移到回收站"),
          t("将“{name}”移到回收站吗？可以从回收站恢复。", { name: entry.name }), t("移到回收站"))) return;
        const deleted = await fileWorkspace.mutatePaths(projectId, affected, () => invoke("delete_project_entry", {
          projectId, relativePath: entry.path, recursive: entry.isDirectory,
        }));
        if (!deleted) return;
      } else return;
      if (openedFile?.projectId === projectId && affected(openedFile.path)) openedFile = null;
      if (openedDiff?.projectId === projectId && affected(openedDiff.path)) openedDiff = null;
      if (selectedProjectId === projectId) filesRefreshToken++;
    } catch (error) {
      if (action === "delete" && selectedProjectId === projectId) filesRefreshToken++;
      showError(error);
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
      fileBusy: fileSaving, navigationLocked: settingsOpen && settingsBusy,
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
    else if (command === "workspacePi" || command === "workspaceDsh") {
      // 设置对话框打开时顶栏整体 inert，键盘切换保持同一语义。
      if (settingsOpen) return;
      void (command === "workspacePi" ? showPi() : showDsh());
    }
    else sidebarVisible = !sidebarVisible;
  }

  const PANEL_WIDTH_KEY = "deeppi-panel-widths";
  const PANEL_WIDTH_BOUNDS = { min: 170, max: 560 } as const;

  function clampPanelWidth(width: number): number {
    return Math.round(Math.min(PANEL_WIDTH_BOUNDS.max, Math.max(PANEL_WIDTH_BOUNDS.min, width)));
  }

  function loadPanelWidths() {
    try {
      const raw = localStorage.getItem(PANEL_WIDTH_KEY);
      if (!raw) return;
      const parsed: unknown = JSON.parse(raw);
      if (typeof parsed !== "object" || parsed === null) return;
      const widths = parsed as Record<string, unknown>;
      if (typeof widths.sidebar === "number" && Number.isFinite(widths.sidebar)) sidebarWidth = clampPanelWidth(widths.sidebar);
      if (typeof widths.files === "number" && Number.isFinite(widths.files)) filesWidth = clampPanelWidth(widths.files);
      if (typeof widths.git === "number" && Number.isFinite(widths.git)) gitWidth = clampPanelWidth(widths.git);
    } catch {
      // 忽略损坏的本地缓存。
    }
  }

  function savePanelWidths() {
    try {
      localStorage.setItem(PANEL_WIDTH_KEY, JSON.stringify({ sidebar: sidebarWidth, files: filesWidth, git: gitWidth }));
    } catch {
      // 存储不可用时静默降级。
    }
  }

  /// 面板边缘拖拽调宽：left 面板右缘向右拖增宽；right 面板左缘向左拖增宽。
  function startPanelResize(panel: "sidebar" | "files" | "git", event: PointerEvent) {
    const handle = event.currentTarget as HTMLElement;
    const panelEl = handle.parentElement;
    if (!panelEl) return;
    event.preventDefault();
    const startX = event.clientX;
    const initial = panelEl.getBoundingClientRect().width;
    const growRight = panel === "sidebar";
    handle.setPointerCapture(event.pointerId);
    handle.classList.add("resizing");
    document.body.classList.add("panel-resizing");
    const onMove = (move: PointerEvent) => {
      const delta = move.clientX - startX;
      const next = clampPanelWidth(initial + (growRight ? delta : -delta));
      if (panel === "sidebar") sidebarWidth = next;
      else if (panel === "files") filesWidth = next;
      else gitWidth = next;
    };
    const onUp = () => {
      handle.removeEventListener("pointermove", onMove);
      handle.removeEventListener("pointerup", onUp);
      handle.classList.remove("resizing");
      document.body.classList.remove("panel-resizing");
      savePanelWidths();
    };
    handle.addEventListener("pointermove", onMove);
    handle.addEventListener("pointerup", onUp);
  }

  function handleShortcut(event: KeyboardEvent) {
    if (startupPending || startupFailure) return;
    const decision = hostShortcutDecision(event, shortcutContext());
    if (decision.consume) { event.preventDefault(); event.stopPropagation(); }
    if (decision.command) runHostCommand(decision.command);
  }


  const piTasks = $derived(tasks.filter((task) => task.agent === "pi"));
  const selectedProject = $derived(
    projects.find((project) => project.id === selectedProjectId),
  );
  /// 同时运行任务数：只统计 AI 正在执行任务的会话（Running）。
  /// 空闲打开的会话（Waiting）不占额度，否则多开几个会话后就再也无法新建。
  const runningCount = $derived(piTasks.filter((task) => task.status === "running").length);
  const allTerminalTasks = $derived(
    terminalTaskIds
      .map((id) => tasks.find((task) => task.id === id))
      .filter((task): task is Task => task !== undefined),
  );
  const terminalTasks = $derived(
    allTerminalTasks.filter((task) => task.projectId === selectedProjectId),
  );
  const inlineSessionTabs = $derived(
    activeAgent === "pi" && view === "workspace" && !boardView && !inspectingFile &&
    !!selectedProject && terminalTasks.some((task) => task.id === activeTaskId &&
      task.interactionMode === "rpc" && paneTaskIds.includes(task.id)),
  );
  const standaloneSessionTabs = $derived(
    activeAgent === "pi" && !!selectedProject && terminalTasks.length > 0 && !inlineSessionTabs &&
    view === "workspace" && !(boardView && activeAgent === "pi"),
  );
  $effect(() => {
    standaloneSessionTabs;
    if (activeAgent === "dsh") void tick().then(syncDshBounds);
  });
  const projectTerminalIds = $derived(terminalTasks.map((task) => task.id));
  const paneCapacity = $derived(layout === "single" ? 1 : layout === "split" ? 2 : 4);
  const canStart = $derived(
    !startupPending && !startupFailure && !isStarting && !closingWindow &&
      runningCount < settings.maxConcurrentTasks &&
      selectedProjectId !== null,
  );


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
    // 上一轮的进度与“已关闭”状态不能留下来：重新检查意味着新的下载/安装周期。
    appUpdateProgress = null;
    appUpdateDismissed = false;
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
        error: describeUpdateError(error),
      };
    }
  }

  async function installDeepPiUpdate() {
    if (!pendingAppUpdate) return;
    appUpdate = { ...appUpdate, status: "installing", error: null };
    appUpdateProgress = null;
    appUpdateDismissed = false;
    try {
      // 右下角浮层是唯一的更新界面：下载是真实百分比，安装是按时间估算。
      await installAppUpdate(pendingAppUpdate, {
        onProgress: (progress) => { appUpdateProgress = progress; },
      });
    } catch (error) {
      // 设置里已没有应用更新入口，失败必须用提示条说清楚，而不是静默消失。
      appUpdateProgress = null;
      const message = describeUpdateError(error);
      appUpdate = { ...appUpdate, status: "error", error: message };
      notices.push(message, "error");
    }
  }


  async function installRuntime(update: RuntimeUpdate) {
    if (!update.latestVersion || !update.installable) return;
    await changeRuntime(update);
  }


  /// “未激活”型 DSH 启动失败：重新获取上游最新运行时后交由用户确认安装。
  async function alignDshRuntime() {
    if (busyRuntime) return;
    try {
      await checkUpdates(true);
      const update = updates.find((candidate) => candidate.id === "dsh");
      if (!update?.latestVersion || !update.installable) {
        showError(t("没有可安装的 DSH 运行时版本，请在“DSH 服务”里检查更新。"));
        return;
      }
      await installRuntime(update);
    } catch (error) {
      showError(error);
    }
  }

  async function uninstallRuntime(runtime: RuntimeComponent) {
    if (busyRuntime || runtime.id === "deeppi" || runtime.source === "development") return;
    busyRuntime = runtime.id;
    try {
      const warning = runtime.id === "dsh"
        ? t("将卸载托管 DSH 程序、插件和配置；会话历史与凭据保留。确定卸载吗？")
        : t("卸载 {name} 吗？保留会话历史和凭据；卸载后需要重新安装才能使用。", { name: runtime.name });
      if (!(await confirmDialog(t("卸载组件"), warning, t("卸载")))) return;
      await invoke("uninstall_runtime", { componentId: runtime.id });
      runtimes = await invoke<RuntimeComponent[]>("runtime_status");
      await checkUpdates(true);
    } catch (error) {
      showError(error);
      try { runtimes = await invoke<RuntimeComponent[]>("runtime_status"); } catch { /* retain original failure */ }
    } finally {
      busyRuntime = null;
    }
  }

  async function cancelRuntime() {
    try {
      if (!(await runtimeRunner.cancel())) showError(t("当前操作尚未接受取消，或已进入提交阶段。"));
    } catch (error) {
      showError(error);
    }
  }

  async function changeRuntime(update: RuntimeUpdate) {
    if (busyRuntime) return;
    busyRuntime = update.id;
    let stoppedDsh = false;
    const wasDshRunning = dshRunning;
    try {
      const confirmed = await confirmDialog(t("安装组件"), `${t("下载并激活 {name} {version} 吗？当前运行时会保留。", { name: update.name, version: String(update.latestVersion) })}${update.id === "dsh" && update.note ? ` ${tm(update.note)}` : ""}`, t("安装"));
      if (!confirmed) return;
      const affectsDsh = update.id === "dsh" || update.id === "dshmarket";
      if (affectsDsh && wasDshRunning) {
        await dshWebview?.close();
        dshWebview = null;
        await invoke("stop_dsh");
        stoppedDsh = true;
      }
      await runtimeRunner.run("install_runtime", { componentId: update.id, version: update.latestVersion });
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
    loadPanelWidths();
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
        const nextTerminalTaskIds = storedTasks.filter((task) => task.agent === "pi" && task.runId).map((task) => task.id);
        terminalTaskIds = nextTerminalTaskIds;
        const nextProjectId =
          storedProjects.find((project) => project.path === storedSettings.lastProject)?.id ??
          storedProjects[0]?.id ??
          null;
        selectedProjectId = nextProjectId;
        const initialPane = changePaneCapacity({
          current: [],
          active: null,
          available: nextTerminalTaskIds.filter((id) => storedTasks.find((task) => task.id === id)?.projectId === nextProjectId),
          capacity: paneCapacity,
        });
        paneTaskIds = initialPane.panes;
        activeTaskId = initialPane.active;
      },
      error: (scope, error) => {
        if (scope === "workspace") startupFailure = tm(String(error));
        else errorMessage = tm(String(error));
      },
    });
    void startup.ready.then((ready) => {
      if (disposed) return;
      startupPending = false;
      if (ready) {
        // Network update checks are optional background work, not a startup dependency.
        void checkUpdates();
        void checkDeepPiUpdate();
      }
    });

    const systemTheme = window.matchMedia("(prefers-color-scheme: light)");
    const handleSystemThemeChange = () => {
      if (settings.colorMode === "system") applyAppearance(settings);
    };
    systemTheme.addEventListener("change", handleSystemThemeChange);

    const appWindow = getCurrentWindow();
    const requestWindowClose = createWindowCloseHandler({
      // 托盘菜单的退出是显式退出，不受「关闭窗口行为」影响。
      behavior: (source) => (source === "tray" || !nativeReady ? "exit" : settings.closeBehavior),
      blocked: () => recoveryBusy || fileWorkspace.busy() || switchingTasks.size > 0 || gitIndexBusy || settingsBusy || busyRuntime !== null,
      blockedReason: () => recoveryBusy || fileWorkspace.busy() ? t("项目文件正在处理，请完成后再关闭窗口") : settingsBusy || busyRuntime !== null ? t("组件操作正在处理，请完成或取消后再关闭窗口") : gitIndexBusy ? t("Git 写入正在处理，请完成后再关闭窗口") : t("任务正在切换模式，请完成后再关闭窗口"),
      hasActiveTasks: () => tasks.some((task) => ["running", "waiting"].includes(task.status)),
      choose: closeChoiceDialog,
      confirm: () => confirmDialog(t("退出 DeepPi"), t("仍有活动任务，停止任务并退出吗？")),
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
    });
    quitFromSlash = () => requestWindowClose({ preventDefault() {} }, "tray");
    const closeListener = appWindow.onCloseRequested(async (event) => { await requestWindowClose(event); });
    // 托盘菜单的「退出」：复用既有关闭流程（活动任务确认、设置落盘、任务清理）。
    // 需要弹确认框或提示时，先把可能隐藏着的主窗口亮出来，否则用户看不到对话框。
    const trayQuitListener = listen("tray-quit", async () => {
      const needsVisibleWindow = recoveryBusy || fileWorkspace.busy()
        || switchingTasks.size > 0 || gitIndexBusy || settingsBusy || busyRuntime !== null
        || tasks.some((task) => ["running", "waiting"].includes(task.status));
      if (needsVisibleWindow) {
        await appWindow.show();
        await appWindow.setFocus();
      }
      await requestWindowClose({ preventDefault() {} }, "tray");
    });
    const trayRuntimeListener = listen<"pi" | "dsh">("tray-runtime-update", ({ payload }) => {
      if (payload === "pi" || payload === "dsh") openSettingsCategory(payload);
    });
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
    const boardActionListener = listen<BoardActionRequest>(BOARD_ACTION_EVENT, ({ payload }) => {
      void handleBoardAction(payload);
    });
    const petActionListener = listen<unknown>(PET_TASK_ACTION_EVENT, ({ payload }) => {
      if (!isPetTaskRequest(payload)) return;
      void handlePetTaskAction(payload)
        .then((result) => emitTo("pet", PET_TASK_RESULT_EVENT, result))
        .catch((error) => showError(error));
    });
    // 任务完成系统通知：pty（TUI）与 rpc 两条退出通道共用一个通知器。
    const ptyExitNoticeListener = listen<{ taskId: string; runId: string }>("pty-exit", ({ payload }) => {
      void taskNotifier.handlePtyExit(payload);
    });
    const rpcExitNoticeListener = listen<TaskStatusUpdate>("rpc-task-exit", ({ payload }) => {
      void taskNotifier.handleRpcExit(payload);
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
      quitFromSlash = null;
      startup.dispose();
      window.removeEventListener("keydown", handleShortcut, true);
      observer.disconnect();
      systemTheme.removeEventListener("change", handleSystemThemeChange);
      void closeListener.then((unlisten) => unlisten());
      void trayQuitListener.then((unlisten) => unlisten());
      void trayRuntimeListener.then((unlisten) => unlisten());
      void dshTaskListener.then((unlisten) => unlisten());
      void dshStatusListener.then((unlisten) => unlisten());
      void statusListener.then((unlisten) => unlisten());
      void rpcExitListener.then((unlisten) => unlisten());
      void ptyExitNoticeListener.then((unlisten) => unlisten());
      void rpcExitNoticeListener.then((unlisten) => unlisten());
      void boardActionListener.then((unlisten) => unlisten());
      void petActionListener.then((unlisten) => unlisten());
      void runtimeProgressListener.then((unlisten) => unlisten());
      dialogs.dispose();
      void dshWebview?.close();
    };
  });

  function showError(error: unknown) {
    // 后端只回消息码（`@msg:` 协议），在这里按当前语言渲染。
    // 错误不用阻塞弹窗：改为顶部居中的浮动提示，自动消失、不打断操作。
    errorMessage = tm(String(error));
    notices.push(errorMessage, "error");
  }

  function resolveDialog(value: DialogValue) {
    dialogs.resolve(value);
  }

  function confirmDialog(title: string, message: string, confirmLabel = t("确认")) {
    return dialogs.request({ kind: "confirm", title, message, confirmLabel })
      .then((value) => value === true);
  }

  function closeChoiceDialog() {
    return dialogs.request({
      kind: "choice",
      title: t("关闭 DeepPi"),
      message: t("选择点击窗口关闭按钮时的默认行为。这个选择会记住，也可以在设置中修改。"),
      choices: [
        { value: "minimize", label: t("最小化") },
        { value: "exit", label: t("退出应用") },
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
      placeholder: t("输入任务名称"),
      confirmLabel: t("保存"),
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
        title: t("选择 Pi 项目目录"),
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
    if (recoveryBusy) { showError(t("恢复副本正在处理，请完成后再移除项目。")); return; }
    if (
      !(await confirmDialog(
        t("移除项目"),
        t("从工作区移除“{name}”吗？本机目录、任务和会话不会被删除。", { name: project.name }),
        t("移除"),
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
      // RPC 会话支持「未启动」状态：直接打开（休眠态，不占进程/额度），
      // 输入提示词回车后才真正启动 pi；TUI 终端仍需要活动进程。
      if (task.interactionMode !== "rpc") {
        showError(t("该任务没有活动终端，请先重启任务"));
        return;
      }
      terminalTaskIds.push(task.id);
    }
    const available = terminalTaskIds.filter(
      (id) => tasks.find((candidate) => candidate.id === id)?.projectId === task.projectId,
    );
    const selection = openPane({
      current: paneTaskIds,
      active: activeTaskId,
      requested: task.id,
      available,
      capacity: paneCapacity,
    });
    applyPaneSelection(selection);
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
      showError(t("该任务没有活动终端，请先重启任务"));
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
      : t("{id} 未安装", { id });
  }

  /** 工作区切换按钮的悬停提示：始终带可读说明，有运行时版本信息时追加在后面。 */
  function workspaceTitle(label: string, id: RuntimeComponent["id"]): string {
    const runtime = runtimeTitle(id);
    return runtime ? `${label} · ${runtime}` : label;
  }

  async function syncDshBounds() {
    if (!dshWebview || activeAgent !== "dsh") return;
    const bounds = (workspace.querySelector(".dsh-placeholder") ?? workspace).getBoundingClientRect();
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
      if (settingsOpen) {
        await dshWebview.hide();
      } else {
        await dshWebview.show();
        await dshWebview.setFocus();
      }
      await syncDshBounds();
      return;
    }
    isDshStarting = true;
    dshHostError = null;
    errorMessage = "";
    try {
      const url = await invoke<string>("start_dsh");
      const bounds = (workspace.querySelector(".dsh-placeholder") ?? workspace).getBoundingClientRect();
      await invoke("create_dsh_webview", {
        baseUrl: url,
        x: bounds.left,
        y: bounds.top,
        width: Math.max(1, bounds.width),
        height: Math.max(1, bounds.height),
      });
      dshWebview = await waitForDshWebview();
      if (activeAgent !== "dsh" || dialogRequest || view !== "workspace" || settingsOpen) await dshWebview.hide();
      else await dshWebview.setFocus();
      await syncDshBounds();
    } catch (error) {
      activeAgent = "pi";
      dshHostError = tm(String(error));
      showError(t("DSH 启动失败：{error}", { error: tm(String(error)) }));
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
    if (!settingsOpen && document.activeElement instanceof HTMLElement) settingsReturnFocus = document.activeElement;
    settingsOpen = true;
    void dshWebview?.hide();
  }

  function closeSettings() {
    if (!canLeaveSettings()) return;
    settingsOpen = false;
    const returnFocus = settingsReturnFocus;
    settingsReturnFocus = null;
    void tick().then(() => {
      if (returnFocus?.isConnected && returnFocus !== document.body && returnFocus !== document.documentElement && !returnFocus.closest("[inert]")) returnFocus.focus();
      else if (activeAgent === "dsh") void dshVisibility.then(() => dshWebview?.setFocus());
      else document.getElementById("open-settings")?.focus();
    });
  }

  function canLeaveSettings() {
    if (settingsOpen && dshBusy) {
      showError(t("DSH 检测/修复操作正在处理，请先完成或取消操作"));
      return false;
    }
    if (settingsOpen && settingsBusy) {
      showError(t("扩展操作正在处理，请先完成或取消操作"));
      return false;
    }
    return true;
  }

  function openSettingsCategory(category: SettingsCategory) {
    settingsCategory = category;
    openSettings();
  }

  async function hostSlash(task: Task, command: "new" | "settings" | "login" | "logout" | "reload" | "quit" | "resume" | "scoped-models" | "changelog" | "hotkeys" | "import" | "share" | "trust", args: string): Promise<boolean> {
    switch (command) {
      case "resume": {
        const available = tasks.filter((candidate) => candidate.agent === "pi" && candidate.id !== task.id && candidate.projectId === task.projectId && candidate.interactionMode === "rpc" && !candidate.archivedAt);
        if (!available.length) { showError(t("此项目没有其他可恢复的会话")); return false; }
        const selected = args || await dialogs.request({ kind: "choice", title: t("恢复会话"),
          message: t("选择此项目的会话"), choices: available.map((candidate) => ({ value: candidate.id, label: candidate.title })) });
        const target = available.find((candidate) => candidate.id === selected);
        if (!target) { if (selected) showError(t("找不到指定的会话")); return false; }
        openTask(target);
        return activeTaskId === target.id;
      }
      case "hotkeys": {
        await dialogs.request({ kind: "confirm", title: t("快捷键"),
          message: ["tasks", "files", "settings", "workspacePi", "workspaceDsh", "sidebar", "composer", "save", "saveAs", "nextDiff", "previousDiff"].map((id) => `${id}: ${shortcutLabel(id as Parameters<typeof shortcutLabel>[0])}`).join("\n") });
        return true;
      }
      case "changelog": {
        const text = await invoke<string>("pi_slash_changelog");
        await dialogs.request({ kind: "alert", title: t("Pi 更新日志"), message: text });
        return true;
      }
      case "scoped-models": {
        if (!task.runId) { showError(t("请先启动会话后再使用命令")); return false; }
        const result = await invoke<{ models: { provider: string; id: string; name: string }[] }>("rpc_command", {
          taskId: task.id, runId: task.runId, command: { type: "get_available_models" },
        });
        const models = result.models ?? [];
        if (!models.length) { showError(t("当前会话没有可用模型")); return false; }
        const saved = await invoke<string[] | null>("pi_slash_config", { taskId: task.id, action: "get_scoped", payload: null });
        const enabled = new Set(saved ?? models.map((model) => `${model.provider}/${model.id}`));
        while (true) {
          const choice = await dialogs.request({ kind: "choice", title: t("轮换模型"),
            message: t("选择模型切换状态，完成后保存；重新加载会话后生效。"),
            choices: [{ value: "save", label: t("保存选择") }, { value: "all", label: t("启用全部模型") },
              ...models.map((model) => ({ value: `${model.provider}/${model.id}`, label: `${enabled.has(`${model.provider}/${model.id}`) ? "[x]" : "[ ]"} ${model.name} (${model.provider})` }))],
          });
          if (choice === "all") { enabled.clear(); for (const model of models) enabled.add(`${model.provider}/${model.id}`); continue; }
          if (choice === "save") {
            const chosen = [...enabled];
            await invoke("pi_slash_config", { taskId: task.id, action: "set_scoped", payload: chosen.length === models.length ? null : chosen });
            await dialogs.request({ kind: "alert", title: t("轮换模型"), message: t("模型选择已保存。重新加载会话后生效。") });
            return true;
          }
          if (typeof choice !== "string" || !models.some((model) => `${model.provider}/${model.id}` === choice)) return false;
          if (enabled.has(choice)) enabled.delete(choice); else enabled.add(choice);
        }
      }
      case "import":
      case "share":
      case "trust":
      case "login":
      case "logout":
      case "reload":
      case "quit":
        return runSensitiveHostSlash(task, command as SensitiveHostSlash, args, {
          invoke,
          confirm: confirmDialog,
          dialog: (request) => dialogs.request(request),
          chooseImport: () => open({ filters: [{ name: "Pi JSONL", extensions: ["jsonl"] }], multiple: false, title: t("导入 Pi 会话") }),
          reportError: showError,
          openLogin: (provider) => {
            loginRequest = provider ? { provider, serial: (loginRequest?.serial ?? 0) + 1 } : null;
            openSettingsCategory("models");
          },
          notifyModelsChanged,
          restart: async (current) => { await restartTask(current, "rpc"); },
          quit: quitFromSlash,
        });
      case "new": {
        const before = tasks.length;
        await startTask(task.projectId);
        return tasks.length > before;
      }
      case "settings": openSettings(); return true;
    }
  }

  async function startTask(projectId: string | null = selectedProjectId, mode: "rpc" | "tui" = "rpc") {
    const project = projects.find((candidate) => candidate.id === projectId);
    if (!project) {
      showError(t("请先添加并选择一个 Pi 项目目录"));
      return;
    }
    if (isStarting) return;
    if (runningCount >= settings.maxConcurrentTasks) {
      showError(t("最多同时运行 {limit} 个任务，请等待部分任务完成，或在设置中调大「同时运行任务数」", { limit: settings.maxConcurrentTasks }));
      return;
    }
    isStarting = true;
    errorMessage = "";
    try {
      if (selectedProjectId !== project.id) selectProject(project);
      if (runningCount >= settings.maxConcurrentTasks) {
        showError(t("最多同时运行 {limit} 个任务，请等待部分任务完成，或在设置中调大「同时运行任务数」", { limit: settings.maxConcurrentTasks }));
        return;
      }
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
    if (modeSwitcher.isBusy(task.id) || taskControlBusy.has(task.id)) return;
    taskControlBusy.add(task.id);
    try {
      if (task.interactionMode === "rpc") {
        await invoke("stop_rpc_task", { taskId: task.id, runId: task.runId });
      } else {
        await invoke("stop_pi_run", { taskId: task.id, runId: task.runId });
      }
      task.status = "cancelled";
    } catch (error) {
      showError(error);
    } finally {
      taskControlBusy.delete(task.id);
    }
  }

  /// 任务看板浮窗：独立 OS 窗口，停靠在主窗口右侧（Rust 端负责定位与跟随）。
  async function openBoardWindow() {
    if (!canLeaveSettings()) return;
    try {
      await invoke("open_board_window");
    } catch (cause) {
      // 浮窗创建失败（如平台限制）时退回应用内看板，功能不缺失。
      showError(t("任务看板窗口打开失败：{error}", { error: tm(String(cause)) }));
      view = "workspace";
      activeAgent = "pi";
      boardView = true;
    }
  }

  /// 四象限清单浮窗：独立 OS 窗口，打开失败只显示非阻塞错误提示。
  async function openChecklistWindow() {
    if (!canLeaveSettings()) return;
    try {
      await invoke("open_checklist_window");
    } catch (cause) {
      showError(t("四象限清单窗口打开失败：{error}", { error: tm(String(cause)) }));
    }
  }

  async function openShell() {
    if (!canLeaveSettings()) return;
    if (!selectedProjectId || !selectedProject) {
      showError(t("请先添加并选择一个项目目录"));
      return;
    }
    activeAgent = "pi";
    view = "workspace";
    boardView = false;
    shellOpen = true;
    await dshWebview?.hide();
  }

  function closeShell() {
    shellOpen = false;
  }

  /// 看板浮窗没有终端窗格与模式切换器：卡片动作全部委托回主窗口执行。
  async function handleBoardAction(request: BoardActionRequest) {
    const task = request.taskId ? tasks.find((candidate) => candidate.id === request.taskId) : undefined;
    try {
      switch (request.action) {
        case "open":
          if (!task) return;
          // 打开任务要在主窗口里进行：先把主窗口带回前台，并确保 Pi 工作区可见
          //（可能停在 DSH 视图、应用内看板或设置页）。
          if (await getCurrentWindow().isMinimized()) await getCurrentWindow().unminimize();
          await getCurrentWindow().show();
          await getCurrentWindow().setFocus();
          view = "workspace";
          activeAgent = "pi";
          boardView = false;
          openTask(task);
          break;
        case "stop":
          if (task) await stopTask(task);
          break;
        case "restart":
          if (task) await restartTask(task);
          break;
        case "archive":
          if (task) await archiveTask(task);
          break;
        case "restore":
          if (task) await restoreTask(task);
          break;
        case "delete":
          if (task) await deleteTask(task);
          break;
        case "new-task":
          view = "workspace";
          activeAgent = "pi";
          boardView = false;
          void startTask(request.projectId ?? undefined);
          break;
      }
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
    const title = await inputDialog(t("重命名任务"), t("修改任务在项目列表中的显示名称。"), task.title);
    if (!title?.trim()) return;
    try {
      await invoke("rename_task", { taskId: task.id, title });
      task.title = title.trim();
      task.titleOrigin = "manual";
    } catch (error) {
      showError(error);
    }
  }

  async function manualRenameTask(task: Task, title: string) {
    await invoke("rename_task", { taskId: task.id, title });
    task.title = title.trim();
    task.titleOrigin = "manual";
  }

  function autoRenameTask(task: Task, title: string, expected: string): boolean {
    if (task.titleOrigin !== "auto" || task.title !== expected) return false;
    task.title = title;
    return true;
  }

  async function closeTask(task: Task) {
    if (taskControlBusy.has(task.id)) return;
    if (!modeSwitcher.isBusy(task.id) && ["running", "waiting"].includes(task.status)) {
      try { await stopTask(task); } catch { /* 停止失败时仍然关闭标签页 */ }
    }
    removeTerminal(task.id);
  }

  function requestTaskRestart(task: Task, mode: InteractionMode, expectedRunId?: string | null) {
    return invoke<Task>(mode === "rpc" ? "start_rpc_task" : "restart_pi_task", {
      request: { taskId: task.id, projectId: task.projectId, title: task.title, rows: 32, cols: 100,
        ...(expectedRunId ? { expectedRunId } : {}) },
    });
  }

  function applyTaskRestart(task: Task, restarted: Task) {
    Object.assign(task, restarted);
    if (!terminalTaskIds.includes(task.id)) terminalTaskIds.push(task.id);
    openTask(task);
  }

  async function restartTask(task: Task, mode = task.interactionMode ?? "tui", expectedRunId?: string | null): Promise<boolean> {
    if (modeSwitcher.isBusy(task.id) || taskControlBusy.has(task.id)) return false;
    if (task.piEnvironment && task.piEnvironment !== "managed") {
      showError(t("该任务来自旧版本机 Pi 环境，当前稳定版仅支持 DeepPi 托管任务，本机会话记录已保留。"));
      return false;
    }
    taskControlBusy.add(task.id);
    try {
      // 启动请求必须收口：一旦它永不返回，taskControlBusy 会永久持有这个 id，
      // 之后该任务的打开/停止/重启/关闭全都在开头的 busy 检查里静默返回，
      // 表现就是「点任务没反应」。给请求加时间上限，超时后释放锁、可以重试。
      const outcome = await runBoundedStep(
        "pi_start",
        () => requestTaskRestart(task, mode, expectedRunId),
        PI_START_TIMEOUT_MS,
      );
      if (!outcome.ok) {
        showError(outcome.error);
        return false;
      }
      applyTaskRestart(task, outcome.value);
      return true;
    } finally {
      taskControlBusy.delete(task.id);
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
    if (taskControlBusy.has(task.id)) return;
    if (mode === "tui" && !terminalModule) loadTerminalModule();
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
    if (!(await confirmDialog(t("删除任务"), t("永久删除“{title}”的 DeepPi 任务记录吗？", { title: task.title })))) return;
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

{#snippet sessionTabs()}
  <TaskTabs
    tasks={terminalTasks}
    {activeTaskId}
    onOpen={(task) => { if (task.agent === "pi") { boardView = false; void showPi(); } openTask(task); }}
    onRename={renameTask}
    onStop={stopTask}
    onArchive={archiveTask}
    onRestart={restartTask}
    onRestore={restoreTask}
    onDelete={deleteTask}
    onClose={closeTask}
    onAdd={() => void startTask()}
    onSplit={splitTask}
    splitActive={layout !== "single"}
    onUnsplit={() => changeLayout("single")}
  />
{/snippet}

<svelte:head><title>DeepPi</title></svelte:head>

{#if startupPending || startupFailure}
  <section class="startup-status" aria-label={t("工作区启动")} aria-live="polite">
    {#if startupFailure}
      <p role="alert">{tm(startupFailure)}</p>
      <button type="button" onclick={() => window.location.reload()}><RotateCcw size={16} />{t("重新加载")}</button>
    {:else}
      <p role="status">{t("正在加载工作区…")}</p>
    {/if}
  </section>
{/if}

  <div style={`--sidebar-w:${sidebarWidth ?? 260}px; --files-w:${filesWidth ?? 260}px; --git-w:${gitWidth ?? 300}px`} class:dsh-mode={activeAgent === "dsh"} class:sidebar-hidden={!showSidebar} class:files-hidden={!showFiles} class:files-visible={showFiles} class:git-visible={showGit} class="app-shell" inert={closingWindow || startupPending || !!startupFailure || settingsOpen} aria-busy={closingWindow || startupPending}>
  <header class="topbar">
    <!-- 工作区切换器：两种工作区下位置恒定，替代已删除的控制柱（ADR-0001）。 -->
    <div class="workspace-switcher" role="group" aria-label={t("工作区切换")}>
      <button type="button" class:selected={activeAgent === "pi"}
        aria-label={t("Pi 工作区")} aria-pressed={activeAgent === "pi"}
        title={workspaceTitle(t("Pi 工作区"), "pi")} onclick={showPi}><Bot size={14} /></button>
      <button type="button" class:selected={activeAgent === "dsh"}
        aria-label={t("DSH 工作区")} aria-pressed={activeAgent === "dsh"}
        title={workspaceTitle(t("DSH 工作区"), "dsh")} onclick={showDsh}><Globe size={14} /></button>
    </div>
    <div class="stage-crumbs">
      {#if activeAgent === "pi" && selectedProject}
        <span class="crumb-project"><span class="crumb-dot"></span>{selectedProject.name}</span>
      {:else if activeAgent === "pi"}
        <span class="crumb-task">{t("尚未选择项目")}</span>
      {:else}
        <span class="crumb-project"><span class="crumb-dot"></span>DeepSeek Harness</span>
      {/if}
    </div>


    <div class="toolbar">
      {#if activeAgent === "pi"}
        <button type="button" aria-label={t("切换 Git 变更栏")} title={showGit ? t("隐藏 Git 变更栏") : t("显示 Git 变更栏")}
          aria-pressed={showGit} onclick={() => { if (!canLeaveSettings()) return; gitVisible = !gitVisible; }}><GitBranch size={16} /></button>
      {/if}
      {#if activeAgent === "pi"}
        <button type="button" aria-label={t("命令终端")} title={t("打开命令终端")} aria-pressed={shellOpen}
          onclick={() => void openShell()}><Terminal size={16} /></button>
      {/if}
      <button type="button" aria-label={t("任务看板")}
        title={t("任务看板：总览所有任务状态")}
        onclick={() => void openBoardWindow()}><Kanban size={16} /></button>
      <button type="button" aria-label={t("四象限清单")} title={t("四象限清单")}
        onclick={() => void openChecklistWindow()}><ClipboardList size={16} /></button>
      <button id="open-settings" type="button" aria-label={t("打开应用设置")} aria-keyshortcuts={shortcutAria("settings")}
        title={`${t("设置")} (${shortcutLabel("settings")})`} onclick={openSettings}><Settings2 size={16} /></button>
    </div>
  </header>
  <aside class="project-sidebar" class:panel-hidden={!showSidebar} aria-label={t("项目侧栏")}>
    <div class="panel-resize-handle resize-right" role="separator" aria-orientation="vertical" aria-label={t("拖拽调整项目侧栏宽度")} title={t("拖拽调整宽度")} onpointerdown={(event) => startPanelResize("sidebar", event)}></div>
    <button type="button" class="panel-toggle" aria-label={t("隐藏项目侧栏")} aria-keyshortcuts={shortcutAria("sidebar")}
      title={`${t("隐藏项目侧栏")} (${shortcutLabel("sidebar")})`} onclick={() => { sidebarVisible = false; }}><PanelLeftClose size={15} /></button>
    <TaskSidebar
      {projects}
      tasks={piTasks}
      selectedProjectId={selectedProjectId}
      activeTaskId={activeTaskId}
      quotaModelKey={quotaSelection?.taskId === activeTaskId ? quotaSelection.modelKey : ""}
      quotaSessionKey={`${activeTaskId ?? ""}:${activeTask?.runId ?? ""}`}
      quotaVisible={inlineSessionTabs}
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

  <main class="workspace" bind:this={workspace}>
    {#if boardView && activeAgent === "pi" && view === "workspace"}
      <div class="board-view">
        <TaskBoard
          tasks={piTasks}
          {projects}
          {selectedProjectId}
          {activeTaskId}
          onOpen={(task) => { boardView = false; openTask(task); }}
          onStop={stopTask}
          onRestart={restartTask}
          onArchive={archiveTask}
          onRestore={restoreTask}
          onDelete={deleteTask}
          onNewTask={() => void startTask()}
        />
      </div>
    {/if}
    {#if activeAgent === "pi" && view === "workspace" && !sidebarVisible}
      <button type="button" class="panel-toggle workspace-toggle sidebar-toggle" aria-label={t("展开项目侧栏")}
        aria-keyshortcuts={shortcutAria("sidebar")} title={`${t("展开项目侧栏")} (${shortcutLabel("sidebar")})`}
        onclick={() => { sidebarVisible = true; }}><PanelLeftOpen size={15} /></button>
    {/if}
    {#if activeAgent === "pi" && view === "workspace" && !filesVisible}
      <button type="button" class="panel-toggle workspace-toggle files-toggle" aria-label={t("展开文件栏")} title={t("展开文件栏")}
        onclick={() => { filesVisible = true; }}><PanelRightOpen size={15} /></button>
    {/if}
    {#if standaloneSessionTabs}
      <div style="min-width: 0; padding: 2px 8px;">{@render sessionTabs()}</div>
    {/if}
    {#if activeAgent === "dsh" && view === "workspace"}
      <div class="dsh-placeholder" style:grid-row={standaloneSessionTabs ? "2" : "1 / -1"} aria-live="polite">
        {#if isDshStarting}
          <span>{t("正在启动 DSH")}</span>
        {:else if dshHostError}
          <p class="dsh-error" role="alert">
            <strong>{t("DSH 启动失败")}</strong>
            <span>{tm(dshHostError)}</span>
          </p>
          <DshFailure {dshRunning} {busyRuntime} confirm={confirmDialog} onError={showError}
            onUpdateDshRuntime={() => void alignDshRuntime()} onBusyChange={(busy) => { dshBusy = busy; }} />
          <div class="dsh-error-actions">
            <button type="button" class="start-button" onclick={showDsh} disabled={isDshStarting || dshBusy}>
              <RotateCcw size={16} />{t("重启 DSH")}
            </button>
          </div>
        {/if}
      </div>
    {:else if !selectedProject}
      <div class="empty-state" class:panel-hidden={boardView}>
        <Bot size={32} strokeWidth={1.4} />
        <h1>{t("工作区")}</h1>
        <button type="button" class="start-button" onclick={addProject}>
          <FolderPlus size={16} />{t("添加项目目录")}
        </button>
      </div>
    {:else if terminalTasks.length === 0}
      <div class="empty-state" class:panel-hidden={boardView}>
        <Bot size={32} strokeWidth={1.4} />
        <h1>{selectedProject.name}</h1>
        <button type="button" class="start-button" onclick={() => void startTask()} disabled={!canStart}>
          <Plus size={16} />{t("新建任务")}
        </button>
      </div>
    {/if}
      <div class="workspace-content" style:grid-row={inlineSessionTabs ? "1 / -1" : undefined} class:panel-hidden={boardView || activeAgent !== "pi" || view !== "workspace" || terminalTasks.length === 0 || !selectedProject}>
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
                taskId={task.id} runId={task.runId} stopped={!(["running", "waiting"].includes(task.status))} title={task.title}
                tabs={inlineSessionTabs && task.id === activeTaskId ? sessionTabs : undefined}
                autoName={task.titleOrigin === "auto"}
                onAutoRename={(title, expected) => autoRenameTask(task, title, expected)}
                onManualRename={(title) => manualRenameTask(task, title)}
                switching={switchingTasks.has(task.id)}
                visible={activeAgent === "pi" && view === "workspace" && !inspectingFile && task.projectId === selectedProjectId && paneTaskIds.includes(task.id)}
                active={task.id === activeTaskId}
                onModelChange={(modelKey) => {
                  if (task.id === activeTaskId && (quotaSelection?.taskId !== task.id || quotaSelection.modelKey !== modelKey)) {
                    quotaSelection = { taskId: task.id, modelKey };
                  }
                }}
                focusToken={composerFocusToken}
                onUseTerminal={() => void switchTaskMode(task, "tui")}
                onDialog={(request) => dialogs.request(request)}
                onCancelDialogs={(scope) => dialogs.cancelScope(scope)}
                onActivity={(busy) => {
                  if (task.status === "running" && !busy && task.projectId === selectedProjectId) filesRefreshToken++;
                  if (["running", "waiting"].includes(task.status)) task.status = busy ? "running" : "waiting";
                }}
              onOpenModelSettings={() => openSettingsCategory("models")}
              onReloadSession={() => void restartTask(task, "rpc")}
              onStartSession={() => restartTask(task, "rpc")}
              onRecoverSession={async (expectedRunId) => {
                if (task.runId !== expectedRunId) throw new Error(t("运行中的会话已变化，请重新检查任务状态"));
                const restarted = await restartTask(task, "rpc", expectedRunId);
                return restarted ? task.runId : null;
              }}
              onHostSlash={(command, args) => hostSlash(task, command, args)}
              onSessionRebound={async () => {
                const updated = (await invoke<Task[]>("list_tasks")).find((candidate) => candidate.id === task.id);
                if (!updated || updated.sessionId === task.sessionId) throw new Error(t("会话映射未更新"));
                task.sessionId = updated.sessionId;
                task.sessionFile = updated.sessionFile;
              }}
              chatDetailLevel={settings.chatDetailLevel}
              />
            {:else if terminalModule}
              {#await terminalModule}
                <section class="terminal-pane" class:panel-hidden={activeAgent !== "pi" || view !== "workspace" || inspectingFile || task.projectId !== selectedProjectId || !paneTaskIds.includes(task.id)}>
                  <p role="status">{t("正在加载终端…")}</p>
                </section>
              {:then terminal}
                <terminal.default
                  transitioning={switchingTasks.has(task.id)}
                  runId={task.runId}
                  taskId={task.id}
                  title={task.title}
                  status={task.status}
                  codeFont={settings.codeFont}
                  themeCodeFont={activeTheme.typography?.codeFont}
                  sessionFontName={settings.sessionFontName}
                  sessionFontSize={effectiveSessionFontSize}
                  colorMode={settings.colorMode}
                  visible={activeAgent === "pi" && view === "workspace" && !inspectingFile && task.projectId === selectedProjectId && paneTaskIds.includes(task.id)}
                  active={task.id === activeTaskId}
                  onExit={(exitCode, error, exitedRunId) => { if (task.runId === exitedRunId) markExited(task, exitCode, error); }}
                  onUseConversation={() => void switchTaskMode(task, "rpc")}
                />
              {:catch}
                <section class="terminal-pane" class:panel-hidden={activeAgent !== "pi" || view !== "workspace" || inspectingFile || task.projectId !== selectedProjectId || !paneTaskIds.includes(task.id)}>
                  <p role="alert">{t("终端组件加载失败")}</p>
                  <button type="button" title={t("重新加载终端组件")} aria-label={t("重新加载终端组件")} onclick={loadTerminalModule}><RotateCcw size={16} /></button>
                </section>
              {/await}
            {:else}
              <section class="terminal-pane" class:panel-hidden={activeAgent !== "pi" || view !== "workspace" || inspectingFile || task.projectId !== selectedProjectId || !paneTaskIds.includes(task.id)}>
                <p role="status">{t("正在加载终端…")}</p>
              </section>
            {/if}
          {/each}
        </div>
      </div>
    <div class="file-preview-layer" class:panel-hidden={!inspectingFile}
      style="position: absolute; inset: 0; min-width: 0; min-height: 0; z-index: 2;" style:grid-row={standaloneSessionTabs ? "2" : "1 / -1"}>
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
        <p role="alert">{t("编辑器加载失败：{error}", { error: tm(String(error)) })}</p>
      {/await}
    {/if}
    {#if openedDiff && openedDiff.projectId === selectedProjectId && activeAgent === "pi" && view === "workspace"}
      <GitDiffView selection={openedDiff} refreshToken={filesRefreshToken + gitWatchRefreshToken} onClose={() => { openedDiff = null; }} />
    {/if}
    </div>
    {#if shellOpen && selectedProject}
      <div class="shell-overlay">
        <ShellPane
          projectId={selectedProject.id}
          shell={settings.terminalShell}
          codeFont={settings.codeFont}
          themeCodeFont={activeTheme.typography?.codeFont}
          sessionFontName={settings.sessionFontName}
          sessionFontSize={effectiveSessionFontSize}
          colorMode={settings.colorMode}
          onClose={closeShell}
        />
      </div>
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
          <p>{t("恢复副本界面加载失败：{error}", { error: tm(String(error)) })}</p>
          <button onclick={() => { recoveryProject = null; recoveryModule = null; }}>{t("关闭")}</button>
        </section>
      {/await}
    {/if}
  </main>
  <aside class="file-panel" class:panel-hidden={!showFiles} aria-label={t("文件侧栏")}>
    <div class="panel-resize-handle resize-left" role="separator" aria-orientation="vertical" aria-label={t("拖拽调整文件栏宽度")} title={t("拖拽调整宽度")} onpointerdown={(event) => startPanelResize("files", event)}></div>
    <button type="button" class="panel-toggle" aria-label={t("隐藏文件栏")} title={t("隐藏文件栏")}
      onclick={() => { filesVisible = false; }}><PanelRightClose size={15} /></button>
    {#if fileSidebarVisited}
      <FileSidebar project={selectedProject} onOpen={openFile} onAction={fileTreeAction}
        visible={showFiles} refreshToken={filesRefreshToken}
        {watchError} onRetryWatch={() => { watchRetry++; }}
        searchFocusToken={showFiles ? fileSearchFocusToken : 0} />
    {/if}
  </aside>
  <GitSidebar projectId={selectedProjectId}
    visible={showGit} refreshToken={filesRefreshToken + gitWatchRefreshToken}
    onResizeStart={(event) => startPanelResize("git", event)}
    selected={openedDiff?.projectId === selectedProjectId ? openedDiff : null} onOpen={openGitDiff}
    onBusyChange={(busy) => { gitIndexBusy = busy; }}
    onChanged={(projectId, path) => {
      if (openedDiff?.projectId === projectId && (!path || openedDiff.path === path)) openedDiff = null;
      if (selectedProjectId === projectId) filesRefreshToken++;
    }}
    onClose={() => { gitVisible = false; }} />

  </div>
<AppDialog request={dialogRequest} onResolve={resolveDialog} />
<AppToasts />
{#if appUpdateProgress || (appUpdate.status === "available" && !appUpdateDismissed)}
  <div class="app-update-float" role="region" aria-label={t("应用更新")}>
    <div class="app-update-float-head">
      <strong>{t("应用更新")}{appUpdate.version ? ` · v${appUpdate.version}` : ""}</strong>
      {#if !appUpdateProgress}
        <button type="button" class="app-update-float-close" aria-label={t("关闭提示")} title={t("关闭提示")}
          onclick={() => { appUpdateDismissed = true; }}><X size={12} /></button>
      {/if}
    </div>
    {#if appUpdateProgress}
      <AppUpdateProgressBar progress={appUpdateProgress} />
    {:else}
      <button type="button" class="quiet-button" onclick={() => void installDeepPiUpdate()}>
        <Download size={14} />{t("安装更新")}
      </button>
    {/if}
  </div>
{/if}
{#if settingsOpen}
  <div class="settings-overlay" role="presentation">
    <div class="settings-modal" role="dialog" aria-modal="true" aria-label={t("设置")} tabindex="-1"
      bind:this={settingsDialog} onkeydown={handleSettingsKeydown}>
      <PiSettings
        saving={settingsSaving}
        closeBlocked={settingsBusy || dshBusy}
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
        onChangeSettings={updateSettings}
        onEditorSaved={(externalEditor) => { settings = { ...settings, externalEditor }; }}
        onCheckUpdates={() => void checkUpdates(true)}
        onUpdateRuntime={(update) => void installRuntime(update)}
        onUninstallRuntime={(runtime) => void uninstallRuntime(runtime)}
        {runningPiCount}
        {dshRunning}
        onClose={closeSettings}
      >
        {#snippet models()}
          <PiProviderSettings embedded confirm={confirmDialog} onClose={closeSettings} onError={showError} {loginRequest}
            codexTransport={settings.codexTransport} onCodexTransportChange={(codexTransport) => updateSettings({ ...settings, codexTransport })} />
        {/snippet}
        {#snippet codemode()}
          <PiCodemodeSettings projectPath={selectedProject?.path ?? null} onError={showError} onBusyChange={(busy) => { settingsCodemodeBusy = busy; }} />
        {/snippet}
        {#snippet extensions()}
          <PiMarketplace embedded projectPath={selectedProject?.path ?? null} confirm={confirmDialog} onClose={closeSettings} onError={showError} onBusyChange={(busy) => { settingsPackageBusy = busy; }} />
        {/snippet}
        {#snippet mcp()}
          <PiMcpSkillsSettings mode="mcp" confirm={confirmDialog} onError={showError} projectPath={selectedProject?.path ?? null} onBusyChange={(busy) => { settingsPackageBusy = busy; }} />
        {/snippet}
        {#snippet skills()}
          <PiMcpSkillsSettings mode="skills" confirm={confirmDialog} onError={showError} projectPath={selectedProject?.path ?? null} onBusyChange={(busy) => { settingsPackageBusy = busy; }} />
        {/snippet}
        {#snippet dsh()}
          <DshSettings {dshRunning} />
        {/snippet}
      </PiSettings>
    </div>
  </div>
{/if}

<style>
  .settings-overlay { position: fixed; inset: 0; z-index: 18; display: grid; place-items: center; padding: 36px; background: transparent; }
  .shell-overlay { position: absolute; inset: 8px; z-index: 12; min-width: 0; min-height: 0; overflow: hidden; border-radius: 6px; box-shadow: 0 18px 50px rgb(0 0 0 / 38%); }
  .settings-modal { width: min(1100px, calc(100vw - 72px)); height: min(760px, calc(100vh - 72px)); min-width: 0; min-height: 0; overflow: hidden; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--page-bg); box-shadow: 0 24px 70px rgb(0 0 0 / 34%); outline: none; }
  .settings-modal :global(.settings-page) { min-width: 0; }
  .settings-modal :global(.settings-layout) { min-width: 0; min-height: 0; }
  /* 应用更新进度浮层：设置面板关闭后也留在右下角，直到更新流程结束。 */
  .app-update-float { position: fixed; z-index: 39; right: 16px; bottom: 16px; display: grid; gap: 4px; width: min(320px, calc(100vw - 32px)); padding: 10px 12px; border: 1px solid var(--border-strong); border-radius: 8px; color: var(--text); background: var(--surface); box-shadow: 0 12px 34px rgb(0 0 0 / 26%); font-family: var(--text-font); font-size: 12px; }
  .app-update-float :global(.app-update-progress) { margin-top: 0; }
  .app-update-float-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .app-update-float-close { display: inline-flex; align-items: center; justify-content: center; width: 20px; height: 20px; margin: -2px -2px 0 0; padding: 0; border: 1px solid transparent; border-radius: 4px; color: var(--text-muted); background: transparent; cursor: pointer; }
  .app-update-float-close:hover { border-color: var(--border-strong); color: var(--text-strong); background: var(--surface-hover); }
  .app-update-float :global(.quiet-button) { justify-self: start; }
  @media (max-width: 760px), (max-height: 560px) {
    .settings-overlay { padding: 12px; }
    .settings-modal { width: 100%; height: 100%; border-radius: 8px; }
  }
</style>
