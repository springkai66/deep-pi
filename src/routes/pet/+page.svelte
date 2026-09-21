<script lang="ts">
  // 桌宠浮窗：透明、置顶、不进任务栏的小窗口。
  // 交互：抓着桌宠/顶部拖动条拖动位置（防抖持久化），悬停显示关闭按钮与
  // 随机气泡；任务运行时进入工作状态，完成时庆祝、失败时沮丧（pet-state）。
  // 形象支持内置机器人、本地图片与 AI 生成的 SVG（get_pet_appearance）。
  import { invoke } from "@tauri-apps/api/core";
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

  type PetAppearance = { kind: "builtin" } | { kind: "image"; mime: string; dataBase64: string };

  let tasks: Task[] = $state([]);
  let appearance: PetAppearance = $state({ kind: "builtin" } as PetAppearance);
  let mood: PetState = $state("idle");
  let bubble = $state("");
  let hover = $state(false);
  let imageSrc = $derived(
    appearance.kind === "image" ? `data:${appearance.mime};base64,${appearance.dataBase64}` : "",
  );

  let transientTimer: ReturnType<typeof setTimeout> | null = null;
  let moveSaveTimer: ReturnType<typeof setTimeout> | null = null;
  let reloadTimer: ReturnType<typeof setTimeout> | null = null;
  let hoverPhraseAt = 0;

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
    } catch {
      // 状态拉取失败保持现状；下一次事件会再试。
    }
  }

  async function refreshAppearance() {
    try {
      appearance = await invoke<{ kind: "builtin" } | { kind: "image"; mime: string; dataBase64: string }>("get_pet_appearance");
    } catch {
      // 读取失败保留当前形象。
    }
  }

  /// 任务退出事件的瞬时状态 + 气泡；之后回落到任务列表派生的常态。
  function onTaskExit(event: PetExitEvent) {
    const transient = petStateFromExit(event, tasks);
    scheduleReload();
    if (!transient) return;
    mood = transient;
    bubble = transient === "celebrate" ? t("任务完成！") : t("任务失败…");
    if (transientTimer) clearTimeout(transientTimer);
    transientTimer = setTimeout(() => {
      transientTimer = null;
      mood = petStateFromTasks(tasks);
      bubble = "";
    }, PET_TRANSIENT_MS);
  }

  function showIdlePhrase() {
    // 悬停问候限频：避免反复划过时喋喋不休。
    const now = Date.now();
    if (now - hoverPhraseAt < 15_000) return;
    hoverPhraseAt = now;
    bubble = IDLE_PHRASES[Math.floor(Math.random() * IDLE_PHRASES.length)];
    if (transientTimer) clearTimeout(transientTimer);
    transientTimer = setTimeout(() => {
      transientTimer = null;
      bubble = "";
    }, 2600);
  }

  function startDrag(event: PointerEvent) {
    if (event.button !== 0) return;
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
    ];
    // 拖动结束（移动事件停住）后保存位置；重新聚焦时同步外观与语言。
    void getCurrentWindow().onMoved(({ payload }) => schedulePositionSave(payload));
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
      if (transientTimer) clearTimeout(transientTimer);
      if (moveSaveTimer) clearTimeout(moveSaveTimer);
      if (reloadTimer) clearTimeout(reloadTimer);
    };
  });
</script>

<svelte:head>
  <title>{t("桌宠")}</title>
  <!-- 透明背景以 head 内联样式实现：本页组件 <style> 与多个 :global 规则
       组合时 svelte-check 会报 $state 解析错误（成因未定位），整段绕开。 -->
  {@html "<style>html,body{background:transparent !important;overflow:hidden}</style>"}
</svelte:head>

<div class="pet-window">
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
    <!-- 桌宠本体：按住即拖动；悬停出气泡。 -->
    <div class="pet-figure pet-{mood}" role="img" aria-label={t("桌宠")}
      onpointerenter={() => { hover = true; showIdlePhrase(); }}
      onpointerleave={() => { hover = false; bubble = ""; }}
      onpointerdown={startDrag}>
      <!-- 两个独立 if 块：实测 {:else} 跨 HTML 与 SVG 命名空间时
           svelte-check 会误报 $state 错误（成因未定位），拆开绕开 -->
      {#if appearance.kind === "image"}
        <img class="pet-image" src={imageSrc} alt={t("桌宠")} draggable="false" />
      {/if}
      {#if appearance.kind !== "image"}
        <svg class="pet-robot" viewBox="0 0 120 120" aria-hidden="true">
          <!-- 天线：顶灯随状态呼吸 -->
          <line class="antenna" x1="60" y1="26" x2="60" y2="14" />
          <circle class="antenna-tip" cx="60" cy="11" r="5" />
          <!-- 头身一体 -->
          <rect class="body" x="26" y="26" width="68" height="66" rx="22" />
          <!-- 耳侧 -->
          <rect class="ear" x="18" y="48" width="8" height="18" rx="4" />
          <rect class="ear" x="94" y="48" width="8" height="18" rx="4" />
          <!-- 眼睛：眨眼动画由 CSS 驱动 -->
          <g class="eyes">
            <circle class="eye" cx="48" cy="52" r="5.5" />
            <circle class="eye" cx="72" cy="52" r="5.5" />
          </g>
          <!-- 嘴：三套嘴形常驻 DOM，由容器状态类驱动 CSS 切换 -->
          <path class="mouth mouth-happy" d="M51 64 Q60 72 69 64" />
          <circle class="mouth mouth-working" cx="60" cy="67" r="3.4" />
          <path class="mouth mouth-sad" d="M52 68 Q60 62 68 68" />
          <!-- 腮红与脚 -->
          <circle class="blush" cx="40" cy="62" r="3.4" />
          <circle class="blush" cx="80" cy="62" r="3.4" />
          <rect class="foot" x="42" y="92" width="14" height="8" rx="4" />
          <rect class="foot" x="64" y="92" width="14" height="8" rx="4" />
        </svg>
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
  .pet-robot { width: 100%; height: 100%; }
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

  /* —— 内置机器人配色（跟随主题令牌） —— */
  .body { fill: var(--accent); opacity: 0.92; }
  .ear { fill: var(--accent); opacity: 0.65; }
  .foot { fill: var(--accent); opacity: 0.65; }
  .antenna { stroke: var(--accent); stroke-width: 3; stroke-linecap: round; }
  .antenna-tip { fill: var(--status-running, var(--accent)); }
  .eye { fill: var(--surface); animation: pet-blink 4.2s infinite; transform-origin: center; }
  .mouth { stroke: var(--surface); stroke-width: 2.6; stroke-linecap: round; fill: none; display: none; }
  .pet-idle .mouth-happy, .pet-celebrate .mouth-happy { display: block; }
  .pet-working .mouth-working { display: block; fill: var(--surface); stroke: none; }
  .pet-sad .mouth-sad { display: block; }
  .blush { fill: var(--surface); opacity: 0.35; }

  /* —— 状态动画（对内置 SVG 与自定义图片同样生效） —— */
  .pet-idle { animation: pet-bob 2.6s ease-in-out infinite; }
  .pet-working { animation: pet-bob 1.4s ease-in-out infinite; }
  .pet-working .antenna-tip { animation: pet-glow 1.1s ease-in-out infinite; }
  .pet-celebrate { animation: pet-jump 0.62s ease-in-out 4; transform-origin: bottom center; }
  .pet-celebrate .antenna-tip { fill: #f6c945; }
  .pet-sad { animation: pet-shake 0.45s ease-in-out 3; }

  @keyframes pet-bob {
    0%, 100% { transform: translateY(0); }
    50% { transform: translateY(-5px); }
  }
  @keyframes pet-blink {
    0%, 91%, 100% { transform: scaleY(1); }
    94%, 97% { transform: scaleY(0.12); }
  }
  @keyframes pet-glow {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
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
