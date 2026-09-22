<script lang="ts">
  // 桌宠任务气泡浮层：独立 OS 窗口（透明、置顶、不抢焦点、不进任务栏）。
  // 悬停桌宠本体时由后端转发带版本的 hover 快照；本页只负责显示/隐藏与展示，
  // 停止/继续动作委托给主窗口执行（PET_TASK_RESULT_EVENT 回传关联结果）。
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

  let taskBubbles = $state<PetTaskBubblesState>({ open: false, loading: false, tasks: [], pending: [], error: "" });
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

  onMount(() => {
    let disposed = false;
    void (async () => {
      try {
        await invoke<void>("await_startup");
        applyAppearance(await invoke<AppSettings>("get_settings"));
      } catch { /* 启动未就绪：保持默认外观 */ }
    })();
    const unlisteners = [
      listen("pet-appearance", () => {
        void (async () => {
          try { applyAppearance(await invoke<AppSettings>("get_settings")); } catch { /* 保留外观 */ }
        })();
      }),
      // 先注册 hover 监听，再读带版本的当前状态：覆盖页面加载期间的移出事件。
      listen<{ revision: number; hovered: boolean; forceClose: boolean }>("pet-task-hover", ({ payload }) => {
        if (payload.revision < hoverRevision) return;
        hoverRevision = payload.revision;
        if (payload.forceClose) { void bubbles.close(); return; }
        if (payload.hovered) void bubbles.enter(); else bubbles.leave();
      }),
    ];
    void (async () => {
      try {
        const state = await invoke<{ revision: number; hovered: boolean }>("get_pet_task_hover");
        if (disposed || state.revision < hoverRevision) return;
        hoverRevision = state.revision;
        if (state.hovered) void bubbles.enter();
      } catch { /* 快照读取失败时等待下一次 hover 事件 */ }
    })();
    return () => {
      disposed = true;
      bubbles.dispose();
      taskActions.dispose();
      for (const unlisten of unlisteners) void unlisten.then((dispose) => dispose()).catch(() => {});
    };
  });
</script>

<svelte:head>
  <title>{t("桌宠任务")}</title>
  {@html "<style>html,body{background:transparent !important;overflow:hidden}</style>"}
</svelte:head>

<div class="pet-tasks" role="presentation"
  onpointerenter={() => bubbles.keepOpen()}
  onpointerleave={() => bubbles.leave()}>
  {#if taskBubbles.open}
    <PetTaskBubbles state={taskBubbles} onAction={(action, task) => void bubbles.act(action, task)}
      onRetry={() => void bubbles.retry()} />
  {/if}
</div>

<style>
  .pet-tasks {
    position: fixed;
    inset: 0;
    user-select: none;
    -webkit-user-select: none;
  }
</style>
