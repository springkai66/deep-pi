<script lang="ts">
  import { Play, Square } from "@lucide/svelte";
  import { t, tm } from "$lib/i18n.svelte";
  import { statusLabels, type Task } from "$lib/task";
  import { canContinuePetTask, canStopPetTask, type PetTaskAction } from "$lib/pet-task-actions";
  import type { PetTaskBubblesState } from "$lib/pet-task-bubbles";

  let { state, onAction, onRetry }: {
    state: PetTaskBubblesState;
    onAction: (action: PetTaskAction, task: Task) => void;
    onRetry: () => void;
  } = $props();

  const timeLabel = $derived.by(() => {
    void state.tasks;
    return new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  });
</script>

<section class="task-bubbles" aria-label={t("桌宠任务")}
  onpointerdown={(event) => event.stopPropagation()}
  oncontextmenu={(event) => { event.preventDefault(); event.stopPropagation(); }}>
  <header class="bubbles-header">
    <strong>{t("执行中的任务")}</strong>
    <span>{timeLabel}</span>
  </header>
  {#if state.error}
    <div class="task-notice" role="alert">
      <span>{tm(state.error)}</span>
      <button type="button" onclick={onRetry}>{t("重试")}</button>
    </div>
  {/if}
  {#if state.loading}
    <p class="task-notice" role="status">{t("正在读取任务…")}</p>
  {:else if !state.tasks.length && !state.error}
    <p class="task-notice" role="status">{t("现在没有执行中的任务")}</p>
  {/if}
  <div class="task-list">
    {#each state.tasks as task (task.id)}
      {@const busy = state.pending.includes(task.id)}
      <article class="task-bubble" aria-label={task.title} aria-busy={busy}>
        <span class="task-dot task-dot-{task.status}" aria-hidden="true"></span>
        <div class="task-text">
          <strong class="task-title" title={task.title}>{task.title || t("未命名任务")}</strong>
          <span class="task-meta">
            {task.agent.toUpperCase()} · {t(task.status === "cancelled" ? "已停止" : statusLabels[task.status])}
            {#if busy} · {t("正在处理…")}{/if}
            {#if task.agent === "dsh"} · {t("DSH 任务请在主窗口操作")}{/if}
          </span>
        </div>
        <div class="task-actions">
          <button type="button" class="icon-button" disabled={busy || !canStopPetTask(task)}
            aria-label={t("停止任务：{title}", { title: task.title })}
            title={t(task.agent === "dsh" ? "DSH 任务请在主窗口操作" : "停止当前执行，保留会话")}
            onclick={() => onAction("stop", task)}>
            <Square size={12} aria-hidden="true" />
          </button>
          <button type="button" class="icon-button" disabled={busy || !canContinuePetTask(task)}
            aria-label={t("继续任务：{title}", { title: task.title })}
            title={t(task.agent === "dsh" ? "DSH 任务请在主窗口操作" : "恢复并打开原会话，继续输入")}
            onclick={() => onAction("continue", task)}>
            <Play size={12} aria-hidden="true" />
          </button>
        </div>
      </article>
    {/each}
  </div>
</section>

<style>
  .task-bubbles {
    position: absolute;
    inset: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 10px;
    font-family: var(--text-font);
    font-size: 12px;
  }
  .bubbles-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: 0 3px;
    color: var(--text);
  }
  .bubbles-header span { color: var(--text-muted); font-size: 10px; }
  .task-list {
    display: flex;
    flex-direction: column;
    gap: 7px;
    align-items: flex-start;
  }
  .task-bubble {
    width: fit-content;
    min-width: 200px;
    max-width: 100%;
  }
  .task-bubble, .task-notice {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 48px;
    padding: 8px 10px;
    border: 1px solid var(--border-strong);
    border-radius: 14px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 4px 12px rgb(0 0 0 / 22%);
  }
  .task-notice { margin: 0; overflow-wrap: anywhere; }
  .task-notice button {
    margin-left: auto;
    flex-shrink: 0;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 3px 9px;
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .task-dot { flex-shrink: 0; width: 9px; height: 9px; border-radius: 50%; background: var(--accent); }
  .task-dot-running { background: var(--status-running, var(--accent)); }
  .task-dot-waiting { background: #f6c945; }
  .task-dot-cancelled, .task-dot-failed { background: var(--status-failed); }
  .task-text { min-width: 0; flex: 0 1 auto; display: flex; flex-direction: column; gap: 2px; }
  .task-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  .task-meta { color: var(--text-muted); font-size: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .task-actions { display: flex; gap: 5px; flex-shrink: 0; }
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .icon-button:hover:not(:disabled) { background: var(--surface-hover); }
  .icon-button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .icon-button:disabled { opacity: .4; cursor: not-allowed; }
</style>
