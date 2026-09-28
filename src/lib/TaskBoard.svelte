<script lang="ts">
  import {
    AlertTriangle,
    Archive,
    CheckCircle2,
    Inbox,
    Loader2,
    Plus,
    RotateCcw,
    Square,
    Trash2,
  } from "@lucide/svelte";
  import { onMount } from "svelte";
  import type { Project } from "$lib/project";
  import { statusLabels, type Task, type TaskStatus } from "$lib/task";
  import { t } from "$lib/i18n.svelte";

  interface Props {
    tasks: Task[];
    projects: Project[];
    selectedProjectId: string | null;
    activeTaskId: string | null;
    onOpen: (task: Task) => void;
    onStop: (task: Task) => void;
    onRestart: (task: Task) => void;
    onArchive: (task: Task) => void;
    onRestore: (task: Task) => void;
    onDelete: (task: Task) => void;
    onNewTask: () => void;
    filterProjectId?: string | null;
  }

  let {
    tasks,
    projects,
    selectedProjectId,
    activeTaskId,
    onOpen,
    onStop,
    onRestart,
    onArchive,
    onRestore,
    onDelete,
    onNewTask,
    filterProjectId = null,
  }: Props = $props();

  // selectedProjectId / onRestore 只用于父组件接线，看板本身不据此改变展示。

  type ColumnKey = "waiting" | "running" | "interrupted" | "completed";

  interface BoardColumn {
    key: ColumnKey;
    title: string;
    accent: string;
    tasks: Task[];
  }

  /** 纯函数：把毫秒时长格式化成「1 小时 2 分」「3 分 4 秒」「5 秒」。 */
  function formatDuration(milliseconds: number): string {
    const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;
    if (hours > 0) return t("{hours} 小时 {minutes} 分", { hours, minutes });
    if (minutes > 0) return t("{minutes} 分 {seconds} 秒", { minutes, seconds });
    return t("{seconds} 秒", { seconds });
  }

  /** 纯函数：把时间戳格式化成「3 分钟前」「昨天」这类相对时间。 */
  function formatRelativeTime(timestamp: number, now: number): string {
    const elapsed = Math.max(0, now - timestamp);
    const minutes = Math.floor(elapsed / 60_000);
    if (minutes < 1) return t("刚刚");
    if (minutes < 60) return t("{minutes} 分钟前", { minutes });
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return t("{hours} 小时前", { hours });
    const days = Math.floor(hours / 24);
    if (days === 1) return t("昨天");
    if (days < 30) return t("{days} 天前", { days });
    const date = new Date(timestamp);
    return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
  }

  /** 状态色只取现有令牌：等待 / 运行中 / 失败（含取消）/ 已完成。 */
  function statusAccent(status: TaskStatus): string {
    if (status === "running") return "var(--status-running)";
    if (status === "waiting") return "var(--status-waiting)";
    if (status === "completed") return "var(--status-done)";
    return "var(--status-failed)";
  }

  function projectLabel(task: Task): string {
    return projects.find((project) => project.id === task.projectId)?.name ?? task.projectPath;
  }

  let now = $state(Date.now());

  onMount(() => {
    const timer = setInterval(() => {
      now = Date.now();
    }, 1000);
    return () => clearInterval(timer);
  });

  const scopedTasks = $derived(
    filterProjectId ? tasks.filter((task) => task.projectId === filterProjectId) : tasks,
  );
  const boardTasks = $derived(scopedTasks.filter((task) => task.archivedAt === null));
  const archivedCount = $derived(scopedTasks.length - boardTasks.length);

  const waitingTasks = $derived(boardTasks.filter((task) => task.status === "waiting"));
  const runningTasks = $derived(boardTasks.filter((task) => task.status === "running"));
  const interruptedTasks = $derived(
    boardTasks.filter((task) => task.status === "failed" || task.status === "cancelled"),
  );
  const completedTasks = $derived(boardTasks.filter((task) => task.status === "completed"));

  const columns: BoardColumn[] = $derived([
    { key: "waiting", title: t("等待中"), accent: "var(--status-waiting)", tasks: waitingTasks },
    { key: "running", title: t("正在执行"), accent: "var(--status-running)", tasks: runningTasks },
    { key: "interrupted", title: t("中断 / 报错"), accent: "var(--status-failed)", tasks: interruptedTasks },
    { key: "completed", title: t("已完成"), accent: "var(--status-done)", tasks: completedTasks },
  ]);
</script>

<div class="task-board">
  <header class="board-header">
    <div class="board-summary">
      <span class="summary-line">
        {t("共 {total} 个任务 · 运行中 {running} · 等待 {waiting} · 中断 {interrupted} · 已完成 {completed}", {
          total: boardTasks.length,
          running: runningTasks.length,
          waiting: waitingTasks.length,
          interrupted: interruptedTasks.length,
          completed: completedTasks.length,
        })}
      </span>
      <span class="archived-hint">{t("另有 {count} 个已归档任务", { count: archivedCount })}</span>
    </div>
    <button class="new-task" type="button" onclick={onNewTask}>
      <Plus size={15} aria-hidden="true" />
      {t("新建任务")}
    </button>
  </header>

  <div class="board-columns">
    {#each columns as column (column.key)}
      <section
        class="board-column"
        style={`--column-accent: ${column.accent}`}
        aria-label={t("{title}（{count}）", { title: column.title, count: column.tasks.length })}
      >
        <header class="column-header">
          {#if column.key === "waiting"}
            <Inbox size={14} aria-hidden="true" />
          {:else if column.key === "running"}
            <Loader2 size={14} aria-hidden="true" />
          {:else if column.key === "interrupted"}
            <AlertTriangle size={14} aria-hidden="true" />
          {:else}
            <CheckCircle2 size={14} aria-hidden="true" />
          {/if}
          <span class="column-title">{column.title}</span>
          <span class="column-count">{column.tasks.length}</span>
        </header>

        <div class="column-body">
          {#if column.tasks.length === 0}
            <p class="column-empty">{t("暂无任务")}</p>
          {:else}
            {#each column.tasks as task (task.id)}
              <article
                class="task-card"
                class:running={task.status === "running"}
                class:active={task.id === activeTaskId}
                style={`--card-accent: ${statusAccent(task.status)}`}
              >
                <button
                  class="card-main"
                  type="button"
                  aria-label={t("打开任务：{title}", { title: task.title })}
                  onclick={() => onOpen(task)}
                >
                  <span class="card-title">{task.title}</span>
                  <span class="card-project">{projectLabel(task)}</span>
                  <span class="card-meta">
                    <span class="status-badge">{t(statusLabels[task.status])}</span>
                    <span class="card-time">
                      {#if task.status === "running"}
                        <span class="pulse" aria-hidden="true"></span>
                        {t("已运行 {duration}", { duration: formatDuration(now - (task.startedAt ?? task.createdAt)) })}
                      {:else}
                        {formatRelativeTime(task.completedAt ?? task.createdAt, now)}
                      {/if}
                    </span>
                  </span>
                </button>

                <div class="card-actions">
                  {#if task.status === "running"}
                    <button type="button" title={t("停止")} aria-label={t("停止任务：{title}", { title: task.title })} onclick={() => onStop(task)}>
                      <Square size={13} aria-hidden="true" />
                    </button>
                  {:else if task.status === "waiting" || task.status === "failed" || task.status === "cancelled"}
                    <button type="button" title={t("重开")} aria-label={t("重开任务：{title}", { title: task.title })} onclick={() => onRestart(task)}>
                      <RotateCcw size={13} aria-hidden="true" />
                    </button>
                  {:else}
                    <button type="button" title={t("归档")} aria-label={t("归档任务：{title}", { title: task.title })} onclick={() => onArchive(task)}>
                      <Archive size={13} aria-hidden="true" />
                    </button>
                  {/if}
                  <button class="danger" type="button" title={t("删除")} aria-label={t("删除任务：{title}", { title: task.title })} onclick={() => onDelete(task)}>
                    <Trash2 size={13} aria-hidden="true" />
                  </button>
                </div>
              </article>
            {/each}
          {/if}
        </div>
      </section>
    {/each}
  </div>
</div>

<style>
  .task-board {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    min-height: 0;
    padding: 16px;
    background: var(--page-bg);
    color: var(--text);
  }

  .board-header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-shrink: 0;
  }

  .board-summary {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .summary-line {
    color: var(--text-strong);
    font-size: 13px;
  }

  .archived-hint {
    color: var(--text-subtle);
    font-size: 12px;
  }

  .new-task {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    padding: 6px 12px;
    border: 1px solid var(--accent);
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-ink);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .new-task:hover {
    background: color-mix(in srgb, var(--accent) 82%, var(--text-strong));
  }

  .board-columns {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 16px;
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .board-column {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--surface-alt);
    overflow: hidden;
  }

  .column-header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    color: var(--column-accent, var(--text-muted));
  }

  .column-title {
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .column-count {
    margin-left: auto;
    min-width: 22px;
    padding: 1px 7px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-raised);
    color: var(--column-accent, var(--text-muted));
    font-size: 11px;
    text-align: center;
  }

  .column-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    flex: 1;
    min-height: 0;
    padding: 10px;
    overflow: auto;
  }

  .column-empty {
    margin: 6px 4px;
    color: var(--text-subtle);
    font-size: 12px;
    text-align: center;
  }

  .task-card {
    position: relative;
    border: 1px solid var(--border);
    border-radius: 11px;
    background: var(--surface);
    transition: transform 0.14s ease, box-shadow 0.14s ease, border-color 0.14s ease;
  }

  .task-card:hover {
    transform: translateY(-2px);
    border-color: var(--border-strong);
    box-shadow: 0 10px 22px color-mix(in srgb, var(--page-bg) 85%, transparent);
  }

  .task-card.running {
    border-color: color-mix(in srgb, var(--status-running) 40%, transparent);
  }

  .task-card.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .task-card.active:hover {
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent),
      0 10px 22px color-mix(in srgb, var(--page-bg) 85%, transparent);
  }

  .card-main {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    padding: 10px 12px;
    border: 0;
    border-radius: 11px;
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }

  .card-title {
    padding-right: 54px;
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 600;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }

  .card-project {
    color: var(--text-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .card-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: 2px;
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    padding: 1px 7px;
    border: 1px solid color-mix(in srgb, var(--card-accent, var(--border-strong)) 45%, transparent);
    border-radius: 999px;
    background: color-mix(in srgb, var(--card-accent, var(--border-strong)) 14%, transparent);
    color: var(--card-accent, var(--text-muted));
    font-size: 11px;
    white-space: nowrap;
  }

  .card-time {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-subtle);
    font-size: 11px;
    white-space: nowrap;
  }

  .pulse {
    width: 8px;
    height: 8px;
    flex-shrink: 0;
    border-radius: 50%;
    background: var(--status-running);
    animation: board-pulse 1.6s ease-in-out infinite;
  }

  @keyframes board-pulse {
    0% {
      opacity: 1;
      transform: scale(1);
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--status-running) 50%, transparent);
    }
    70% {
      opacity: 0.7;
      transform: scale(0.85);
      box-shadow: 0 0 0 6px color-mix(in srgb, var(--status-running) 0%, transparent);
    }
    100% {
      opacity: 1;
      transform: scale(1);
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--status-running) 0%, transparent);
    }
  }

  .card-actions {
    position: absolute;
    top: 7px;
    right: 7px;
    z-index: 2;
    display: flex;
    gap: 4px;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.12s ease;
  }

  .task-card:hover .card-actions,
  .task-card:focus-within .card-actions {
    opacity: 1;
    pointer-events: auto;
  }

  .card-actions button {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--surface-raised);
    color: var(--text-muted);
    cursor: pointer;
  }

  .card-actions button:hover {
    border-color: var(--accent);
    color: var(--text-strong);
  }

  .card-actions button.danger:hover {
    border-color: color-mix(in srgb, var(--status-failed) 45%, transparent);
    color: var(--status-failed);
  }

  @media (prefers-reduced-motion: reduce) {
    .task-card:hover {
      transform: none;
    }
    .pulse {
      animation: none;
    }
  }
</style>
