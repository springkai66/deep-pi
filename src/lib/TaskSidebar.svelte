<script lang="ts">
  import {
    Archive,
    ArchiveRestore,
    ChevronDown,
    ChevronRight,
    Folder,
    FolderPlus,
    Pencil,
    Plus,
    RefreshCw,
    RotateCcw,
    Search,
    X,
    Square,
    Trash2,
  } from "@lucide/svelte";
  import type { Project } from "$lib/project";
  import { onMount } from "svelte";
  import { statusLabels, type Task } from "$lib/task";
  import { matchesSearch } from "$lib/navigation";
  import { shortcutAria } from "$lib/shortcuts";

  interface Props {
    projects: Project[];
    tasks: Task[];
    selectedProjectId: string | null;
    onAddProject: () => void;
    onOpenProject: (project: Project) => void;
    onOpen: (task: Task) => void;
    onRename: (task: Task) => void;
    onStop: (task: Task) => void;
    onArchive: (task: Task) => void;
    onRestart: (task: Task) => void;
    onRestore: (task: Task) => void;
    onDelete: (task: Task) => void;
    onAddSession: (project: Project) => void;
    onRemoveProject: (project: Project) => void;
    searchFocusToken?: number;
  }

  let {
    projects,
    tasks,
    selectedProjectId,
    onAddProject,
    onOpenProject,
    onOpen,
    onRename,
    onStop,
    onArchive,
    onRestart,
    onRestore,
    onDelete,
    onAddSession,
    onRemoveProject,
    searchFocusToken = 0,
  }: Props = $props();

  const activeStatuses = ["queued", "running", "waiting"];
  const completedStatuses = ["completed", "failed", "cancelled"];
  const archivedTasks = $derived(tasks.filter((task) => task.archivedAt !== null));
  let collapsedProjectIds = $state<Set<string>>(new Set());
  let projectMenu = $state<{ project: Project; x: number; y: number } | null>(null);
  let sessionMenu = $state<{ task: Task; x: number; y: number } | null>(null);
  let projectMenuElement = $state<HTMLDivElement>();
  let sessionMenuElement = $state<HTMLDivElement>();
  let searchInput = $state<HTMLInputElement>();
  let searchResults = $state<HTMLDivElement>();
  let searchText = $state("");
  let query = $state("");
  let composing = false;
  const searchMatches = $derived(tasks.filter((task) => matchesSearch(task.title, query)));

  $effect(() => {
    if (searchFocusToken > 0) {
      searchInput?.focus();
      searchInput?.select();
    }
  });

  function searchKeydown(event: KeyboardEvent) {
    if (event.isComposing || composing) return;
    if (event.key === "Escape") {
      event.preventDefault();
      searchText = query = "";
      searchInput?.focus();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const buttons = Array.from(searchResults?.querySelectorAll<HTMLButtonElement>("button") ?? []);
      if (!buttons.length) return;
      event.preventDefault();
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = index < 0
        ? (event.key === "ArrowDown" ? 0 : buttons.length - 1)
        : (index + (event.key === "ArrowDown" ? 1 : buttons.length - 1)) % buttons.length;
      buttons[next]?.focus();
    } else if (event.key === "Enter" && event.target === searchInput && query.trim() && searchMatches[0]) {
      event.preventDefault();
      onOpen(searchMatches[0]);
    }
  }

  onMount(() => {
    const dismiss = () => {
      projectMenu = null;
      sessionMenu = null;
    };
    document.addEventListener("click", dismiss);
    return () => document.removeEventListener("click", dismiss);
  });

  $effect(() => {
    if (projectMenu) projectMenuElement?.focus();
    if (sessionMenu) sessionMenuElement?.focus();
  });

  function showProjectMenu(event: MouseEvent, project: Project) {
    event.preventDefault();
    event.stopPropagation();
    sessionMenu = null;
    const menuWidth = 196;
    const menuHeight = 44;
    projectMenu = {
      project,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - menuWidth - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - menuHeight - 8)),
    };
  }

  function showSessionMenu(event: MouseEvent, task: Task) {
    event.preventDefault();
    event.stopPropagation();
    projectMenu = null;
    const menuWidth = 196;
    const menuHeight = 160;
    sessionMenu = {
      task,
      x: Math.max(8, Math.min(event.clientX, window.innerWidth - menuWidth - 8)),
      y: Math.max(8, Math.min(event.clientY, window.innerHeight - menuHeight - 8)),
    };
  }

  function chooseSessionAction(action: "archive" | "restore" | "rename" | "stop" | "restart" | "remove") {
    const task = sessionMenu?.task;
    sessionMenu = null;
    if (!task) return;
    if (action === "archive") onArchive(task);
    else if (action === "restore") onRestore(task);
    else if (action === "rename") onRename(task);
    else if (action === "stop") onStop(task);
    else if (action === "restart") onRestart(task);
    else onDelete(task);
  }

  function handleSessionMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      sessionMenu = null;
    }
  }

  function chooseRemoveProject() {
    const project = projectMenu?.project;
    projectMenu = null;
    if (project) onRemoveProject(project);
  }

  function handleProjectMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      projectMenu = null;
    }
  }

  function projectExpanded(projectId: string) {
    return projectId === selectedProjectId && !collapsedProjectIds.has(projectId);
  }

  function openOrToggleProject(project: Project) {
    if (project.id !== selectedProjectId) {
      const next = new Set(collapsedProjectIds);
      next.delete(project.id);
      collapsedProjectIds = next;
      onOpenProject(project);
      return;
    }
    const next = new Set(collapsedProjectIds);
    if (next.has(project.id)) next.delete(project.id);
    else next.add(project.id);
    collapsedProjectIds = next;
  }

  function projectTasks(projectId: string, statuses: string[]) {
    return tasks.filter(
      (task) =>
        task.projectId === projectId &&
        task.archivedAt === null &&
        statuses.includes(task.status),
    );
  }

  function projectLabel(project: Project) {
    return project.path.length > 34 ? `...${project.path.slice(-31)}` : project.path;
  }
</script>

<aside class="task-sidebar" aria-label="工作区">
  <div class="task-search" role="search">
    <Search size={14} aria-hidden="true" />
    <input bind:this={searchInput} bind:value={searchText} type="search"
      aria-label="搜索任务" aria-keyshortcuts={shortcutAria("tasks")} placeholder="搜索任务"
      onkeydown={searchKeydown}
      oncompositionstart={() => { composing = true; }}
      oncompositionend={() => { composing = false; query = searchText; }}
      oninput={(event) => { if (!composing) query = event.currentTarget.value; }} />
    {#if searchText}
      <button type="button" title="清空搜索" aria-label="清空搜索"
        onclick={() => { searchText = query = ""; searchInput?.focus(); }}><X size={14} /></button>
    {/if}
  </div>
  {#if query.trim()}
    <div class="task-search-results" bind:this={searchResults} role="group" aria-label="任务搜索结果">
      <p role="status">{searchMatches.length ? `${searchMatches.length} 个任务` : "没有匹配的任务"}</p>
      {#each searchMatches as task (task.id)}
        <button type="button" class="search-result" onclick={() => onOpen(task)} onkeydown={searchKeydown}>
          <strong>{task.title}</strong>
          <small>{projects.find((project) => project.id === task.projectId)?.name ?? task.projectPath} · {task.archivedAt !== null ? "归档" : statusLabels[task.status]}</small>
        </button>
      {/each}
    </div>
  {:else}
  <section class="projects-section">
    <header class="projects-header">
      <span>工作区</span>
      <button type="button" aria-label="添加项目目录" title="添加项目目录" onclick={onAddProject}>
        <FolderPlus size={15} />
      </button>
    </header>

    {#if projects.length === 0}
      <button class="add-project-empty" type="button" onclick={onAddProject}>
        <FolderPlus size={16} />
        <span>添加项目目录</span>
      </button>
    {:else}
      {#each projects as project (project.id)}
        {@const activeTasks = projectTasks(project.id, activeStatuses)}
        {@const completedTasks = projectTasks(project.id, completedStatuses)}
        <section class:selected={project.id === selectedProjectId} class="project-group">
          <div class="project-header" role="presentation" oncontextmenu={(event) => showProjectMenu(event, project)}>
            <button
              class="project-open"
              type="button"
              aria-expanded={projectExpanded(project.id)}
              title={project.path}
              onclick={() => openOrToggleProject(project)}
            >
              {#if projectExpanded(project.id)}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
              <Folder size={15} />
              <span class="project-copy">
                <strong>{project.name}</strong>
                <small>{projectLabel(project)}</small>
              </span>
            </button>
            <button
              class="project-add"
              type="button"
              aria-label={`在 ${project.name} 中新建 Session`}
              title="新建 Session"
              onclick={() => onAddSession(project)}
            >
              <Plus size={15} />
            </button>
          </div>

          {#if projectExpanded(project.id)}
            <section class="project-task-section">
              <header><span>进行中</span><span>{activeTasks.length}</span></header>
              {#each activeTasks as task (task.id)}
                <div class="task-row" role="presentation" oncontextmenu={(event) => showSessionMenu(event, task)}>
                  {#if task.status === "running"}
                    <RefreshCw class="session-status running" size={13} aria-hidden="true" />
                  {:else}
                    <span class:failed={task.status === "failed"} class:waiting={task.status === "waiting"} class:cancelled={task.status === "cancelled"} class="session-status" aria-hidden="true"></span>
                  {/if}
                  <button class="task-copy" type="button" onclick={() => onOpen(task)}>
                    <strong>{task.title}</strong>
                    <span>{statusLabels[task.status]}</span>
                  </button>
                </div>
              {/each}
            </section>

            <section class="project-task-section">
              <header><span>已完成</span><span>{completedTasks.length}</span></header>
              {#each completedTasks as task (task.id)}
                <div class="task-row" role="presentation" oncontextmenu={(event) => showSessionMenu(event, task)}>
                  {#if task.status === "running"}
                    <RefreshCw class="session-status running" size={13} aria-hidden="true" />
                  {:else}
                    <span class:failed={task.status === "failed"} class:waiting={task.status === "waiting"} class:cancelled={task.status === "cancelled"} class="session-status" aria-hidden="true"></span>
                  {/if}
                  <button class="task-copy" type="button" onclick={() => onOpen(task)}>
                    <strong>{task.title}</strong>
                    <span>{statusLabels[task.status]}</span>
                  </button>
                </div>
              {/each}
            </section>
          {/if}
        </section>
      {/each}
    {/if}
  </section>

  <section class="task-section archived-section">
    <header><span>归档</span><span>{archivedTasks.length}</span></header>
    {#each archivedTasks as task (task.id)}
      <div class="task-row" role="presentation" oncontextmenu={(event) => showSessionMenu(event, task)}>
        {#if task.status === "running"}
          <RefreshCw class="session-status running" size={13} aria-hidden="true" />
        {:else}
          <span class:failed={task.status === "failed"} class:waiting={task.status === "waiting"} class:cancelled={task.status === "cancelled"} class="session-status" aria-hidden="true"></span>
        {/if}
        <button class="task-copy" type="button" onclick={() => onOpen(task)}>
          <strong>{task.title}</strong>
          <span>{statusLabels[task.status]}</span>
        </button>
      </div>
    {/each}
  </section>

  {/if}

  {#if projectMenu}
    <div
      bind:this={projectMenuElement}
      class="context-menu"
      role="menu"
      tabindex="-1"
      aria-label="项目菜单"
      style={`left: ${projectMenu.x}px; top: ${projectMenu.y}px`}
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleProjectMenuKeydown}
    >
      <button type="button" role="menuitem" onclick={chooseRemoveProject}>
        <Trash2 size={14} />从工作区移除
      </button>
    </div>
  {/if}

  {#if sessionMenu}
    <div
      bind:this={sessionMenuElement}
      class="context-menu"
      role="menu"
      tabindex="-1"
      aria-label="Session 菜单"
      style={`left: ${sessionMenu.x}px; top: ${sessionMenu.y}px`}
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleSessionMenuKeydown}
    >
      <button type="button" role="menuitem" onclick={() => chooseSessionAction("rename")}>
        <Pencil size={14} />重命名
      </button>
      {#if activeStatuses.includes(sessionMenu.task.status)}
        <button type="button" role="menuitem" onclick={() => chooseSessionAction("stop")}>
          <Square size={14} />停止
        </button>
      {:else if sessionMenu.task.archivedAt === null}
        <button type="button" role="menuitem" onclick={() => chooseSessionAction("restart")}>
          <RotateCcw size={14} />重启
        </button>
      {/if}
      {#if sessionMenu.task.archivedAt === null}
        <button type="button" role="menuitem" onclick={() => chooseSessionAction("archive")}>
          <Archive size={14} />归档
        </button>
      {:else}
        <button type="button" role="menuitem" onclick={() => chooseSessionAction("restore")}>
          <ArchiveRestore size={14} />恢复
        </button>
      {/if}
      <button type="button" role="menuitem" onclick={() => chooseSessionAction("remove")}>
        <Trash2 size={14} />移除 Session
      </button>
    </div>
  {/if}

</aside>

<style>
  .task-search { display: flex; align-items: center; gap: 6px; margin: 12px 10px 4px; padding: 5px 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text-muted); flex-shrink: 0; }
  .task-search input { min-width: 0; width: 100%; border: 0; background: transparent; color: var(--text); }
  .task-search button { display: grid; place-items: center; border: 0; background: transparent; color: var(--text-muted); cursor: pointer; }
  .task-search-results { padding: 8px; overflow: auto; }
  .task-search-results p { color: var(--text-muted); margin: 4px 4px 12px; }
  .search-result { display: flex; flex-direction: column; gap: 5px; width: 100%; padding: 10px 8px; border: 0; border-radius: 4px; text-align: left; background: transparent; color: var(--text); cursor: pointer; overflow-wrap: anywhere; }
  .search-result:hover, .search-result:focus-visible { background: var(--surface-hover); }
  .search-result small { color: var(--text-muted); }
</style>
