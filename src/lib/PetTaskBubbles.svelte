<script lang="ts">
  // 桌宠任务环：气泡围着本体排成一圈，按「花瓣依次绽放」的节奏显隐。
  // 每张气泡只保留标题与结束按钮；状态只用左侧圆点表达，不占宽度。
  // 本组件只做渲染与动画，生命周期与动作都在 pet-task-bubbles.ts。
  import { Square, RotateCw } from "@lucide/svelte";
  import { untrack } from "svelte";
  import { t, tm } from "$lib/i18n.svelte";
  import type { Task } from "$lib/task";
  import { canStopPetTask } from "$lib/pet-task-actions";
  import type { PetTaskBubblesState } from "$lib/pet-task-bubbles";
  import {
    RING_ENTER_MS,
    RING_ENTER_STAGGER_MS,
    RING_EXIT_MS,
    RING_EXIT_STAGGER_MS,
    ringLayout,
    ringSlotCount,
  } from "$lib/pet-task-ring";

  // 注意：prop 不能叫 state —— 那会遮蔽 Svelte 的 $state rune。
  let { ring, onOpen, onAction, onRetry, onKeepOpen, onLeave, onSettled }: {
    ring: PetTaskBubblesState;
    /** 点击气泡主体：在主窗口打开该任务（Pi 进任务面板，DSH 进 DSH 视图）。 */
    onOpen: (task: Task) => void;
    onAction: (task: Task) => void;
    onRetry: () => void;
    onKeepOpen: () => void;
    onLeave: () => void;
    /** 退场动画播完：可以真正隐藏窗口了。 */
    onSettled: () => void;
  } = $props();

  let leaving = $state(false);
  let settleTimer: ReturnType<typeof setTimeout> | null = null;
  /** 每次打开都从「未就绪」重新开始，保证依次显示的动画真的重播。 */
  let enterToken = $state(0);
  let showToken = $state(-1);

  const layout = $derived(ringLayout(ring.tasks));
  const radius = $derived(layout.radius);
  const slots = $derived(ringSlotCount(layout));

  $effect(() => {
    if (!ring.open) return;
    // 先以「隐藏」渲染一帧，再切到「显示」：否则元素一挂载就已经是终态，没有动画可看。
    // 写入必须 untrack：`++enterToken` 的读半边会把 enterToken 登记为本效应的依赖，
    // 写半边又立刻作废它 → 效应无限自触发（effect_update_depth_exceeded），
    // 反应式图被毒化，DOM 永久冻结在「正在读取任务…」，气泡与收环全部失灵。
    const token = untrack(() => ++enterToken);
    untrack(() => { showToken = -1; });
    const frame = requestAnimationFrame(() => {
      if (token === enterToken) showToken = token;
    });
    return () => cancelAnimationFrame(frame);
  });

  $effect(() => {
    if (settleTimer) { clearTimeout(settleTimer); settleTimer = null; }
    if (!ring.leaving) { leaving = false; return; }
    leaving = true;
    const total = slots * RING_EXIT_STAGGER_MS + RING_EXIT_MS;
    settleTimer = setTimeout(() => {
      settleTimer = null;
      leaving = false;
      onSettled();
    }, total);
    return () => { if (settleTimer) { clearTimeout(settleTimer); settleTimer = null; } };
  });

  const shown = $derived(showToken === enterToken && !leaving);
  /** 结束态（已完成/失败/被停止）在环上停留后退出，这段时间不参与点击。 */
  function ended(task: Task) {
    return task.status !== "running" && task.status !== "waiting";
  }
</script>

{#snippet dot(task: Task)}
  <span class="task-dot task-dot-{task.status}" aria-hidden="true"></span>
{/snippet}

<!-- 整层不接鼠标事件：只有气泡本身可点，本体窗口与桌面照常收到点击。 -->
<div class="ring" role="presentation"
  onpointerenter={onKeepOpen} onpointerleave={onLeave}
  oncontextmenu={(event) => { event.preventDefault(); event.stopPropagation(); }}>
  {#if ring.open && ring.error}
    <div class="ring-badge ring-error" class:shown class:leaving role="alert"
      style="--x: {layout.badge ? layout.badge.x : 0}px; --y: {layout.badge ? layout.badge.y : -radius}px; --in-delay: {(slots - 1) * RING_ENTER_STAGGER_MS}ms; --out-delay: {(slots - 1) * RING_EXIT_STAGGER_MS}ms">
      <span>{tm(ring.error)}</span>
      <button type="button" onclick={onRetry}>{t("重试")}</button>
    </div>
  {/if}

  {#each layout.bubbles as bubble, index (bubble.task.id)}
    {@const busy = ring.pending.includes(bubble.task.id)}
    {@const failure = ring.actionErrors[bubble.task.id] ?? ""}
    {@const done = ended(bubble.task)}
    <article class="task-bubble" class:shown class:leaving class:busy class:done class:interactive={!busy}
      data-task={bubble.task.id}
      style="--x: {bubble.x}px; --y: {bubble.y}px; width: {layout.bubbleWidth}px; --in-delay: {index * RING_ENTER_STAGGER_MS}ms; --out-delay: {index * RING_EXIT_STAGGER_MS}ms"
      aria-label={bubble.task.title} aria-busy={busy}>
      {#if failure}
        <span class="task-failure" role="alert">{tm(failure)}</span>
        <button type="button" class="retry-button" onclick={() => onAction(bubble.task)}>{t("重试")}</button>
      {:else}
        <!-- 主体是一枚真按钮（不把「结束」按钮嵌进来）：点击回到主窗口对应任务。 -->
        <button type="button" class="bubble-open"
          title={t("在主窗口打开：{title}", { title: bubble.task.title })}
          aria-label={t("在主窗口打开：{title}", { title: bubble.task.title })}
          onclick={() => onOpen(bubble.task)}>
          {@render dot(bubble.task)}
          <strong class="task-title" title={bubble.task.title}>{bubble.task.title || t("未命名任务")}</strong>
        </button>
        {#if !busy && !done && canStopPetTask(bubble.task)}
          <button type="button" class="stop-button"
            aria-label={t("结束任务：{title}", { title: bubble.task.title })}
            title={t("停止当前执行，保留会话")}
            onclick={() => onAction(bubble.task)}>
            <Square size={11} aria-hidden="true" />
          </button>
        {/if}
      {/if}
    </article>
  {/each}

  {#if layout.badge}
    <div class="ring-badge" class:shown class:leaving
      style="--x: {layout.badge.x}px; --y: {layout.badge.y}px; --in-delay: {layout.badge.delayMs}ms; --out-delay: {(slots - 1) * RING_EXIT_STAGGER_MS}ms">
      {t("还有 {count} 个任务", { count: layout.badge.count })}
    </div>
  {/if}

  {#if ring.loading && !ring.tasks.length}
    <div class="ring-badge ring-loading" role="status" style="--y: -{radius}px">
      <RotateCw size={11} aria-hidden="true" />
      <span>{t("正在读取任务…")}</span>
    </div>
  {/if}

  {#if ring.empty && !ring.tasks.length && !ring.error}
    <div class="ring-badge ring-empty" role="status" style="--y: -{radius}px">
      <span>{t("暂无进行中任务")}</span>
    </div>
  {/if}
</div>

<style>
  :global(html, body) { overflow: hidden; }

  .ring {
    position: fixed;
    inset: 0;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
    font-family: var(--text-font);
    font-size: 12px;
  }

  /* —— 中心：与本体中心重合，故以 50%/50% 定位再叠圈的偏移 ——
     位移用独立的 translate 属性、缩放用 scale 属性，两者互不覆盖，
     这样「淡出只改不透明度、不改位置」才不会被位移写死。
     时长写死在这里（不能用 {RING_ENTER_MS}：Svelte 会把样式里的花括号
     当表达式解析），与 pet-task-ring.ts 的一致性由 pet-task-ring.test.ts 守住。 */
  .task-bubble,
  .ring-badge {
    position: absolute;
    top: 50%;
    left: 50%;
    pointer-events: auto;
    translate: calc(-50% + var(--x, 0px)) calc(-50% + var(--y, 0px));
    opacity: 0;
    scale: 0.86;
    transition: opacity 180ms ease-out, scale 180ms ease-out;
  }
  .task-bubble {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 168px;
    height: 32px;
    padding: 0 7px 0 9px;
    border: 1px solid var(--border-strong);
    border-radius: 16px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 4px 12px rgb(0 0 0 / 26%);
    box-sizing: border-box;
  }
  .shown { opacity: 1; scale: 1; transition-delay: var(--in-delay, 0ms); }
  .leaving {
    opacity: 0;
    scale: 0.86;
    transition-duration: 140ms;
    transition-delay: var(--out-delay, 0ms);
    pointer-events: none;
  }

  .task-dot { flex-shrink: 0; width: 8px; height: 8px; border-radius: 50%; background: var(--accent); }
  .task-dot-running { background: var(--status-running, var(--accent)); }
  .task-dot-waiting { background: #f6c945; }
  .task-dot-cancelled, .task-dot-failed { background: var(--status-failed); }
  .task-dot-completed { background: var(--status-completed, #3fb950); }
  /* 打开按钮：铺满气泡左侧，视觉上仍是「圆点 + 标题 + 右侧按钮」的一行。 */
  .bubble-open {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .bubble-open:hover .task-title { color: var(--accent); }
  .bubble-open:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: 6px; }
  .task-title {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    font-weight: 600;
  }
  .stop-button {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    background: var(--surface);
    color: var(--text-muted);
    cursor: pointer;
  }
  .stop-button:hover { background: var(--surface-hover); color: var(--text-strong); }
  .stop-button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

  .task-bubble.busy { opacity: 0.45; pointer-events: none; }
  /* 结束态停留：看得见结果，但不给点击。 */
  .task-bubble.done { opacity: 0.62; }
  .task-failure { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--status-failed); font-size: 11px; }
  .retry-button {
    flex-shrink: 0;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 2px 7px;
    background: var(--surface);
    color: var(--text);
    font-size: 11px;
    cursor: pointer;
  }
  .retry-button:hover { background: var(--surface-hover); }

  .ring-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 220px;
    padding: 4px 10px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
    box-shadow: 0 4px 12px rgb(0 0 0 / 26%);
    font-size: 11px;
    white-space: nowrap;
  }
  .ring-error { color: var(--text); border-color: var(--status-failed); }
  .ring-error button {
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 1px 7px;
    background: var(--surface);
    color: var(--text);
    font-size: 11px;
    cursor: pointer;
  }
  /* 读取中/空态：不参与依次显示的次序，直接可见，且不参与命中。 */
  .ring-loading,
  .ring-empty { pointer-events: none; opacity: 1; scale: 1; }
</style>
