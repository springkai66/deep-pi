<script lang="ts">
  // 任务看板浮窗：贴着主窗口右侧的独立 OS 窗口（无边框、可拖动、自带关闭按钮）。
  // 这里只负责数据与呈现；所有卡片动作通过 board-action 事件委托给主窗口执行，
  // 终端窗格、模式切换器与确认对话框都只存在于主窗口，浮窗不重复这套状态。
  import { invoke } from "@tauri-apps/api/core";
  import { emit, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { LayoutGrid, X } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { applyAppearance } from "$lib/appearance";
  import { BOARD_ACTION_EVENT, type BoardActionRequest } from "$lib/board-actions";
  import { t, tm } from "$lib/i18n.svelte";
  import type { Project } from "$lib/project";
  import type { AppSettings } from "$lib/settings";
  import TaskBoard from "$lib/TaskBoard.svelte";
  import type { Task } from "$lib/task";

  let tasks = $state<Task[]>([]);
  let projects = $state<Project[]>([]);
  let loadError = $state("");
  let reloadTimer: ReturnType<typeof setTimeout> | null = null;

  const piTasks = $derived(tasks.filter((task) => task.agent === "pi"));

  async function reload() {
    try {
      const [nextTasks, nextProjects] = await Promise.all([
        invoke<Task[]>("list_tasks"),
        invoke<Project[]>("list_projects"),
      ]);
      tasks = nextTasks;
      projects = nextProjects;
      loadError = "";
    } catch (cause) {
      loadError = tm(String(cause));
    }
  }

  /// 事件驱动的重载合并到一次请求；动作之后延迟一拍，给主窗口留出执行时间。
  function scheduleReload(delay = 160) {
    if (reloadTimer) return;
    reloadTimer = setTimeout(() => {
      reloadTimer = null;
      void reload();
    }, delay);
  }

  function request(action: BoardActionRequest["action"], task?: Task) {
    const payload: BoardActionRequest = {
      action,
      taskId: task?.id,
      projectId: task?.projectId ?? undefined,
    };
    void emit(BOARD_ACTION_EVENT, payload);
    scheduleReload(400);
  }

  async function refreshAppearance() {
    try {
      applyAppearance(await invoke<AppSettings>("get_settings"));
    } catch {
      // 设置读取失败时保留当前外观，不打断看板使用。
    }
  }

  onMount(() => {
    void (async () => {
      try {
        await invoke<void>("await_startup");
        applyAppearance(await invoke<AppSettings>("get_settings"));
        await getCurrentWindow().setTitle(t("任务看板"));
        await reload();
      } catch (cause) {
        loadError = tm(String(cause));
      }
    })();
    const unlisteners = [
      listen("task-status", () => scheduleReload()),
      listen("rpc-task-exit", () => scheduleReload()),
    ];
    // 重新聚焦看板时同步一次设置与任务：主窗口改了主题/语言，浮窗跟着变。
    void getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused) return;
      scheduleReload(80);
      void refreshAppearance();
    });
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((dispose) => dispose());
      if (reloadTimer) { clearTimeout(reloadTimer); reloadTimer = null; }
    };
  });
  async function closeBoard() {
    try { await getCurrentWindow().close(); } catch { /* 窗口已销毁时忽略 */ }
  }

  /// 无边框窗口没有系统缩放边框：边缘/角落放隐形手柄，按下即进入 OS 级八方向缩放。
  type ResizeDirection = "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";

  function startResize(direction: ResizeDirection) {
    void getCurrentWindow().startResizeDragging(direction).catch(() => { /* 窗口已销毁时忽略 */ });
  }
</script>

<svelte:head><title>{t("任务看板")}</title></svelte:head>

<div class="board-window" class:failed={!!loadError}>
  <header class="board-titlebar" data-tauri-drag-region>
    <LayoutGrid size={14} data-tauri-drag-region aria-hidden="true" />
    <strong data-tauri-drag-region>{t("任务看板")}</strong>
    <button type="button" class="board-close" aria-label={t("关闭任务看板")} title={t("关闭任务看板")}
      onclick={() => void closeBoard()}><X size={15} aria-hidden="true" /></button>
  </header>
  <div class="board-body">
    {#if loadError}
      <p class="board-load-error" role="alert">{tm(loadError)}</p>
      <button type="button" class="board-retry" onclick={() => void reload()}>{t("重试")}</button>
    {:else}
      <TaskBoard
        tasks={piTasks}
        {projects}
        selectedProjectId={null}
        activeTaskId={null}
        onOpen={(task) => request("open", task)}
        onStop={(task) => request("stop", task)}
        onRestart={(task) => request("restart", task)}
        onArchive={(task) => request("archive", task)}
        onRestore={(task) => request("restore", task)}
        onDelete={(task) => request("delete", task)}
        onNewTask={() => request("new-task")}
      />
    {/if}
  </div>
  <!-- 八方向缩放手柄：隐形贴边条，按下交给 OS 进入系统级缩放。 -->
  <div class="resize-handle n" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("North"); }}></div>
  <div class="resize-handle s" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("South"); }}></div>
  <div class="resize-handle w" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("West"); }}></div>
  <div class="resize-handle e" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("East"); }}></div>
  <div class="resize-handle nw" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("NorthWest"); }}></div>
  <div class="resize-handle ne" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("NorthEast"); }}></div>
  <div class="resize-handle sw" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("SouthWest"); }}></div>
  <div class="resize-handle se" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("SouthEast"); }}></div>
</div>
<style>
  /* 浮窗是独立 OS 窗口：标题栏自绘（可拖动），下方整块交给看板内容。 */
  .board-window {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 100vw;
    height: 100vh;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--page-bg);
    color: var(--text);
  }

  .board-titlebar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    min-height: 34px;
    padding: 4px 6px 4px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    user-select: none;
    cursor: default;
  }

  .board-titlebar strong {
    flex: 1;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-strong);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .board-close {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    color: var(--text-muted);
    background: transparent;
    cursor: pointer;
  }

  .board-close:hover {
    color: var(--text-strong);
    background: var(--surface-hover);
  }

  .board-body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .board-load-error {
    margin: 12px 16px 0;
    color: #d46b61;
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  .board-retry {
    margin: 8px 16px;
    padding: 6px 10px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: var(--surface-raised);
    color: var(--text);
    cursor: pointer;
  }

  /* 窄浮窗里四列看板放不下：两行两列铺开，卡片保持可读宽度。
     :global 只在浮窗页面生效，应用内全宽看板仍是四列。 */
  .board-body :global(.board-columns) {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    padding: 10px;
  }

  .board-body :global(.task-board) {
    padding: 0;
  }

  .board-body :global(.board-header) {
    padding: 10px 10px 0;
  }

  /* 八方向缩放手柄：贴边隐形条，只负责命中缩放手势与光标提示。 */
  .resize-handle {
    position: absolute;
    z-index: 40;
    touch-action: none;
  }

  .resize-handle.n { top: 0; left: 12px; right: 12px; height: 5px; cursor: ns-resize; }
  .resize-handle.s { bottom: 0; left: 12px; right: 12px; height: 5px; cursor: ns-resize; }
  .resize-handle.w { left: 0; top: 12px; bottom: 12px; width: 5px; cursor: ew-resize; }
  .resize-handle.e { right: 0; top: 12px; bottom: 12px; width: 5px; cursor: ew-resize; }
  .resize-handle.nw { left: 0; top: 0; width: 12px; height: 12px; cursor: nwse-resize; }
  .resize-handle.ne { right: 0; top: 0; width: 12px; height: 12px; cursor: nesw-resize; }
  .resize-handle.sw { left: 0; bottom: 0; width: 12px; height: 12px; cursor: nesw-resize; }
  .resize-handle.se { right: 0; bottom: 0; width: 12px; height: 12px; cursor: nwse-resize; }
</style>
