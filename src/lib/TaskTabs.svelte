<script lang="ts">
  import {
    Archive,
    ArchiveRestore,
    Pencil,
    RefreshCw,
    RotateCcw,
    Square,
    Trash2,
  } from "@lucide/svelte";
  import { onMount } from "svelte";
  import type { Task } from "$lib/task";

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
    const menuHeight = 160;
    contextMenu = {
      task,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - menuWidth - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - menuHeight - 8)),
    };
  }

  function chooseAction(action: "archive" | "restore" | "rename" | "stop" | "restart" | "remove") {
    const task = contextMenu?.task;
    contextMenu = null;
    if (!task) return;
    if (action === "archive") onArchive(task);
    else if (action === "restore") onRestore(task);
    else if (action === "rename") onRename(task);
    else if (action === "stop") onStop(task);
    else if (action === "restart") onRestart(task);
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
  <nav class="task-tabs" aria-label="Session 标签">
    {#each tasks as task (task.id)}
      <button
        class:active={task.id === activeTaskId}
        type="button"
        title={task.title}
        onclick={() => onOpen(task)}
        oncontextmenu={(event) => showContextMenu(event, task)}
      >
        {#if activeStatuses.includes(task.status)}
          <RefreshCw class="session-status running" size={13} aria-hidden="true" />
        {:else}
          <span class:failed={task.status === "failed"} class:cancelled={task.status === "cancelled"} class="session-status" aria-hidden="true"></span>
        {/if}
        <span>{task.title}</span>
      </button>
    {/each}
  </nav>

  {#if contextMenu}
    <div
      bind:this={contextMenuElement}
      class="context-menu"
      role="menu"
      tabindex="-1"
      aria-label="Session 菜单"
      style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px`}
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleContextMenuKeydown}
    >
      <button type="button" role="menuitem" onclick={() => chooseAction("rename")}>
        <Pencil size={14} />重命名
      </button>
      {#if activeStatuses.includes(contextMenu.task.status)}
        <button type="button" role="menuitem" onclick={() => chooseAction("stop")}>
          <Square size={14} />停止
        </button>
      {:else if contextMenu.task.archivedAt === null}
        <button type="button" role="menuitem" onclick={() => chooseAction("restart")}>
          <RotateCcw size={14} />重启
        </button>
      {/if}
      {#if contextMenu.task.archivedAt === null}
        <button type="button" role="menuitem" onclick={() => chooseAction("archive")}>
          <Archive size={14} />归档
        </button>
      {:else}
        <button type="button" role="menuitem" onclick={() => chooseAction("restore")}>
          <ArchiveRestore size={14} />恢复
        </button>
      {/if}
      <button type="button" role="menuitem" onclick={() => chooseAction("remove")}>
        <Trash2 size={14} />移除 Session
      </button>
    </div>
  {/if}
</div>

<style>
  .task-tabs-shell {
    min-width: 0;
  }

  .task-tabs {
    display: flex;
    min-width: 0;
    height: 32px;
    overflow-x: auto;
    border-bottom: 1px solid #303832;
    background: #161a17;
    scrollbar-width: thin;
  }

  .task-tabs > button {
    display: grid;
    grid-template-columns: 14px minmax(60px, 140px);
    align-items: center;
    gap: 6px;
    min-width: 110px;
    height: 31px;
    padding: 0 9px;
    border: 0;
    border-right: 1px solid #303832;
    color: #89928b;
    background: transparent;
    cursor: pointer;
  }

  .task-tabs > button:hover {
    color: #d8ded9;
    background: #202521;
  }

  .task-tabs > button.active {
    color: #f4f7f5;
    background: #252b27;
    box-shadow: inset 0 -2px #8fd6ad;
  }

  .task-tabs > button > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
