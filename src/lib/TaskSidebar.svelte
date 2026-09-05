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
    Settings2,
    Square,
    Trash2,
  } from "@lucide/svelte";
  import type { Project } from "$lib/project";
  import { onMount } from "svelte";
  import { statusLabels, type Task } from "$lib/task";

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
    onOpenSettings: () => void;
    settingsActive: boolean;
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
    onOpenSettings,
    settingsActive,
  }: Props = $props();

  const activeStatuses = ["queued", "running", "waiting"];
  const completedStatuses = ["completed", "failed", "cancelled"];
  const archivedTasks = $derived(tasks.filter((task) => task.archivedAt !== null));
  let collapsedProjectIds = $state<Set<string>>(new Set());
  let projectMenu = $state<{ project: Project; x: number; y: number } | null>(null);
  let sessionMenu = $state<{ task: Task; x: number; y: number } | null>(null);
  let projectMenuElement = $state<HTMLDivElement>();
  let sessionMenuElement = $state<HTMLDivElement>();

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

  <footer class="sidebar-footer">
    <button
      class:active={settingsActive}
      type="button"
      aria-current={settingsActive ? "page" : undefined}
      onclick={onOpenSettings}
    >
      <Settings2 size={15} />
      <span>设置</span>
    </button>
  </footer>

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
