<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
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
    PanelTop,
    Plus,
    RotateCcw,
  } from "@lucide/svelte";
  import { onMount } from "svelte";
  import AppDialog from "$lib/AppDialog.svelte";
  import { checkAppUpdate, installAppUpdate, type AppUpdateState, type Update } from "$lib/app-update";
  import type { DialogRequest, DialogValue } from "$lib/dialog";
  import type { Project } from "$lib/project";
  import PiMarketplace from "$lib/PiMarketplace.svelte";
  import PiProviderSettings from "$lib/PiProviderSettings.svelte";
  import PiSettings from "$lib/PiSettings.svelte";
  import TaskSidebar from "$lib/TaskSidebar.svelte";
  import TaskTabs from "$lib/TaskTabs.svelte";
  import TerminalPane from "$lib/TerminalPane.svelte";
  import type { RuntimeComponent, RuntimeUpdate } from "$lib/runtime";
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
  } from "$lib/workspace";

  type LayoutMode = "single" | "split" | "grid";

  interface TaskStatusUpdate {
    taskId: string;
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
  let appUpdate = $state<AppUpdateState>({
    status: "idle",
    version: null,
    notes: null,
    error: null,
  });
  let pendingAppUpdate: Update | null = null;
  let settings = $state<AppSettings>({ ...DEFAULT_APP_SETTINGS });
  let terminalTaskIds = $state<string[]>([]);
  let paneTaskIds = $state<string[]>([]);
  let activeTaskId = $state<string | null>(null);
  let selectedProjectId = $state<string | null>(null);
  let isStarting = $state(false);
  let errorMessage = $state("");
  let layout = $state<LayoutMode>("split");
  let view = $state<"workspace" | "settings" | "market" | "models">("workspace");
  let activeAgent = $state<"pi" | "dsh">("pi");
  let isDshStarting = $state(false);
  let dshHostError = $state<string | null>(null);
  let dialogRequest = $state<DialogRequest | null>(null);
  let isClosing = false;
  let workspace: HTMLElement;
  let dshWebview: Webview | null = null;
  let nextTaskNumber = 1;
  let dialogId = 0;

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
    !isStarting &&
      runningCount < settings.maxConcurrentTasks &&
      selectedProjectId !== null,
  );

  function applyAppearance(next: AppSettings) {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    root.dataset.colorMode = next.colorMode;
    root.dataset.colorScheme = isLightColorMode(next.colorMode) ? "light" : "dark";
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
        [update.id]: Date.now() + 24 * 60 * 60 * 1000,
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
    if (
      !(await confirmDialog(
        "更新组件",
        `下载并激活 ${update.name} ${update.latestVersion} 吗？当前运行时会先备份。`,
        "更新",
      ))
    ) {
      return;
    }
    try {
      const affectsDsh = update.id === "dsh" || update.id === "dshmarket";
      if (affectsDsh) {
        await dshWebview?.close();
        dshWebview = null;
        await invoke("stop_dsh");
      }
      await invoke("install_runtime", {
        request: { componentId: update.id, version: update.latestVersion },
      });
      runtimes = await invoke<RuntimeComponent[]>("runtime_status");
      if (affectsDsh) await invoke("start_dsh");
      await checkUpdates(true);
    } catch (error) {
      showError(error);
    }
  }

  async function rollbackRuntime(update: RuntimeUpdate) {
    if (!update.canRollback) return;
    if (!(await confirmDialog("回滚组件", `恢复 ${update.name} 的上一版本吗？`, "回滚"))) return;
    try {
      const affectsDsh = update.id === "dsh" || update.id === "dshmarket";
      if (affectsDsh) {
        await dshWebview?.close();
        dshWebview = null;
        await invoke("stop_dsh");
      }
      await invoke("rollback_runtime", { request: { componentId: update.id } });
      runtimes = await invoke<RuntimeComponent[]>("runtime_status");
      if (affectsDsh) await invoke("start_dsh");
      await checkUpdates(true);
    } catch (error) {
      showError(error);
    }
  }

  onMount(() => {
    void checkUpdates();
    void checkDeepPiUpdate();
    void Promise.all([
      invoke<string>("default_working_directory"),
      invoke<Task[]>("list_tasks"),
      invoke<Project[]>("list_projects"),
      invoke<RuntimeComponent[]>("runtime_status"),
      invoke<AppSettings>("get_settings"),
    ])
      .then(([cwd, storedTasks, storedProjects, runtimeStatus, storedSettings]) => {
        settings = storedSettings;
        tasks = storedTasks;
        projects = storedProjects;
        runtimes = runtimeStatus;
        nextTaskNumber = storedTasks.length + 1;
        selectedProjectId =
          storedProjects.find((project) => project.path === storedSettings.lastProject)?.id ??
          storedProjects[0]?.id ??
          null;
        if (storedProjects.length === 0) {
          void cwd;
        }
      })
      .catch(showError);

    const systemTheme = window.matchMedia("(prefers-color-scheme: light)");
    const handleSystemThemeChange = () => {
      if (settings.colorMode === "system") applyAppearance(settings);
    };
    systemTheme.addEventListener("change", handleSystemThemeChange);

    const appWindow = getCurrentWindow();
    const closeListener = appWindow.onCloseRequested(async (event) => {
      if (isClosing) return;
      event.preventDefault();

      let behavior: CloseBehavior | null = settings.closeBehavior;
      if (behavior === "ask") {
        behavior = await closeChoiceDialog();
        if (!behavior) return;
        updateSettings({ ...settings, closeBehavior: behavior });
      }
      if (behavior === "minimize") {
        await appWindow.minimize();
        return;
      }

      const hasActiveTasks = tasks.some((task) =>
        ["running", "waiting"].includes(task.status),
      );
      if (
        hasActiveTasks &&
        !(await confirmDialog("退出 DeepPi", "仍有活动任务，停止任务并退出吗？"))
      ) {
        return;
      }
      isClosing = true;
      try {
        await invoke("stop_all_pi_tasks");
        await invoke("stop_dsh");
        await appWindow.destroy();
      } catch (error) {
        isClosing = false;
        showError(error);
      }
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
      if (task && ["running", "waiting"].includes(task.status)) {
        task.status = payload.status;
      }
    });
    const observer = new ResizeObserver(() => void syncDshBounds());
    observer.observe(workspace);

    return () => {
      observer.disconnect();
      systemTheme.removeEventListener("change", handleSystemThemeChange);
      void closeListener.then((unlisten) => unlisten());
      void dshTaskListener.then((unlisten) => unlisten());
      void dshStatusListener.then((unlisten) => unlisten());
      void statusListener.then((unlisten) => unlisten());
      void dshWebview?.close();
    };
  });

  function showError(error: unknown) {
    errorMessage = String(error);
    dialogRequest = {
      id: ++dialogId,
      kind: "alert",
      title: "操作失败",
      message: errorMessage,
      confirmLabel: "知道了",
      resolve: () => {},
    };
  }

  function resolveDialog(value: DialogValue) {
    const request = dialogRequest;
    dialogRequest = null;
    request?.resolve(value);
  }

  function confirmDialog(title: string, message: string, confirmLabel = "确认") {
    return new Promise<boolean>((resolve) => {
      dialogRequest = {
        id: ++dialogId,
        kind: "confirm",
        title,
        message,
        confirmLabel,
        resolve: (value) => resolve(value === true),
      };
    });
  }

  function closeChoiceDialog() {
    return new Promise<CloseBehavior | null>((resolve) => {
      dialogRequest = {
        id: ++dialogId,
        kind: "choice",
        title: "关闭 DeepPi",
        message: "选择点击窗口关闭按钮时的默认行为。这个选择会记住，也可以在设置中修改。",
        choices: [
          { value: "minimize", label: "最小化" },
          { value: "exit", label: "退出应用" },
        ],
        resolve: (value) => resolve(value === "minimize" || value === "exit" ? value : null),
      };
    });
  }

  function updateSettings(next: AppSettings) {
    settings = next;
    void invoke("save_settings", { settings: next }).catch(showError);
  }

  function inputDialog(title: string, message: string, initialValue: string) {
    return new Promise<string | null>((resolve) => {
      dialogRequest = {
        id: ++dialogId,
        kind: "input",
        title,
        message,
        initialValue,
        placeholder: "输入任务名称",
        confirmLabel: "保存",
        resolve: (value) => resolve(typeof value === "string" ? value.trim() : null),
      };
    });
  }

  function applyPaneSelection(selection: { panes: string[]; active: string | null }) {
    paneTaskIds = selection.panes;
    activeTaskId = selection.active;
  }

  function selectProject(project: Project) {
    selectedProjectId = project.id;
    settings = { ...settings, lastProject: project.path };
    void invoke("touch_project", { projectId: project.id }).catch(showError);
    void invoke("save_settings", { settings }).catch(showError);
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
    if (
      !(await confirmDialog(
        "移除项目",
        `从工作区移除“${project.name}”吗？本机目录、任务和会话不会被删除。`,
        "移除",
      ))
    ) {
      return;
    }
    try {
      await invoke("remove_project", { projectId: project.id });
      projects = projects.filter((candidate) => candidate.id !== project.id);
      if (selectedProjectId !== project.id) return;

      const nextProject = projects[0];
      if (nextProject) {
        selectProject(nextProject);
      } else {
        selectedProjectId = null;
        settings = { ...settings, lastProject: null };
        applyPaneSelection({ panes: [], active: null });
        void invoke("save_settings", { settings }).catch(showError);
      }
    } catch (error) {
      showError(error);
    }
  }

  function openTask(task: Task) {
    if (task.agent === "dsh") {
      void showDsh();
      return;
    }
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
    if (isDshStarting) return;
    activeAgent = "dsh";
    view = "workspace";
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
      await dshWebview.setFocus();
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
    activeAgent = "pi";
    view = "workspace";
    await dshWebview?.hide();
  }

  function openSettings() {
    view = "settings";
  }

  function closeSettings() {
    view = "workspace";
  }

  function openMarket() {
    view = "market";
  }

  function openModels() {
    view = "models";
  }

  function closeMarket() {
    view = "settings";
  }

  function closeModels() {
    view = "settings";
  }

  async function startTask(projectId: string | null = selectedProjectId) {
    const project = projects.find((candidate) => candidate.id === projectId);
    if (!project) {
      showError("请先添加并选择一个 Pi 项目目录");
      return;
    }
    if (isStarting || runningCount >= settings.maxConcurrentTasks) return;
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
    isStarting = true;
    errorMessage = "";
    try {
      const title = `Pi Task ${nextTaskNumber++}`;
      const task = await invoke<Task>("start_pi_task", {
        request: { projectId: project.id, title, rows: 32, cols: 100 },
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
    try {
      await invoke("stop_pi_task", { taskId: task.id });
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

  async function restartTask(task: Task) {
    try {
      const restarted = await invoke<Task>("restart_pi_task", {
        request: { taskId: task.id, rows: 32, cols: 100 },
      });
      Object.assign(task, restarted);
      if (!terminalTaskIds.includes(task.id)) terminalTaskIds.push(task.id);
      openTask(task);
    } catch (error) {
      showError(error);
    }
  }

  async function archiveTask(task: Task) {
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
    if (!(await confirmDialog("删除任务", `永久删除“${task.title}”的 DeepPi 任务记录吗？`))) return;
    try {
      await invoke("delete_task", { taskId: task.id });
      tasks = tasks.filter((candidate) => candidate.id !== task.id);
      removeTerminal(task.id);
    } catch (error) {
      showError(error);
    }
  }
</script>

<svelte:head><title>DeepPi</title></svelte:head>

<div class:dsh-mode={activeAgent === "dsh"} class="app-shell">
  <header class="topbar">
    <div class="brand"><Bot size={18} strokeWidth={1.8} /><strong>DeepPi</strong></div>

    <div class="agent-switch" aria-label="Agent">
      <button class:selected={activeAgent === "pi"} type="button" title={runtimeTitle("pi")} onclick={showPi}>Pi</button>
      <button class:selected={activeAgent === "dsh"} type="button" title={runtimeTitle("dsh")} onclick={showDsh}>DSH</button>
    </div>

    {#if activeAgent === "pi"}
      <div class="toolbar">
        <button class:active={layout === "single"} type="button" aria-label="单任务布局" title="单任务布局" onclick={() => changeLayout("single")}>
          <PanelTop size={16} />
        </button>
        <button class:active={layout === "split"} type="button" aria-label="双列布局" title="双列布局" onclick={() => changeLayout("split")}>
          <PanelLeft size={16} />
        </button>
        <button class:active={layout === "grid"} type="button" aria-label="网格布局" title="网格布局" onclick={() => changeLayout("grid")}>
          <LayoutGrid size={16} />
        </button>
        <button class="primary" type="button" aria-label="新建 Pi 任务" title="新建 Pi 任务" disabled={!canStart} onclick={() => void startTask()}>
          <Plus size={17} />
        </button>
      </div>
    {/if}
  </header>

  {#if activeAgent === "pi"}
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
      onOpenSettings={openSettings}
      settingsActive={view !== "workspace"}
    />
  {/if}

  <main class="workspace" bind:this={workspace}>
    {#if activeAgent === "dsh"}
      <div class="dsh-placeholder" aria-live="polite">
        {#if isDshStarting}
          <span>正在启动 DSH</span>
        {:else if dshHostError}
          <span>{dshHostError}</span>
          <button type="button" class="start-button" onclick={showDsh} disabled={isDshStarting}>
            <RotateCcw size={16} />重启 DSH
          </button>
        {/if}
      </div>
    {:else if view === "settings"}
      <PiSettings
        settings={settings}
        runtimes={runtimes}
        updates={updates}
        isCheckingUpdates={isCheckingUpdates}
        {appUpdate}
        onChangeSettings={updateSettings}
        onCheckUpdates={() => void checkUpdates(true)}
        onCheckAppUpdate={() => void checkDeepPiUpdate()}
        onInstallAppUpdate={() => void installDeepPiUpdate()}
        onUpdateRuntime={(update) => void installRuntime(update)}
        onRollbackRuntime={(update) => void rollbackRuntime(update)}
        onSnoozeRuntime={(update) => snoozeRuntime(update)}
        onSkipRuntime={(update) => skipRuntime(update)}
        onClose={closeSettings}
        onOpenModels={openModels}
        onOpenMarket={openMarket}
      />
    {:else if view === "market"}
      <PiMarketplace
        projectPath={selectedProject?.path ?? null}
        confirm={confirmDialog}
        onClose={closeMarket}
        onError={showError}
      />
    {:else if view === "models"}
      <PiProviderSettings
        confirm={confirmDialog}
        onClose={closeModels}
        onError={showError}
      />
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
      />
      <div class="workspace-content">
        <div
          class:single={layout === "single"}
          class:split={layout === "split"}
          class:grid={layout === "grid"}
          class:pane-count-one={paneTaskIds.length === 1}
          class="terminal-grid"
        >
          {#each allTerminalTasks as task (task.id)}
            <TerminalPane
              taskId={task.id}
              title={task.title}
              status={task.status}
              codeFont={settings.codeFont}
              colorMode={settings.colorMode}
              visible={activeAgent === "pi" && task.projectId === selectedProjectId && paneTaskIds.includes(task.id)}
              active={task.id === activeTaskId}
              onExit={(exitCode, error) => markExited(task, exitCode, error)}
            />
          {/each}
        </div>
      </div>
    {/if}
  </main>

  <AppDialog request={dialogRequest} onResolve={resolveDialog} />
</div>
