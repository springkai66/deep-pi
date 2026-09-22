<script lang="ts">
  // 桌宠浮窗：透明、置顶、不进任务栏的小窗口。
  // 交互：抓着桌宠/顶部拖动条拖动位置（防抖持久化），悬停显示关闭按钮与
  // 随机气泡；任务运行时进入工作状态，完成时庆祝、失败时沮丧（pet-state）。
  // 形象来自 get_pet_appearance：默认内置图片或本地自定义图片。
  import { invoke } from "@tauri-apps/api/core";
  import { PhysicalPosition } from "@tauri-apps/api/dpi";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { X } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { applyAppearance } from "$lib/appearance";
  import {
    PET_TRANSIENT_MS,
    petStateFromExit,
    petStateFromTasks,
    type PetExitEvent,
    type PetState,
  } from "$lib/pet-state";
  import { t, tm } from "$lib/i18n.svelte";
  import type { AppSettings } from "$lib/settings";
  import { statusLabels, type Task } from "$lib/task";

  type PetAppearance = { kind: "image"; mime: string; dataBase64: string; isDefault: boolean };

  let tasks: Task[] = $state([]);
  let appearance = $state<PetAppearance | null>(null);
  let mood: PetState = $state("idle");
  let bubble = $state("");
  let hover = $state(false);
  let imageSrc = $derived(
    appearance ? `data:${appearance.mime};base64,${appearance.dataBase64}` : "",
  );

  let moodTimer: ReturnType<typeof setTimeout> | null = null;
  let moveSaveTimer: ReturnType<typeof setTimeout> | null = null;
  let reloadTimer: ReturnType<typeof setTimeout> | null = null;
  let hoverPhraseAt = 0;

  // —— 气泡队列：多条提示（多任务完成/悬停问候等）依次播放，不互相覆盖 ——
  const bubbleQueue: { text: string; duration: number }[] = [];
  let bubbleQueueTimer: ReturnType<typeof setTimeout> | null = null;

  function queueBubble(text: string, duration = 2600) {
    bubbleQueue.push({ text, duration });
    if (!bubbleQueueTimer) drainBubbleQueue();
  }

  function drainBubbleQueue() {
    const next = bubbleQueue.shift();
    if (!next) {
      bubbleQueueTimer = null;
      bubble = "";
      return;
    }
    bubble = next.text;
    bubbleQueueTimer = setTimeout(drainBubbleQueue, next.duration);
  }

  const IDLE_PHRASES = ["嗨，我在呢～", "有任务尽管交给我！", "咯咯——", "一起加油鸭！"];

  function scheduleReload(delay = 160) {
    if (reloadTimer) return;
    reloadTimer = setTimeout(() => {
      reloadTimer = null;
      void reloadTasks();
    }, delay);
  }

  async function reloadTasks() {
    try {
      const next = await invoke<Task[]>("list_tasks");
      tasks = next;
      if (mood !== "celebrate" && mood !== "sad") mood = petStateFromTasks(tasks);
      // 环绕气泡展开中：同步刷新，让新任务/已结束任务即时进出。
      if (ringOpen) ringTasks = activeRingTasks();
    } catch {
      // 状态拉取失败保持现状；下一次事件会再试。
    }
  }

  async function refreshAppearance() {
    try {
      appearance = await invoke<PetAppearance>("get_pet_appearance");
    } catch {
      // 读取失败保留当前形象。
    }
  }

  /// 任务退出事件的瞬时状态 + 气泡；之后回落到任务列表派生的常态。
  /// 多个任务接连完成时文案进队列依次播放（多任务依次提示）。
  function onTaskExit(event: PetExitEvent) {
    const transient = petStateFromExit(event, tasks);
    scheduleReload();
    if (!transient) return;
    mood = transient;
    queueBubble(transient === "celebrate" ? t("任务完成！") : t("任务失败…"), PET_TRANSIENT_MS);
    if (moodTimer) clearTimeout(moodTimer);
    moodTimer = setTimeout(() => {
      moodTimer = null;
      mood = petStateFromTasks(tasks);
    }, PET_TRANSIENT_MS);
  }

  function showIdlePhrase() {
    // 悬停问候限频：避免反复划过时喋喋不休。
    const now = Date.now();
    if (now - hoverPhraseAt < 15_000) return;
    hoverPhraseAt = now;
    queueBubble(IDLE_PHRASES[Math.floor(Math.random() * IDLE_PHRASES.length)]);
  }

  function startDrag(event: PointerEvent) {
    if (event.button !== 0) return;
    if (ringOpen) void closeRing();
    event.preventDefault();
    void getCurrentWindow().startDragging().catch(() => { /* 窗口已销毁时忽略 */ });
  }

  function schedulePositionSave(position: { x: number; y: number }) {
    if (moveSaveTimer) clearTimeout(moveSaveTimer);
    moveSaveTimer = setTimeout(() => {
      moveSaveTimer = null;
      void invoke("save_pet_position", { x: Math.round(position.x), y: Math.round(position.y) })
        .catch(() => { /* 位置保存失败不影响使用 */ });
    }, 500);
  }

  async function closePet() {
    try {
      await getCurrentWindow().close();
    } catch { /* 窗口已销毁时忽略 */ }
  }

  // —— 桌面壁纸交互：按住左键爬行、双击闪现 ——
  // 事件来自后端低级鼠标钩子（仅壁纸命中），坐标为窗口左上角目标位置
  // （已锚定本体中心、按 DPI 换算并钳进屏幕，物理像素）。
  const CRAWL_SPEED = 160; // 物理像素/秒，慢慢爬
  type DesktopPointerEvent = { kind: "crawl" | "crawl-end" | "teleport"; x: number; y: number };
  let crawl = $state<{ x: number; y: number; tx: number; ty: number; holding: boolean } | null>(null);
  let crawlRaf = 0;
  let crawlLastTick = 0;
  let lastKnownPosition: { x: number; y: number } | null = null;

  function onDesktopPointer(event: DesktopPointerEvent) {
    if (ringOpen) void closeRing();
    if (event.kind === "teleport") {
      stopCrawlLoop();
      crawl = null;
      void getCurrentWindow()
        .setPosition(new PhysicalPosition(event.x, event.y))
        .catch(() => { /* 窗口已销毁时忽略 */ });
      return;
    }
    if (event.kind === "crawl") {
      if (!crawl) {
        // 从当前位置开始爬（取不到缓存就退化从目标处开始）。
        const from = lastKnownPosition ?? { x: event.x, y: event.y };
        crawl = { x: from.x, y: from.y, tx: event.x, ty: event.y, holding: true };
        startCrawlLoop();
        return;
      }
      crawl.tx = event.x;
      crawl.ty = event.y;
      crawl.holding = true;
      return;
    }
    // crawl-end：松开鼠标，爬到当前目标后停住。
    if (crawl) crawl.holding = false;
  }

  function startCrawlLoop() {
    if (crawlRaf) return;
    crawlLastTick = performance.now();
    const step = (now: number) => {
      crawlRaf = 0;
      const current = crawl;
      if (!current) return;
      const dt = Math.min((now - crawlLastTick) / 1000, 0.1);
      crawlLastTick = now;
      const dx = current.tx - current.x;
      const dy = current.ty - current.y;
      const dist = Math.hypot(dx, dy);
      if (dist < 1 && !current.holding) {
        crawl = null;
        return;
      }
      if (dist >= 1) {
        const move = Math.min(dist, CRAWL_SPEED * dt);
        crawl = {
          ...current,
          x: current.x + (dx / dist) * move,
          y: current.y + (dy / dist) * move,
        };
        void getCurrentWindow()
          .setPosition(new PhysicalPosition(Math.round(crawl.x), Math.round(crawl.y)))
          .catch(() => { /* 窗口已销毁时忽略 */ });
      }
      crawlRaf = requestAnimationFrame(step);
    };
    crawlRaf = requestAnimationFrame(step);
  }

  function stopCrawlLoop() {
    if (crawlRaf) cancelAnimationFrame(crawlRaf);
    crawlRaf = 0;
  }

  // —— 右键环绕任务气泡 ——
  // 展开时窗口从 210×240 扩到 400×320（逻辑像素，与后端 RING 常量一致），
  // 由后端补偿位置保持本体不动；气泡沿本体上半圈环绕排布。
  const RING_WIDTH = 400;
  const RING_HEIGHT = 320;
  const RING_AUTO_CLOSE_MS = 8000;
  let ringOpen = $state(false);
  let ringTasks = $state<Task[]>([]);
  let ringCloseTimer: ReturnType<typeof setTimeout> | null = null;

  function activeRingTasks(): Task[] {
    return tasks.filter((task) => task.status === "running" || task.status === "waiting");
  }

  function toggleRing(event: MouseEvent) {
    event.preventDefault();
    if (ringOpen) {
      void closeRing();
      return;
    }
    ringTasks = activeRingTasks();
    if (!ringTasks.length) {
      queueBubble(t("现在没有执行中的任务"));
      return;
    }
    ringOpen = true;
    void invoke("set_pet_ring", { open: true }).catch(() => { /* 窗口已销毁时忽略 */ });
    armRingAutoClose();
  }

  async function closeRing() {
    if (!ringOpen) return;
    ringOpen = false;
    if (ringCloseTimer) {
      clearTimeout(ringCloseTimer);
      ringCloseTimer = null;
    }
    try {
      await invoke("set_pet_ring", { open: false });
    } catch { /* 窗口可能已销毁 */ }
  }

  function armRingAutoClose() {
    if (ringCloseTimer) clearTimeout(ringCloseTimer);
    ringCloseTimer = setTimeout(() => {
      ringCloseTimer = null;
      void closeRing();
    }, RING_AUTO_CLOSE_MS);
  }

  function ringBubbleStyle(index: number, total: number): string {
    // 气泡沿本体上半圈环绕：展开窗口里本体中心在 (200, 235)。
    const cx = RING_WIDTH / 2;
    const cy = RING_HEIGHT - 85;
    const rx = 140;
    const ry = 92;
    const degrees = total === 1 ? 90 : 165 - (index * 150) / (total - 1);
    const angle = (degrees * Math.PI) / 180;
    const left = Math.round(cx + rx * Math.cos(angle));
    const top = Math.round(cy - ry * Math.sin(angle));
    return `left:${left}px;top:${top}px`;
  }

  onMount(() => {
    void (async () => {
      try {
        await invoke<void>("await_startup");
        applyAppearance(await invoke<AppSettings>("get_settings"));
        await Promise.all([reloadTasks(), refreshAppearance()]);
      } catch {
        // 启动未就绪/读取失败：保持默认形象与待机状态。
      }
    })();
    const unlisteners = [
      listen("task-status", () => scheduleReload()),
      listen<PetExitEvent>("pty-exit", ({ payload }) => onTaskExit(payload)),
      listen<PetExitEvent>("rpc-task-exit", ({ payload }) => onTaskExit(payload)),
      listen("pet-appearance", () => void refreshAppearance()),
      listen<DesktopPointerEvent>("desktop-pointer", ({ payload }) => onDesktopPointer(payload)),
    ];
    // 记录窗口位置（爬行起点用）；拖动/爬行停住后防抖保存位置。
    void getCurrentWindow().onMoved(({ payload }) => {
      lastKnownPosition = { x: payload.x, y: payload.y };
      schedulePositionSave(payload);
    });
    void getCurrentWindow()
      .outerPosition()
      .then((position) => { lastKnownPosition = { x: position.x, y: position.y }; })
      .catch(() => { /* 取不到就首次爬行从目标处开始 */ });
    void getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused) return;
      void refreshAppearance();
      void (async () => {
        try {
          applyAppearance(await invoke<AppSettings>("get_settings"));
        } catch { /* 保留当前外观 */ }
      })();
    });
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((dispose) => dispose());
      if (moodTimer) clearTimeout(moodTimer);
      if (bubbleQueueTimer) clearTimeout(bubbleQueueTimer);
      if (moveSaveTimer) clearTimeout(moveSaveTimer);
      if (reloadTimer) clearTimeout(reloadTimer);
      if (ringCloseTimer) clearTimeout(ringCloseTimer);
      stopCrawlLoop();
    };
  });
</script>

<svelte:head>
  <title>{t("桌宠")}</title>
  <!-- 透明背景以 head 内联样式实现：本页组件 <style> 与多个 :global 规则
       组合时 svelte-check 会报 $state 解析错误（成因未定位），整段绕开。 -->
  {@html "<style>html,body{background:transparent !important;overflow:hidden}</style>"}
</svelte:head>

<div class="pet-window" role="presentation" oncontextmenu={(event) => event.preventDefault()}>
  <!-- 顶部拖动条：常驻的抓握区（无边框窗口没有系统标题栏）。 -->
  <div class="pet-drag-strip" data-tauri-drag-region>
    {#if hover}
      <button type="button" class="pet-close" aria-label={t("关闭桌宠")} title={t("关闭桌宠")}
        onclick={() => void closePet()}><X size={13} aria-hidden="true" /></button>
    {/if}
  </div>
  <div class="pet-stage">
    {#if bubble}
      <div class="pet-bubble" role="status">{tm(bubble)}</div>
    {/if}
    <!-- 桌宠本体：左键按住拖动，右键环绕任务气泡，悬停出气泡。 -->
    <div class="pet-figure pet-{mood}" class:pet-walk={Boolean(crawl)} role="img" aria-label={t("桌宠")}
      onpointerenter={() => { hover = true; showIdlePhrase(); }}
      onpointerleave={() => { hover = false; bubble = ""; }}
      onpointerdown={startDrag}
      oncontextmenu={toggleRing}>
      {#if appearance}
        <img class="pet-image" src={imageSrc} alt={t("桌宠")} draggable="false" />
      {/if}
    </div>
  </div>
  {#if ringOpen}
    <!-- 环绕任务气泡：覆盖整个窗口，气泡本体不可交互，仅展示。 -->
    <div class="pet-ring" aria-label={t("执行中的任务")}>
      {#each ringTasks as task, index (task.id)}
        <div class="pet-ring-bubble" style={ringBubbleStyle(index, ringTasks.length)}>
          <span class="pet-ring-dot pet-ring-dot-{task.status}" aria-hidden="true"></span>
          <span class="pet-ring-title">{task.title}</span>
          <span class="pet-ring-status">{t(statusLabels[task.status])}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .pet-window {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    user-select: none;
    -webkit-user-select: none;
  }
  .pet-drag-strip {
    position: relative;
    height: 26px;
    flex-shrink: 0;
  }
  .pet-close {
    position: absolute;
    top: 4px;
    right: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    color: var(--text-muted);
    background: var(--surface);
    cursor: pointer;
  }
  .pet-close:hover { color: var(--text-strong); background: var(--surface-hover); }
  .pet-stage {
    position: relative;
    flex: 1;
    display: grid;
    place-items: end center;
    padding-bottom: 10px;
  }
  .pet-figure {
    position: relative;
    width: 150px;
    height: 150px;
    cursor: grab;
    touch-action: none;
  }
  .pet-figure:active { cursor: grabbing; }
  .pet-image {
    width: 100%;
    height: 100%;
    object-fit: contain;
    -webkit-user-drag: none;
  }
  .pet-bubble {
    position: absolute;
    bottom: 128px;
    left: 50%;
    transform: translateX(-50%);
    max-width: 180px;
    padding: 5px 9px;
    border: 1px solid var(--border-strong);
    border-radius: 9px;
    border-bottom-left-radius: 2px;
    color: var(--text);
    background: var(--surface);
    box-shadow: 0 8px 22px rgb(0 0 0 / 25%);
    font-family: var(--text-font);
    font-size: 12px;
    line-height: 1.45;
    white-space: nowrap;
    overflow-wrap: anywhere;
    animation: bubble-in 150ms ease-out;
  }
  @keyframes bubble-in {
    from { opacity: 0; transform: translateX(-50%) translateY(4px); }
    to { opacity: 1; transform: translateX(-50%) translateY(0); }
  }

  /* —— 状态动画（同样作用于自定义图片） —— */
  .pet-idle { animation: pet-bob 2.6s ease-in-out infinite; }
  .pet-working { animation: pet-bob 1.4s ease-in-out infinite; }
  .pet-celebrate { animation: pet-jump 0.62s ease-in-out 4; transform-origin: bottom center; }
  .pet-sad { animation: pet-shake 0.45s ease-in-out 3; }
  /* 爬行：跟随鼠标移动时的小碎步步态（浮动加速 + 轻微摇摆）。 */
  .pet-walk {
    animation: pet-bob 0.9s ease-in-out infinite, pet-waddle 1.8s ease-in-out infinite;
  }
  @keyframes pet-waddle {
    0%, 100% { rotate: 0deg; }
    30% { rotate: 2.5deg; }
    70% { rotate: -2.5deg; }
  }

  @keyframes pet-bob {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-5px); }
  }
  @keyframes pet-jump {
    0%, 100% { transform: translateY(0) rotate(0deg); }
    30% { transform: translateY(-16px) rotate(-4deg); }
    60% { transform: translateY(0) rotate(0deg); }
    80% { transform: translateY(-8px) rotate(3deg); }
  }
  @keyframes pet-shake {
    0%, 100% { transform: translateX(0); }
    25% { transform: translateX(-4px); }
    75% { transform: translateX(4px); }
  }

  /* —— 右键环绕任务气泡 —— */
  .pet-ring {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 5;
  }
  .pet-ring-bubble {
    position: absolute;
    transform: translate(-50%, -50%);
    display: flex;
    align-items: center;
    gap: 5px;
    max-width: 150px;
    padding: 4px 9px;
    border: 1px solid var(--border-strong);
    border-radius: 9px;
    color: var(--text);
    background: var(--surface);
    box-shadow: 0 8px 22px rgb(0 0 0 / 25%);
    font-family: var(--text-font);
    font-size: 12px;
    line-height: 1.4;
    white-space: nowrap;
    animation: ring-in 150ms ease-out;
  }
  .pet-ring-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .pet-ring-status { flex-shrink: 0; color: var(--text-muted); font-size: 10px; }
  .pet-ring-dot { flex-shrink: 0; width: 7px; height: 7px; border-radius: 50%; background: var(--accent); }
  .pet-ring-dot-running { background: var(--status-running, var(--accent)); }
  .pet-ring-dot-waiting { background: #f6c945; }
  @keyframes ring-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
</style>
