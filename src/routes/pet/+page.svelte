<script lang="ts">
  // 桌宠浮窗：透明、置顶、不进任务栏的小窗口，尺寸贴合本体（见 fitWindowToFigure）。
  // 交互：抓着桌宠拖动位置（防抖持久化），悬停窗口任意处显示右上角关闭
  // 按钮；悬停本体让任务环依次绽放，任务完成时庆祝、失败时沮丧
  // （pet-state）。形象来自 get_pet_appearance：默认内置图片或本地自定义图片。
  //
  // 任务环画在独立的浮层窗口里（见 pet-tasks 路由）：本体窗口的尺寸、位置
  // 与图片节点在悬停期间完全不变，避免透明 WebView 改尺寸闪烁；本体只负责
  // 上报「指针进/出本体」，环的宽限与退场动画由浮层自己收尾。
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
  import type { Task } from "$lib/task";

  type PetAppearance = { kind: "image"; mime: string; dataBase64: string; isDefault: boolean };
  type PetTaskHover = { revision: number; hovered: boolean };

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

  /** 本体只上报「指针进了还是出了」，环的显隐、宽限与退场都由浮层窗口负责。 */
  function reportHover(hovered: boolean) {
    void invoke<void>("set_pet_ring", { open: hovered }).catch(() => {
      /* 浮层不可用时忽略：悬停只是看任务，不该打断桌宠本身 */
    });
  }

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

  function scheduleReload(delay = 160) {
    if (reloadTimer) return;
    reloadTimer = setTimeout(() => {
      reloadTimer = null;
      void reloadTasks();
    }, delay);
  }

  function applyTaskSnapshot(next: Task[]) {
    tasks = next;
    if (mood !== "celebrate" && mood !== "sad") mood = petStateFromTasks(tasks);
  }

  async function reloadTasks() {
    try {
      applyTaskSnapshot(await invoke<Task[]>("list_tasks"));
    } catch {
      // 后台状态失败保留现状；任务数据由独立浮层窗口读取与展示。
    }
  }

  async function refreshWindowAppearance() {
    try { applyAppearance(await invoke<AppSettings>("get_settings")); } catch { /* 保留主题 */ }
  }

  async function refreshAppearance() {
    try {
      appearance = await invoke<PetAppearance>("get_pet_appearance");
    } catch {
      // 读取失败保留当前形象。
    }
  }

  // —— 贴合本体：量取形象的不透明边界，把窗口收紧到本体大小 ——
  // 窗口曾经比本体大出一圈（拖动条 + 头部留白），透明区域既挡桌面点击
  // 又显得空。现在窗口只包住本体的不透明部分：默认形象的量测结果与
  // pet.rs 的 PET_WIDTH/HEIGHT 一致（开窗即贴合，无跳变）；自定义形象
  // 载入后量测并调用 fit_pet_window 动态贴合（窗口中心不动）。
  /** 本体显示盒：形象按 contain 缩放的基准尺寸（窗口内图片的显示尺寸）。 */
  const FIGURE_BOX = 150;
  /** 贴合余量：左右给摇摆动画、顶部给庆祝跳跃腾空与关闭按钮留白、底部贴地。 */
  const FIT_MARGIN_X = 4;
  const FIT_MARGIN_TOP = 24;
  const FIT_MARGIN_BOTTOM = 3;

  /** 贴合结果：窗口尺寸与图片在窗口内的裁剪摆放（逻辑像素）；null = 未量测。 */
  let fit = $state<{
    width: number;
    height: number;
    imageX: number;
    imageY: number;
    imageWidth: number;
    imageHeight: number;
  } | null>(null);

  /** 逐像素扫描 alpha，返回图片自然尺寸与不透明边界；失败返回 null。 */
  async function measureOpaqueBox(src: string) {
    const image = new Image();
    image.src = src;
    try {
      await image.decode();
    } catch {
      return null;
    }
    const naturalWidth = image.naturalWidth;
    const naturalHeight = image.naturalHeight;
    if (!naturalWidth || !naturalHeight) return null;
    const canvas = document.createElement("canvas");
    canvas.width = naturalWidth;
    canvas.height = naturalHeight;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) return null;
    context.drawImage(image, 0, 0);
    let pixels: Uint8ClampedArray;
    try {
      pixels = context.getImageData(0, 0, naturalWidth, naturalHeight).data;
    } catch {
      return null; // 画布被污染时放弃贴合（形象来自 data URL，正常不会发生）。
    }
    let minX = naturalWidth;
    let minY = naturalHeight;
    let maxX = -1;
    let maxY = -1;
    for (let y = 0; y < naturalHeight; y += 1) {
      const row = y * naturalWidth * 4;
      for (let x = 0; x < naturalWidth; x += 1) {
        if (pixels[row + x * 4 + 3] > 8) {
          if (x < minX) minX = x;
          if (x > maxX) maxX = x;
          if (y < minY) minY = y;
          if (y > maxY) maxY = y;
        }
      }
    }
    if (maxX < 0) return null;
    return { naturalWidth, naturalHeight, x: minX, y: minY, right: maxX + 1, bottom: maxY + 1 };
  }

  /** 量测并贴合：窗口缩到本体大小（中心不动），图片按裁剪框摆放。 */
  async function fitWindowToFigure() {
    if (!appearance) return;
    const box = await measureOpaqueBox(imageSrc);
    if (!box) return;
    const scale = Math.min(FIGURE_BOX / box.naturalWidth, FIGURE_BOX / box.naturalHeight);
    const imageWidth = box.naturalWidth * scale;
    const imageHeight = box.naturalHeight * scale;
    // contain 居中落点 + 不透明边界 → 本体在显示坐标系里的矩形。
    const bodyX = (FIGURE_BOX - imageWidth) / 2 + box.x * scale;
    const bodyY = (FIGURE_BOX - imageHeight) / 2 + box.y * scale;
    const bodyWidth = (box.right - box.x) * scale;
    const bodyHeight = (box.bottom - box.y) * scale;
    const width = Math.round(bodyWidth) + FIT_MARGIN_X * 2;
    const height = Math.round(bodyHeight) + FIT_MARGIN_TOP + FIT_MARGIN_BOTTOM;
    // 量测异常（过小/过大）时维持现状：宁可留白也不裁掉本体。
    if (width < 48 || height < 48 || width > 320 || height > 320) return;
    fit = {
      width,
      height,
      imageWidth: Math.round(imageWidth),
      imageHeight: Math.round(imageHeight),
      imageX: -(bodyX - FIT_MARGIN_X),
      imageY: -(bodyY - FIT_MARGIN_TOP),
    };
    try {
      await invoke<void>("fit_pet_window", { width, height });
    } catch {
      /* 窗口已关闭或量测被拒：保持当前窗口 */
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

  /**
   * 指针离开本体：只告诉浮层「环进入宽限期」。
   * 关闭按钮的显隐由**窗口级** pointerenter/leave 负责——指针从本体移到
   * 右上角按钮上时窗口没离开，按钮必须留在原地，否则永远点不到。
   */
  function ringClose() {
    reportHover(false);
  }

  /**
   * 桌宠移动（拖动/爬行/闪现）后把浮层同步到新位置：气泡跟着本体一起走。
   * onMoved 触发很密，这里按 40ms 合并一次，避免每个移动事件都打一次 IPC。
   */
  let followTimer: ReturnType<typeof setTimeout> | null = null;
  function syncPetTasksPosition() {
    if (followTimer) return;
    followTimer = setTimeout(() => {
      followTimer = null;
      void invoke<void>("sync_pet_tasks_position").catch(() => { /* 浮层未显示时无操作 */ });
    }, 40);
  }

  function startDrag(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    // 环保持打开：拖动过程中浮层会跟着本体一起移动（不再先收环）。
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
    // 壁纸交互期间指针不在本体上：关掉关闭按钮，但**不报告 hover 结束**——
    // 爬行/闪现时浮层要跟着本体一起移动，不能被收掉。
    hover = false;
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

  onMount(() => {
    void (async () => {
      try {
        await invoke<void>("await_startup");
        await Promise.all([refreshWindowAppearance(), reloadTasks(), refreshAppearance()]);
        await fitWindowToFigure();
      } catch {
        // 启动未就绪/读取失败：保持默认形象与待机状态。
      }
    })();
    const unlisteners = [
      listen("task-status", () => scheduleReload()),
      listen("dsh-tasks", () => scheduleReload()),
      listen<PetExitEvent>("pty-exit", ({ payload }) => onTaskExit(payload)),
      listen<PetExitEvent>("rpc-task-exit", ({ payload }) => onTaskExit(payload)),
      listen("pet-appearance", () => {
        void (async () => {
          await refreshAppearance();
          await fitWindowToFigure();
        })();
      }),
      listen<DesktopPointerEvent>("desktop-pointer", ({ payload }) => onDesktopPointer(payload)),
    ];
    // 指针进入/离开本体：本地控制关闭按钮的显隐；浮层据此判断宽限期起点。
    const hovered = listen<PetTaskHover>("pet-task-hover", ({ payload }) => {
      if (payload.hovered) hover = true;
    });
    unlisteners.push(hovered);
    // 记录窗口位置（爬行起点用）；拖动/爬行停住后防抖保存位置，
    // 并让任务浮层跟着一起移动（环显示期间气泡跟随本体）。
    unlisteners.push(getCurrentWindow().onMoved(({ payload }) => {
      lastKnownPosition = { x: payload.x, y: payload.y };
      schedulePositionSave(payload);
      syncPetTasksPosition();
    }));
    void getCurrentWindow()
      .outerPosition()
      .then((position) => { lastKnownPosition = { x: position.x, y: position.y }; })
      .catch(() => { /* 取不到就首次爬行从目标处开始 */ });
    unlisteners.push(getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (!focused) return;
      void refreshAppearance();
      void refreshWindowAppearance();
      scheduleReload(80);
    }));
    // 窗口重建而指针已经压在本体上时，用快照补一次悬停上报（浮层由此打开）。
    void invoke<PetTaskHover>("get_pet_task_hover")
      .then((state) => { if (state.hovered) { hover = true; reportHover(true); } })
      .catch(() => { /* 快照读不到就等下一次 pointerenter */ });
    return () => {
      for (const unlisten of unlisteners) void unlisten.then((dispose) => dispose()).catch(() => {});
      if (moodTimer) clearTimeout(moodTimer);
      if (bubbleQueueTimer) clearTimeout(bubbleQueueTimer);
      if (moveSaveTimer) clearTimeout(moveSaveTimer);
      if (reloadTimer) clearTimeout(reloadTimer);
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

<!-- hover 跟踪在窗口根节点：若挂在桌宠本体上，鼠标从本体移向右上角
     关闭按钮的途中 pointerleave 会提前触发、按钮被卸载，永远点不到。 -->
<div class="pet-window" role="presentation" oncontextmenu={(event) => event.preventDefault()}
  onpointerenter={() => { hover = true; }}
  onpointerleave={() => { hover = false; }}>
  <div class="pet-stage">
    {#if hover}
      <button type="button" class="pet-close" aria-label={t("关闭桌宠")} title={t("关闭桌宠")}
        onclick={() => void closePet()}><X size={13} aria-hidden="true" /></button>
    {/if}
    {#if bubble}
      <div class="pet-bubble" role="status">{tm(bubble)}</div>
    {/if}
    <!-- 悬停上报给独立任务浮层；窗口尺寸、位置与图片节点在悬停期间保持不变。
         窗口已贴合本体：抓着本体即可拖动（不再有独立的顶部拖动条）。 -->
    <div class="pet-figure pet-{mood}" class:pet-walk={Boolean(crawl)} role="img" aria-label={t("桌宠，悬停查看任务")}
      style={fit ? `--fit-w: ${fit.width}px; --fit-h: ${fit.height}px; --img-w: ${fit.imageWidth}px; --img-h: ${fit.imageHeight}px; --img-x: ${fit.imageX}px; --img-y: ${fit.imageY}px` : undefined}
      onpointerenter={() => { hover = true; reportHover(true); }}
      onpointerleave={ringClose}
      onpointerdown={startDrag}>
      {#if appearance}
        <img class="pet-image" src={imageSrc} alt={t("桌宠")} draggable="false" />
      {/if}
    </div>
  </div>
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
  .pet-stage {
    position: relative;
    flex: 1;
  }
  .pet-close {
    position: absolute;
    top: 2px;
    right: 2px;
    z-index: 2;
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
  /* 贴合本体：窗口即本体盒（默认 99×119，与 pet.rs 的 PET_WIDTH/HEIGHT
     一致），图片按量测出的裁剪框摆放，透明边不再占据窗口；顶部 24px 留白
     既供庆祝跳跃腾空，也让关闭按钮不与本体重叠。 */
  .pet-figure {
    position: absolute;
    bottom: 0;
    left: 50%;
    translate: -50% 0;
    width: var(--fit-w, 99px);
    height: var(--fit-h, 119px);
    overflow: hidden;
    cursor: grab;
    touch-action: none;
  }
  .pet-figure:active { cursor: grabbing; }
  .pet-image {
    position: absolute;
    left: var(--img-x, -26px);
    top: var(--img-y, -7.2px);
    width: var(--img-w, 150px);
    height: var(--img-h, 150px);
    /* 显示尺寸已按 contain 算好，直接铺满裁剪框 */
    object-fit: fill;
    -webkit-user-drag: none;
  }
  .pet-bubble {
    position: absolute;
    top: 2px;
    left: 50%;
    transform: translateX(-50%);
    max-width: calc(100% - 8px);
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
    overflow-wrap: anywhere;
    z-index: 1;
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

</style>
