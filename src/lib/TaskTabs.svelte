<script lang="ts">
  import {
    Archive,
    ArchiveRestore,
    Pencil,
    Plus,
    RefreshCw,
    RotateCcw,
    SplitSquareHorizontal,
    SplitSquareVertical,
    Square,
    Trash2,
    X,
  } from "@lucide/svelte";
  import { onMount } from "svelte";
  import type { Task } from "$lib/task";
  import { t } from "$lib/i18n.svelte";

  interface Props {
    tasks: Task[];
    activeTaskId: string | null;
    onOpen: (task: Task) => void;
    onRename: (task: Task) => void;
    onStop: (task: Task) => void;
    onArchive: (task: Task) => void;
    onRestart: (task: Task) => void;
    onRestore: (task: Task) => void;
    onDelete: (task: Task) => void;
    onClose: (task: Task) => void;
    onAdd: () => void;
    onSplit: (task: Task, direction: "right" | "down") => void;
    /** 当前是否分栏显示（双列/网格）；用于在菜单里提供「单栏显示」。 */
    splitActive?: boolean;
    onUnsplit?: () => void;
  }

  let {
    tasks,
    activeTaskId,
    onOpen,
    onRename,
    onStop,
    onArchive,
    onRestart,
    onRestore,
    onDelete,
    onClose,
    onAdd,
    onSplit,
    splitActive = false,
    onUnsplit = () => {},
  }: Props = $props();
  let contextMenu = $state<{ task: Task; x: number; y: number } | null>(null);
  let contextMenuElement = $state<HTMLDivElement>();
  const activeStatuses = ["running", "waiting"];

  onMount(() => {
    const dismiss = () => {
      contextMenu = null;
    };
    document.addEventListener("click", dismiss);
    return () => document.removeEventListener("click", dismiss);
  });

  $effect(() => {
    if (contextMenu) contextMenuElement?.focus();
  });

  function showContextMenu(event: MouseEvent, task: Task) {
    event.preventDefault();
    event.stopPropagation();
    const menuWidth = 196;
    const menuHeight = 220;
    contextMenu = {
      task,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - menuWidth - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - menuHeight - 8)),
    };
  }

  function chooseAction(
    action: "archive" | "restore" | "rename" | "stop" | "restart" | "remove" | "splitRight" | "splitDown" | "unsplit",
  ) {
    const task = contextMenu?.task;
    contextMenu = null;
    if (!task) return;
    if (action === "archive") onArchive(task);
    else if (action === "restore") onRestore(task);
    else if (action === "rename") onRename(task);
    else if (action === "stop") onStop(task);
    else if (action === "restart") onRestart(task);
    else if (action === "splitRight") onSplit(task, "right");
    else if (action === "splitDown") onSplit(task, "down");
    else if (action === "unsplit") onUnsplit();
    else onDelete(task);
  }

  function handleContextMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      contextMenu = null;
    }
  }
</script>

<div class="task-tabs-shell">
  <nav class="task-tabs" aria-label={t("Session 标签")}>
    {#each tasks as task (task.id)}
      <div
        class="session-tab"
        class:active={task.id === activeTaskId}
        role="presentation"
        oncontextmenu={(event) => showContextMenu(event, task)}
      >
        {#if task.status === "running"}
          <!-- 只有真正在执行的会话转圈；waiting（响应已结束/已停止，等待输入）用黄点，
               completed 绿点、failed 红点、cancelled 灰点，避免停止后的标签一直旋转。 -->
          <RefreshCw class="session-status running" size={10} aria-hidden="true" />
        {:else}
          <span class:failed={task.status === "failed"} class:waiting={task.status === "waiting"} class:cancelled={task.status === "cancelled"} class="session-status" aria-hidden="true"></span>
        {/if}
        <button
          type="button"
          class="session-tab-label"
          title={task.title}
          onclick={() => onOpen(task)}
        >
          <span>{task.title}</span>
        </button>
        <button
          type="button"
          class="session-tab-close"
          aria-label={t("关闭 {title}", { title: task.title })}
          title={t("关闭 Session")}
          onclick={(event) => { event.stopPropagation(); onClose(task); }}
        >
          <X size={13} aria-hidden="true" />
        </button>
      </div>
    {/each}
    <button type="button" class="session-tab-add" title={t("新建 Session")} aria-label={t("新建 Session")} onclick={onAdd}>
      <Plus size={15} aria-hidden="true" />
    </button>
  </nav>

  {#if contextMenu}
    <div
      bind:this={contextMenuElement}
      class="context-menu"
      role="menu"
      tabindex="-1"
      aria-label={t("Session 菜单")}
      style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px`}
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleContextMenuKeydown}
    >
      <button type="button" role="menuitem" onclick={() => chooseAction("rename")}>
        <Pencil size={14} />{t("重命名")}
      </button>
      {#if activeStatuses.includes(contextMenu.task.status)}
        <button type="button" role="menuitem" onclick={() => chooseAction("stop")}>
          <Square size={14} />{t("停止")}
        </button>
      {:else if contextMenu.task.archivedAt === null}
        <button type="button" role="menuitem" onclick={() => chooseAction("restart")}>
          <RotateCcw size={14} />{t("重启")}
        </button>
      {/if}
      {#if contextMenu.task.archivedAt === null}
        <button type="button" role="menuitem" onclick={() => chooseAction("archive")}>
          <Archive size={14} />{t("归档")}
        </button>
      {:else}
        <button type="button" role="menuitem" onclick={() => chooseAction("restore")}>
          <ArchiveRestore size={14} />{t("恢复")}
        </button>
      {/if}
      <button type="button" role="menuitem" onclick={() => chooseAction("splitRight")}>
        <SplitSquareHorizontal size={14} />{t("向右分割窗口")}
      </button>
      <button type="button" role="menuitem" onclick={() => chooseAction("splitDown")}>
        <SplitSquareVertical size={14} />{t("向下分割窗口")}
      </button>
      {#if splitActive}
        <button type="button" role="menuitem" onclick={() => chooseAction("unsplit")}>
          <Square size={14} />{t("单栏显示")}
        </button>
      {/if}
      <button type="button" role="menuitem" onclick={() => chooseAction("remove")}>
        <Trash2 size={14} />{t("移除 Session")}
      </button>
    </div>
  {/if}
</div>

<style>
  .task-tabs-shell {
    min-width: 0;
    padding: 4px 8px;
  }

  /* 药丸标签：胶囊式会话切换条，去掉了原先的整行直角标签。 */
  .task-tabs {
    display: flex;
    gap: 4px;
    min-width: 0;
    height: auto;
    padding: 3px;
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: color-mix(in srgb, var(--page-bg) 55%, transparent);
    scrollbar-width: thin;
  }

  .session-tab {
    display: grid;
    grid-template-columns: 14px minmax(60px, 140px) 18px;
    align-items: center;
    gap: 6px;
    min-width: 128px;
    height: 28px;
    padding: 0 4px 0 9px;
    border: 0;
    border-radius: 6px;
    color: var(--text-muted);
    background: transparent;
  }

  .session-tab:hover {
    color: var(--text);
    background: var(--surface-hover);
  }

  .session-tab.active {
    color: var(--text-strong);
    font-weight: 600;
    background: var(--surface-raised);
    box-shadow: 0 2px 8px #0006;
  }

  .session-tab-label {
    display: block;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    color: inherit;
    background: transparent;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .session-tab-label > span {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .session-tab-close {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 3px;
    color: inherit;
    background: transparent;
    cursor: pointer;
  }

  .session-tab-close:hover {
    color: var(--text-strong);
    background: var(--surface-hover);
  }

  .session-tab-add {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    color: var(--text-muted);
    background: transparent;
    cursor: pointer;
  }

  .session-tab-add:hover {
    color: var(--text-strong);
    background: var(--surface-hover);
  }
</style>
