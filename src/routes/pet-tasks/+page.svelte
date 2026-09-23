<script lang="ts">
  // 桌宠任务环浮层：独立 OS 窗口（透明、置顶、不抢焦点、不进任务栏）。
  // 窗口中心与桌宠本体中心重合，气泡沿圆周排布（见 pet-task-ring）。
  // 本页负责：指针是否还在环上的宽限、退场动画播完后的真正隐藏；
  // 结束动作委托给主窗口执行（PET_TASK_RESULT_EVENT 回传关联结果）。
  import { invoke } from "@tauri-apps/api/core";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow, availableMonitors } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import PetTaskBubbles from "$lib/PetTaskBubbles.svelte";
  import { createPetTaskBubbles, type PetTaskBubblesState } from "$lib/pet-task-bubbles";
  import { createPetTaskActionClient, PET_TASK_ACTION_EVENT, PET_TASK_RESULT_EVENT, type PetTaskResult } from "$lib/pet-task-actions";
  import { applyAppearance } from "$lib/appearance";
  import { t } from "$lib/i18n.svelte";
  import type { AppSettings } from "$lib/settings";
  import type { Task } from "$lib/task";
  import type { Monitor } from "$lib/pet-task-ring";

  let taskBubbles = $state<PetTaskBubblesState>({
    open: false, leaving: false, loading: false, tasks: [], pending: [], actionErrors: {}, endedAt: {}, error: "",
  });
  let screenAreas = $state<Monitor[]>([]);
  let hoverRevision = 0;

  const taskActions = createPetTaskActionClient(async (request) => {
    await emitTo("main", PET_TASK_ACTION_EVENT, request);
  });

  const bubbles = createPetTaskBubbles({
    load: () => invoke<Task[]>("list_tasks"),
    resize: (open) => invoke<void>("set_pet_tasks_visible", { open }),
    action: (action, task) => taskActions.request(action, task),
    changed: (state) => { taskBubbles = state; },
  });

  async function refreshMonitors() {
    try {
      const monitors = await availableMonitors();
      screenAreas = monitors.map((monitor) => [
        monitor.position.x,
        monitor.position.y,
        monitor.size.width,
        monitor.size.height,
      ]);
    } catch {
      screenAreas = []; // 取不到屏幕信息时按半径上限渲染（宁可出屏，也不无故缩小）。
    }
  }

  onMount(() => {
    let disposed = false;
    void (async () => {
      try {
        await invoke<void>("await_startup");
        applyAppearance(await invoke<AppSettings>("get_settings"));
      } catch { /* 启动未就绪：保持默认外观 */ }
    })();
    void refreshMonitors();
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
      for (const unlisten of unlisteners) void unlisten.then((dispose) => dispose()).catch(() => {});
    };
  });

  /** 指针进入本体：先取屏幕快照，再让环按收缩后的半径依次绽放。 */
  async function enterRing() {
    await refreshMonitors();
    await bubbles.enter();
  }
</script>

<svelte:head>
  <title>{t("桌宠任务")}</title>
  {@html "<style>html,body{background:transparent !important;overflow:hidden}</style>"}
</svelte:head>

{#if taskBubbles.open || taskBubbles.leaving}
  <PetTaskBubbles ring={taskBubbles} monitors={screenAreas}
    onAction={(task) => void bubbles.act("stop", task)}
    onRetry={() => void bubbles.retry()}
    onKeepOpen={() => bubbles.keepOpen()}
    onLeave={() => bubbles.leave()}
    onSettled={() => void bubbles.finishClose()} />
{/if}
