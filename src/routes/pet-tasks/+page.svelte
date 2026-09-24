<script lang="ts">
  // 桌宠任务环浮层：独立 OS 窗口（透明、置顶、不抢焦点、不进任务栏）。
  // 窗口中心与桌宠本体中心重合，气泡沿圆周排布（见 pet-task-ring）。
  // 本页负责：指针是否还在环上的宽限、退场动画播完后的真正隐藏；
  // 结束动作委托给主窗口执行（PET_TASK_RESULT_EVENT 回传关联结果）。
  import { invoke } from "@tauri-apps/api/core";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import PetTaskBubbles from "$lib/PetTaskBubbles.svelte";
  import { createPetTaskBubbles, type PetTaskBubblesState } from "$lib/pet-task-bubbles";
  import { createPetTaskActionClient, PET_TASK_ACTION_EVENT, PET_TASK_RESULT_EVENT, type PetTaskResult } from "$lib/pet-task-actions";
  import { applyAppearance } from "$lib/appearance";
  import { t } from "$lib/i18n.svelte";
  import type { AppSettings } from "$lib/settings";
  import type { Task } from "$lib/task";

  let taskBubbles = $state<PetTaskBubblesState>({
    open: false, leaving: false, loading: false, empty: false, tasks: [], pending: [], actionErrors: {}, endedAt: {}, error: "",
  });
  let hoverRevision = 0;
  let petDebug = $state(false);

  // —— 可点矩形推送：浮层默认点击穿透，宿主轮询光标命中这些矩形才临时接管 ——
  type HitRect = { x: number; y: number; width: number; height: number };
  let lastRectsKey = "";
  let rectsEnterTimer: ReturnType<typeof setTimeout> | null = null;

  function pushHitRects(rects: HitRect[]) {
    const key = JSON.stringify(rects);
    if (key === lastRectsKey) return;
    lastRectsKey = key;
    void invoke("set_pet_hit_rects", { rects }).catch(() => { /* 浮层不可用时忽略 */ });
  }

  /**
   * 量取真正可交互的元素：带按钮（打开/结束/重试）且不忙、不退场的气泡 +
   * 带重试的错误角标。没有按钮的气泡不参与命中——否则它会吞掉点击，
   * 既点不动也传不下去，成为死区。
   */
  function measureHitRects(): HitRect[] {
    const nodes = document.querySelectorAll<HTMLElement>(
      ".task-bubble.interactive:not(.leaving), .ring-error",
    );
    return Array.from(nodes, (node) => {
      const rect = node.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    });
  }

  // 每次状态发布后重测量（rAF 等 DOM 落定）；环收起推空数组。
  $effect(() => {
    const open = taskBubbles.open;
    const leaving = taskBubbles.leaving;
    // 触达所有会影响可点元素集合/位置的字段：任何相关发布都触发重测量。
    const tasksKey = taskBubbles.tasks.map((task) => `${task.id}:${task.status}`).join(",");
    const busyKey = taskBubbles.pending.join(",");
    const errorKey = taskBubbles.error;
    if (petDebug) {
      console.debug("[pet-debug] hit rects refresh", { open, leaving, tasksKey, busyKey, errorKey });
    }
    if (rectsEnterTimer) {
      clearTimeout(rectsEnterTimer);
      rectsEnterTimer = null;
    }
    if (!open && !leaving) {
      pushHitRects([]);
      return;
    }
    const frame = requestAnimationFrame(() => {
      pushHitRects(measureHitRects());
      // 入场动画（最长约 540ms）期间 getBoundingClientRect 偏小：结束后补推一次。
      rectsEnterTimer = setTimeout(() => {
        rectsEnterTimer = null;
        pushHitRects(measureHitRects());
      }, 600);
    });
    return () => cancelAnimationFrame(frame);
  });

  const taskActions = createPetTaskActionClient(async (request) => {
    await emitTo("main", PET_TASK_ACTION_EVENT, request);
  });

  const bubbles = createPetTaskBubbles({
    load: () => invoke<Task[]>("list_tasks"),
    resize: (open) => invoke<void>("set_pet_tasks_visible", { open }),
    action: (action, task) => taskActions.request(action, task),
    changed: (state) => { taskBubbles = state; },
  });

  onMount(() => {
    let disposed = false;
    void (async () => {
      try {
        await invoke<void>("await_startup");
        applyAppearance(await invoke<AppSettings>("get_settings"));
      } catch { /* 启动未就绪：保持默认外观 */ }
    })();
    void invoke<boolean>("pet_debug_enabled")
      .then((enabled) => { petDebug = enabled; })
      .catch(() => { /* 查询失败按非调试处理 */ });
    const unlisteners = [
      listen("pet-appearance", () => {
        void (async () => {
          try { applyAppearance(await invoke<AppSettings>("get_settings")); } catch { /* 保留外观 */ }
        })();
      }),
      // 先注册 hover 监听，再读带版本的当前状态：覆盖页面加载期间的移出事件。
      listen<{ revision: number; hovered: boolean }>("pet-task-hover", ({ payload }) => {
        if (payload.revision < hoverRevision) return;
        hoverRevision = payload.revision;
        if (payload.hovered) void enterRing(); else bubbles.leave();
      }),
    ];
    void (async () => {
      try {
        const state = await invoke<{ revision: number; hovered: boolean }>("get_pet_task_hover");
        if (disposed || state.revision < hoverRevision) return;
        hoverRevision = state.revision;
        if (state.hovered) void enterRing();
      } catch { /* 快照读取失败时等待下一次 hover 事件 */ }
    })();
    return () => {
      disposed = true;
      bubbles.dispose();
      taskActions.dispose();
      if (rectsEnterTimer) clearTimeout(rectsEnterTimer);
      for (const unlisten of unlisteners) void unlisten.then((dispose) => dispose()).catch(() => {});
    };
  });

  /** 指针进入本体：让环按当前任务数量反推的半径依次绽放。 */
  async function enterRing() {
    await bubbles.enter();
  }
</script>

<svelte:head>
  <title>{t("桌宠任务")}</title>
  {@html "<style>html,body{background:transparent !important;overflow:hidden}</style>"}
</svelte:head>

{#if taskBubbles.open || taskBubbles.leaving}
  <PetTaskBubbles ring={taskBubbles}
    onOpen={(task) => void bubbles.act("open", task)}
    onAction={(task) => void bubbles.act("stop", task)}
    onRetry={() => void bubbles.retry()}
    onKeepOpen={() => bubbles.keepOpen()}
    onLeave={() => bubbles.leave()}
    onSettled={() => void bubbles.finishClose()} />
{/if}
