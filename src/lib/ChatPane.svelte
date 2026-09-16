<script lang="ts">
  import { Channel, invoke } from "@tauri-apps/api/core";
  import { ArrowDown, ArrowUp, Bot, ChevronDown, History, RefreshCw, Sparkles, Square, Terminal, UserRound } from "@lucide/svelte";
  import { onMount, tick, untrack } from "svelte";
  import type { DialogRequest, DialogValue } from "./dialog";
  import { applyRpcEvent, contentText, emptyConversation, loadHistory, record, type RpcEvent } from "./rpc-state";
  import { captureTranscriptAnchor, restoreTranscriptAnchor, messageWindowStart, transcriptPort } from "./transcript-scroll";
  import MessageDisclosure from "./MessageDisclosure.svelte";
  import { messageSource } from "./message-parts";
  import { createRpcHistorySession, prependRpcHistory } from "./rpc-history";
  import { shortcutAria } from "./shortcuts";
  import { t, tm } from "$lib/i18n.svelte";

  const PROVIDER_LABELS: Record<string, string> = {
    deepseek: "DeepSeek",
    openai: "OpenAI",
    anthropic: "Anthropic",
    google: "Gemini",
    "google-vertex": "Gemini (Vertex)",
    "kimi-coding": "Kimi For Coding",
    opencode: "OpenCode",
    "github-copilot": "GitHub Copilot",
    openrouter: "OpenRouter",
    xai: "xAI",
  };

  function providerLabel(provider: string): string {
    return PROVIDER_LABELS[provider] ?? provider;
  }

  const THINKING_LABELS: Record<string, string> = {
    off: "关闭",
    minimal: "最简",
    low: "低",
    medium: "中",
    high: "高",
    xhigh: "超高",
    max: "最大",
  };

  interface Props {
    taskId: string;
    runId: string | null | undefined;
    title: string;
    visible: boolean;
    active: boolean;
    switching: boolean;
    focusToken: number;
    autoName: boolean;
    onUseTerminal: () => void;
    onDialog: (request: Omit<DialogRequest, "id" | "resolve">) => Promise<DialogValue>;
    onCancelDialogs: (scope: string) => void;
    onActivity: (busy: boolean) => void;
    onAutoRename: (title: string) => void;
    onOpenModelSettings?: () => void;
  }
  let { taskId, runId, title, visible, active, switching, focusToken, autoName, onUseTerminal, onDialog, onCancelDialogs, onActivity, onAutoRename, onOpenModelSettings }: Props = $props();
  let conversation = $state(emptyConversation());
  let draft = $state("");
  let restoredDraft = $state("");
  let initializing = $state(true);
  let connected = $state(false);
  let sending = $state(false);
  let stopping = $state(false);
  let error = $state("");
  let notice = $state("");
  let modelName = $state("");
  let models = $state<{ id: string; provider: string; name: string }[]>([]);
  let selectedModel = $state("");
  let changingModel = $state(false);
  let thinkingLevels = $state<string[]>([]);
  let thinkingLevel = $state("");
  let compacting = $state(false);
  let enhancing = $state(false);
  let changingThinking = $state(false);
  let autoNamed = $state(false);
  let sessionStats = $state<SessionStats | null>(null);
  let statsGeneration = 0;

  interface SessionStats {
    tokens: { input: number; output: number; cacheRead: number; cacheWrite: number; total: number } | null;
    cost: number | null;
    contextUsage: { tokens: number | null; contextWindow: number | null; percent: number | null } | null;
  }
  let streamingBehavior = $state<"followUp" | "steer">("followUp");
  let input = $state<HTMLTextAreaElement>();
  let transcript = $state<HTMLDivElement>();
  let transcriptContent = $state<HTMLDivElement>();
  let messageModule = $state.raw<Promise<typeof import("./ChatMessage.svelte")> | null>(null);
  let showingEarlier = $state(false);
  let followScroll = $state(true);
  let pinnedMessageStart = $state<number | null>(null);
  let historyLimit = $state(100);
  let historyOffset = $state(0);
  let historySession = $state.raw<ReturnType<typeof createRpcHistorySession> | null>(null);
  let reconnect = $state(0);
  let generation = 0;
  let composing = false;
  const messageStart = $derived(messageWindowStart(conversation.messages.length, historyLimit, pinnedMessageStart));
  const visibleMessages = $derived(conversation.messages.slice(messageStart));
  const toolResults = $derived(new Set(conversation.messages.map((message) => message.toolCallId).filter(Boolean)));
  const liveTools = $derived(Object.values(conversation.tools).filter((tool) => !toolResults.has(tool.id)));

  interface ChatTurn {
    /// 该 user 消息在 conversation.messages 中的下标
    index: number;
    /// 从 1 开始的轮次序号
    number: number;
    /// DOM 上 data-message-index 的值（= historyOffset + index）
    domIndex: number;
    text: string;
    summary: string;
    timestamp: number | null;
    timeLabel: string;
  }

  const TURN_SUMMARY_LIMIT = 40;
  /// 当前视口对应的轮次序号；0 表示尚未测量（自动跟随时按最新一轮处理）。
  let currentTurn = $state(0);
  let turnMenuOpen = $state(false);
  let turnMenu = $state<HTMLDivElement>();
  let turnTrigger = $state<HTMLButtonElement>();
  let flashTurnIndex = $state<number | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;
  /// 平滑滚动期间冻结「当前轮次」的测量，避免指示器在中间位置闪烁。
  let jumping = false;

  function turnSummary(text: string): string {
    const line = text.replace(/\s+/g, " ").trim();
    if (!line) return t("（空消息）");
    return line.length > TURN_SUMMARY_LIMIT ? `${line.slice(0, TURN_SUMMARY_LIMIT)}…` : line;
  }

  function turnTimeLabel(value: number | null): string {
    if (value === null) return "";
    const time = value < 1e12 ? value * 1000 : value;
    const minutes = Math.floor((Date.now() - time) / 60_000);
    if (minutes < 1) return t("刚刚");
    if (minutes < 60) return t("{count} 分钟前", { count: minutes });
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return t("{count} 小时前", { count: hours });
    const days = Math.floor(hours / 24);
    if (days < 30) return t("{count} 天前", { count: days });
    const date = new Date(time);
    return `${date.getMonth() + 1}-${date.getDate()} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
  }

  /// 用户轮次索引：遍历消息筛出 role === "user" 的消息并记录真实下标，派生而非手动同步。
  const turns = $derived.by(() => {
    const list: ChatTurn[] = [];
    conversation.messages.forEach((message, index) => {
      if (message.role !== "user") return;
      const text = contentText(message.content).trim();
      const timestamp = typeof message.timestamp === "number" ? message.timestamp : null;
      list.push({
        index,
        number: list.length + 1,
        domIndex: historyOffset + index,
        text,
        summary: turnSummary(text),
        timestamp,
        timeLabel: turnTimeLabel(timestamp),
      });
    });
    return list;
  });
  const turnCount = $derived(turns.length);
  /// 当前轮次：自动跟随最新时是最新一轮，否则取视口内最靠上的用户轮次（全部滚过则取最后一个）。
  const activeTurn = $derived(
    turnCount === 0 ? 0 : followScroll ? turnCount : Math.min(Math.max(currentTurn || 1, 1), turnCount),
  );

  /// 滚动时把当前轮次对齐到视口；跳转期间不做测量，由 jumpToTurn 断言结果。
  function syncCurrentTurn(container: HTMLDivElement, following: boolean) {
    if (jumping || !turns.length) return;
    if (following) { currentTurn = turns.length; return; }
    const rows = container.querySelectorAll<HTMLElement>("article.user-message[data-message-index]");
    if (!rows.length) return;
    const top = container.getBoundingClientRect().top;
    let chosen = rows[rows.length - 1];
    for (const row of rows) {
      if (row.getBoundingClientRect().bottom > top + 4) { chosen = row; break; }
    }
    const domIndex = Number(chosen.dataset.messageIndex);
    const turn = turns.find((candidate) => candidate.domIndex === domIndex);
    if (turn) currentTurn = turn.number;
  }

  /// 目标消息必须真的在 DOM 里才能滚过去：必要时把渲染窗口向前扩大。
  function ensureTurnRendered(messageIndex: number) {
    const total = conversation.messages.length;
    if (messageIndex < 0 || messageIndex >= total || messageIndex >= messageStart) return;
    // messageWindowStart 里 pinnedMessageStart 优先于 historyLimit，只调 historyLimit 不会真的扩大窗口，
    // 因此两者一起调整：把窗口下界压到目标消息、尾部保持全渲染（沿用既有 pinned 语义，仍受“显示更早的消息”兜底）。
    if (pinnedMessageStart === null || pinnedMessageStart > messageIndex) pinnedMessageStart = messageIndex;
    historyLimit = Math.max(historyLimit, total - messageIndex);
  }

  async function jumpToTurn(messageIndex: number) {
    const turn = turns.find((candidate) => candidate.index === messageIndex);
    const container = transcript;
    if (!turn || !container) return;
    const current = generation;
    followScroll = false;              // 关掉自动跟随，避免新内容把视口拉回底部
    turnMenuOpen = false;
    ensureTurnRendered(messageIndex);
    currentTurn = turn.number;         // 断言被跳转的轮次就是当前轮次
    flashTurnIndex = turn.domIndex;
    jumping = true;
    await tick();
    if (current === generation) {
      container.querySelector<HTMLElement>(`[data-message-index="${turn.domIndex}"]`)
        ?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
    if (flashTimer) clearTimeout(flashTimer);
    flashTimer = setTimeout(() => {
      flashTimer = null;
      flashTurnIndex = null;
      jumping = false;
    }, 1200);
  }

  function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
  }

  /// Alt+↑ / Alt+↓ 在用户轮次之间跳转，Esc 关闭下拉；焦点在输入类元素里时不拦截。
  function handleWindowKeydown(event: KeyboardEvent) {
    if (!visible) return;
    if (event.key === "Escape") {
      if (turnMenuOpen) { event.preventDefault(); turnMenuOpen = false; }
      return;
    }
    if (!active || !event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || event.isComposing) return;
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    if (isEditableTarget(event.target)) return;
    const next = turns[activeTurn - 1 + (event.key === "ArrowUp" ? -1 : 1)];
    if (!next) return;                 // 第一轮 / 最后一轮：不做任何事
    event.preventDefault();
    void jumpToTurn(next.index);
  }

  function closeTurnMenu(event: PointerEvent) {
    if (!turnMenuOpen || !turnMenu) return;
    if (!(event.target instanceof Node) || turnMenu.contains(event.target) || event.target === turnTrigger) return;
    turnMenuOpen = false;
  }

  onMount(() => {
    document.addEventListener("pointerdown", closeTurnMenu, true);
    window.addEventListener("keydown", handleWindowKeydown);
    return () => {
      document.removeEventListener("pointerdown", closeTurnMenu, true);
      window.removeEventListener("keydown", handleWindowKeydown);
      if (flashTimer) { clearTimeout(flashTimer); flashTimer = null; }
    };
  });
  const canSend = $derived(!switching && connected && !initializing && !sending && !stopping && !conversation.closed && !!draft.trim());
  const groupedModels = $derived.by(() => {
    const groups = new Map<string, typeof models>();
    for (const model of models) {
      const list = groups.get(model.provider);
      if (list) list.push(model);
      else groups.set(model.provider, [model]);
    }
    return [...groups.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([provider, groupModels]) => ({
        provider,
        label: providerLabel(provider),
        models: groupModels,
      }));
  });
  const RING_CIRCUMFERENCE = 2 * Math.PI * 13;
  const contextPercent = $derived(sessionStats?.contextUsage?.percent ?? null);
  const ringLabel = $derived(contextPercent === null ? "—" : `${Math.round(contextPercent)}%`);
  const ringDash = $derived(
    `${(contextPercent === null ? 0 : Math.min(1, Math.max(0, contextPercent / 100))) * RING_CIRCUMFERENCE} ${RING_CIRCUMFERENCE}`,
  );
  const ringTitle = $derived(
    contextPercent === null
      ? t("上下文用量未知，点击压缩上下文")
      : t("上下文已用 {percent}%，点击压缩上下文", { percent: Math.round(contextPercent) }),
  );
  const canCompact = $derived(connected && !switching && !conversation.busy && !conversation.closed && !compacting);
  /// 只有“关闭”一档时说明该模型没有可调推理强度，此时不显示推理强度选择器。
  const reasoningLevels = $derived(thinkingLevels.filter((level) => level !== "off"));
  /// 空闲（等待输入）不显示状态文字，避免噪音；仅在异常/进行中状态提示。
  const statusText = $derived(
    switching ? t("正在切换模式")
      : conversation.closed ? t("已停止")
        : initializing ? t("连接中")
          : !connected ? t("未连接")
            : conversation.busy ? t("运行中")
              : "",
  );
  const canEnhance = $derived(connected && !switching && !conversation.busy && !conversation.closed && !!draft.trim() && !enhancing);

  /// 压缩上下文：调用 Pi 原生 `compact` RPC，把历史消息总结为摘要。
  async function compactContext() {
    if (!canCompact) return;
    const current = generation;
    const scope = `rpc:${taskId}:${runId ?? ""}`;
    try {
      const accepted = await onDialog({
        scope, kind: "confirm", title: t("压缩上下文"),
        message: t("压缩会把历史消息总结为摘要以释放上下文窗口。继续吗？"),
        confirmLabel: t("压缩"),
      });
      if (!accepted || current !== generation) return;
      compacting = true;
      await call({ type: "compact" });
      if (current === generation) { notice = t("上下文已压缩。"); refreshStats(); }
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) compacting = false;
    }
  }

  /// 提示词增强：调用宿主 LLM 把草稿改写成更清晰的提示词，确认后替换草稿。
  async function enhanceDraft() {
    if (!canEnhance) return;
    const current = generation;
    enhancing = true;
    try {
      const result = await invoke<{ text: string }>("enhance_prompt", { request: { text: draft } });
      if (current !== generation) return;
      const accepted = await onDialog({
        scope: `rpc:${taskId}:${runId ?? ""}`, kind: "confirm",
        title: t("使用增强后的提示词"), message: result.text, confirmLabel: t("替换草稿"),
      });
      if (accepted && current === generation) draft = result.text;
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) enhancing = false;
    }
  }

  $effect(() => {
    if (conversation.messages.length && !messageModule) {
      messageModule = import("./ChatMessage.svelte");
      void messageModule.catch(() => {});
    }
  });

  $effect(() => {
    const run = runId;
    const id = taskId;
    void reconnect;
    const current = ++generation;
    const scope = `rpc:${id}:${run}`;
    historyLimit = 100;
    followScroll = true;
    pinnedMessageStart = null;
    let alive = true;
    let ready = false;
    let buffered: RpcEvent[] = [];
    let bufferedBytes = 0;
    let overflow = false;
    initializing = true;
    connected = false;
    sending = stopping = false;
    changingModel = false;
    changingThinking = false;
    thinkingLevels = [];
    thinkingLevel = "";
    autoNamed = false;
    sessionStats = null;
    models = [];
    modelName = "";
    selectedModel = "";
    conversation = emptyConversation();
    historyOffset = 0;
    error = "";
    if (!run) { initializing = false; conversation = { ...emptyConversation(), closed: true }; return; }
    const historyReader = createRpcHistorySession(invoke, id, run);
    historySession = historyReader;
    const call = <T,>(command: Record<string, unknown>) =>
      invoke<T>("rpc_command", { taskId: id, runId: run, command });
    const channel = new Channel<RpcEvent>();
    channel.onmessage = (event) => {
      if (!alive) return;
      if (event.payload.type === "extension_ui_request") {
        void handleExtension(event.payload, scope, call, () => alive);
      }
      if (event.payload.type === "rpc_exit") {
        connected = false;
        onCancelDialogs(scope);
      }
      if (!ready) {
        bufferedBytes += JSON.stringify(event).length;
        if (bufferedBytes > 8 * 1024 * 1024) { buffered = []; bufferedBytes = 0; overflow = true; }
        buffered.push(event);
      } else {
        conversation = applyRpcEvent(conversation, event);
        if (event.payload.type === "agent_start" || event.payload.type === "agent_settled") {
          onActivity(conversation.busy);
          if (event.payload.type === "agent_settled") { void call({ type: "get_state" }).catch(() => {}); refreshStats(); }
        }
      }
    };
    void (async () => {
      try {
        await invoke<number>("subscribe_rpc", { taskId: id, runId: run, channel });
        if (!alive) return;
        const history = await historyReader.open();
        if (!alive || !history) return;
        const state = await call<Record<string, unknown>>({ type: "get_state" });
        if (!alive) return;
        if (overflow) throw new Error(t("初始化期间事件过多，请重新连接以读取完整会话"));
        let next = loadHistory(emptyConversation(), history.messages, history.eventSequence);
        next.busy = state.isStreaming === true || state.isCompacting === true;
        for (const event of buffered) next = applyRpcEvent(next, event);
        buffered = [];
        ready = true;
        connected = !next.closed;
        conversation = next;
        historyOffset = history.start;
        modelName = String(record(state.model).name ?? record(state.model).id ?? t("未选择模型"));
        selectedModel = `${String(record(state.model).provider ?? "")}/${String(record(state.model).id ?? "")}`;
        thinkingLevel = typeof state.thinkingLevel === "string" ? state.thinkingLevel : "";
        onActivity(next.busy);
        refreshStats();
        void call<{ levels: unknown[] }>({ type: "get_available_thinking_levels" }).then((result) => {
          if (!alive) return;
          thinkingLevels = result.levels.map((level) => String(level)).filter(Boolean);
        }).catch(() => {});
        void refreshModels(() => alive);
      } catch (cause) {
        if (alive) error = tm(String(cause));
      } finally {
        if (alive) initializing = false;
      }
    })();
    return () => {
      alive = false;
      buffered = [];
      untrack(() => onCancelDialogs(scope));
      if (current === generation) generation++;
      void historyReader.dispose();
      untrack(() => { if (historySession === historyReader) historySession = null; });
    };
  });

  $effect(() => {
    if (focusToken > 0 && visible && active) input?.focus();
  });
  $effect(() => {
    const container = transcript;
    const content = transcriptContent;
    if (!container || !content || !visible) return;
    let frame = 0;
    const observer = new ResizeObserver(() => {
      if (!followScroll) return;
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (followScroll && visible) container.scrollTop = container.scrollHeight;
      });
    });
    observer.observe(content);
    return () => { observer.disconnect(); cancelAnimationFrame(frame); };
  });

  async function showEarlier() {
    if (showingEarlier || !transcript) return;
    const current = generation;
    showingEarlier = true;
    followScroll = false;
    pinnedMessageStart = messageStart;
    try {
      if (messageStart === 0 && historyOffset > 0) {
        const page = await historySession?.older();
        if (current !== generation || !page || !transcript) return;
        const port = transcriptPort(transcript);
        const anchor = captureTranscriptAnchor(port);
        conversation = prependRpcHistory(conversation, page, historyOffset);
        historyOffset = page.start;
        if (!followScroll) {
          pinnedMessageStart = 0;
          historyLimit = conversation.messages.length;
        }
        await tick();
        if (current === generation && !followScroll) restoreTranscriptAnchor(port, anchor);
        return;
      }
      const port = transcriptPort(transcript);
      const anchor = captureTranscriptAnchor(port);
      pinnedMessageStart = Math.max(0, messageStart - 100);
      historyLimit = conversation.messages.length - pinnedMessageStart;
      await tick();
      if (current === generation && !followScroll) restoreTranscriptAnchor(port, anchor);
    } catch (cause) { if (current === generation) error = tm(String(cause)); }
    finally { showingEarlier = false; }
  }

  async function openMessageLink(url: string) {
    const current = generation;
    const scope = `rpc:${taskId}:${runId}`;
    const closed = conversation.closed;
    try {
      const [{ confirmMessageLink }, { openUrl }] = await Promise.all([
        import("./message-markdown"), import("@tauri-apps/plugin-opener"),
      ]);
      await confirmMessageLink(url, {
        current: () => current === generation && conversation.closed === closed,
        confirm: async (destination) => await onDialog({
          scope, kind: "confirm", title: t("打开外部链接"), message: destination, confirmLabel: t("打开"),
        }) === true,
        open: openUrl,
      });
    } catch (cause) { if (current === generation) error = tm(String(cause)); }
  }

  function updateScroll(event: Event & { currentTarget: HTMLDivElement }) {
    const element = event.currentTarget;
    const following = element.scrollHeight - element.scrollTop - element.clientHeight < 80;
    if (following) pinnedMessageStart = null;
    else if (pinnedMessageStart === null) pinnedMessageStart = messageStart;
    followScroll = following;
    syncCurrentTurn(element, following);
  }
  $effect(() => {
    void conversation.sequence;
    if (followScroll && visible) void tick().then(() => {
      if (transcript && followScroll) transcript.scrollTop = transcript.scrollHeight;
    });
  });

  function call<T>(command: Record<string, unknown>): Promise<T> {
    return invoke<T>("rpc_command", { taskId, runId, command });
  }

  function commandError(cause: unknown) {
    error = tm(String(cause));
    if (error.includes("RPC_OUTCOME_UNKNOWN:")) connected = false;
  }

  function refreshStats() {
    const current = ++statsGeneration;
    void call<Record<string, unknown>>({ type: "get_session_stats" }).then((result) => {
      if (current !== statsGeneration) return;
      const tokens = record(result.tokens);
      sessionStats = {
        tokens: {
          input: Number(tokens.input ?? 0),
          output: Number(tokens.output ?? 0),
          cacheRead: Number(tokens.cacheRead ?? 0),
          cacheWrite: Number(tokens.cacheWrite ?? 0),
          total: Number(tokens.total ?? 0),
        },
        cost: typeof result.cost === "number" ? result.cost : null,
        contextUsage: result.contextUsage
          ? {
              tokens: Number(record(result.contextUsage).tokens ?? 0) || null,
              contextWindow: Number(record(result.contextUsage).contextWindow ?? 0) || null,
              percent: Number(record(result.contextUsage).percent ?? 0) || null,
            }
          : null,
      };
    }).catch(() => {});
  }

  const cacheHitRate = $derived.by(() => {
    const tokens = sessionStats?.tokens;
    if (!tokens) return null;
    const denominator = tokens.input + tokens.cacheRead + tokens.cacheWrite;
    if (denominator <= 0) return null;
    return tokens.cacheRead / denominator;
  });

  function formatTokens(value: number | null): string {
    if (value === null) return "-";
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(value >= 10_000 ? 0 : 1)}K`;
    return String(value);
  }

  function formatCost(value: number | null): string {
    if (value === null) return "-";
    if (value >= 1) return `$${value.toFixed(2)}`;
    if (value > 0) return `$${value.toFixed(4)}`;
    return "$0";
  }

  function deriveTitle(): string | null {
    const first = conversation.messages.find((message) => message.role === "user");
    if (!first) return null;
    const line = contentText(first.content).split("\n").map((part) => part.trim()).find((part) => part.length > 0);
    if (!line) return null;
    return line.length > 24 ? `${line.slice(0, 24)}…` : line;
  }

  $effect(() => {
    if (!autoName || autoNamed || conversation.busy) return;
    const messages = conversation.messages;
    if (!messages.some((message) => message.role === "user")) return;
    if (!messages.some((message) => message.role !== "user" && message.role !== "toolResult")) return;
    const next = deriveTitle();
    if (!next) return;
    autoNamed = true;
    void call({ type: "set_session_name", name: next }).catch(() => {});
    onAutoRename(next);
  });


  async function changeModel(value: string) {
    const model = models.find((model) => `${model.provider}/${model.id}` === value);
    if (!model || changingModel) return;
    const current = generation;
    changingModel = true;
    try {
      await call({ type: "set_model", provider: model.provider, modelId: model.id });
      if (current === generation) { selectedModel = value; modelName = model.name; error = ""; }
      void call<{ levels: unknown[] }>({ type: "get_available_thinking_levels" }).then((result) => {
        if (current !== generation) return;
        thinkingLevels = result.levels.map((level) => String(level)).filter(Boolean);
        if (!thinkingLevels.includes(thinkingLevel)) thinkingLevel = thinkingLevels[0] ?? "";
      }).catch(() => {});
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) changingModel = false;
    }
  }

  /// 读取当前会话可用的模型列表；列表为空时由空状态提示兜底。
  async function refreshModels(alive?: () => boolean) {
    const current = generation;
    try {
      const result = await call<{ models: unknown[] }>({ type: "get_available_models" });
      if (alive && !alive()) return;
      if (current !== generation) return;
      models = result.models
        .map(record)
        .filter((model) => typeof model.id === "string" && typeof model.provider === "string")
        .map((model) => ({ id: String(model.id), provider: String(model.provider), name: String(model.name ?? model.id) }));
    } catch {
      // 静默失败：列表缺失时显示空状态提示，不打断会话。
    }
  }

  async function changeThinkingLevel(level: string) {
    if (!level || changingThinking) return;
    const current = generation;
    changingThinking = true;
    try {
      await call({ type: "set_thinking_level", level });
      if (current === generation) { thinkingLevel = level; error = ""; }
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) changingThinking = false;
    }
  }

  async function send() {
    if (!canSend) return;
    const text = draft;
    const current = generation;
    sending = true;
    error = "";
    try {
      await call({ type: "prompt", message: text, streamingBehavior });
      if (current !== generation) return;
      if (draft === text) draft = "";
      followScroll = true;
      pinnedMessageStart = null;
      refreshStats();
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) sending = false;
    }
  }

  async function interrupt() {
    if (stopping || conversation.closed) return;
    const current = generation;
    stopping = true;
    try {
      const queue = await call<{ steering?: string[]; followUp?: string[] }>({ type: "clear_queue" });
      if (current !== generation) return;
      const text = [...queue.steering ?? [], ...queue.followUp ?? []].join("\n\n");
      if (text) {
        restoredDraft = text;
        if (!draft) { draft = text; restoredDraft = ""; }
      }
      await call({ type: "abort" });
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) stopping = false;
    }
  }

  async function handleExtension(
    request: Record<string, unknown>, scope: string,
    sendResponse: <T>(command: Record<string, unknown>) => Promise<T>, alive: () => boolean,
  ) {
    const method = request.method;
    if (method === "notify") { notice = String(request.message ?? ""); return; }
    if (method === "setStatus") { notice = String(request.text ?? ""); return; }
    if (method === "setWidget") { notice = Array.isArray(request.lines) ? request.lines.join("\n") : ""; return; }
    if (method === "setTitle") return;
    if (method === "set_editor_text") {
      if (draft) restoredDraft = draft;
      draft = String(request.text ?? "");
      return;
    }
    if (typeof request.id !== "string") return;
    const response: Record<string, unknown> = { type: "extension_ui_response", id: request.id };
    if (!["select", "confirm", "input", "editor"].includes(String(method))) {
      notice = t("此扩展交互需要终端兼容模式");
      response.cancelled = true;
    } else {
      const value = await onDialog({
        scope,
        kind: method === "select" ? "choice" : method === "confirm" ? "confirm" : "input",
        title: `${title} · ${String(request.title ?? t("扩展请求"))}`,
        message: String(request.message ?? ""),
        initialValue: String(request.prefill ?? ""),
        placeholder: String(request.placeholder ?? ""),
        choices: Array.isArray(request.options) ? request.options.filter((option): option is string => typeof option === "string").map((option) => ({ value: option, label: option })) : undefined,
        multiline: method === "editor",
        maxLength: 131072,
        timeoutMs: typeof request.timeout === "number" ? request.timeout : undefined,
      });
      if (!alive()) return;
      if (value === null) response.cancelled = true;
      else if (method === "confirm") response.confirmed = value === true;
      else response.value = value;
    }
    try { await sendResponse(response); } catch (cause) { if (alive()) error = tm(String(cause)); }
  }
</script>

<section class="terminal-pane chat-pane" class:hidden={!visible} class:active aria-label={title}>
  <header>
    <Bot size={16} /><strong title={title}>{title}</strong>
    {#if sessionStats?.tokens}
      <details class="stats">
        <summary title={t("Token 统计（点击查看明细）")}>
          <span class="stat">↑{formatTokens(sessionStats.tokens.input)}</span>
          <span class="stat">↓{formatTokens(sessionStats.tokens.output)}</span>
          {#if cacheHitRate !== null}<span class="stat hit">{t("缓存 {percent}%", { percent: Math.round(cacheHitRate * 100) })}</span>{/if}
          {#if sessionStats.cost !== null}<span class="stat cost">{formatCost(sessionStats.cost)}</span>{/if}
        </summary>
        <div class="stats-panel">
          <dl>
            <dt>{t("输入")}</dt><dd>{formatTokens(sessionStats.tokens.input)}</dd>
            <dt>{t("输出")}</dt><dd>{formatTokens(sessionStats.tokens.output)}</dd>
            <dt>{t("缓存读")}</dt><dd>{formatTokens(sessionStats.tokens.cacheRead)}</dd>
            <dt>{t("缓存写")}</dt><dd>{formatTokens(sessionStats.tokens.cacheWrite)}</dd>
            <dt>{t("合计")}</dt><dd>{formatTokens(sessionStats.tokens.total)}</dd>
            <dt>{t("缓存命中率")}</dt><dd>{cacheHitRate === null ? "-" : `${(cacheHitRate * 100).toFixed(1)}%`}</dd>
            <dt>{t("花费")}</dt><dd>{formatCost(sessionStats.cost)}</dd>
            {#if sessionStats.contextUsage?.percent !== null && sessionStats.contextUsage?.percent !== undefined}
              <dt>{t("上下文占用")}</dt><dd>{sessionStats.contextUsage.percent}%{sessionStats.contextUsage.tokens ? ` · ${formatTokens(sessionStats.contextUsage.tokens)}` : ""}</dd>
            {/if}
          </dl>
        </div>
      </details>
    {/if}
    <span class="model-name" title={modelName}>{modelName}</span>
    {#if turnCount > 0}
      <div class="turn-nav">
        <button type="button" class="turn-trigger" bind:this={turnTrigger} aria-haspopup="true"
          aria-expanded={turnMenuOpen} aria-label={t("对话历史跳转，当前第 {current} 轮，共 {total} 轮", { current: activeTurn, total: turnCount })}
          title={t("对话历史跳转（Alt+↑ / Alt+↓）")}
          onclick={() => { turnMenuOpen = !turnMenuOpen; }}>
          <History size={13} />
          <span class="turn-label">{t("对话历史 · 第 {current} 轮 / 共 {total} 轮", { current: activeTurn, total: turnCount })}</span>
          <ChevronDown size={13} />
        </button>
        {#if turnMenuOpen}
          <div class="turn-panel" bind:this={turnMenu}>
            <p class="turn-panel-head">{t("共 {total} 轮 · Alt+↑ / Alt+↓ 快速切换", { total: turnCount })}</p>
            <ul>
              {#each turns as turn (turn.index)}
                <li>
                  <button type="button" class:active={turn.number === activeTurn} title={turn.text || turn.summary}
                    onclick={() => void jumpToTurn(turn.index)}>
                    <span class="turn-no">#{turn.number}</span>
                    <span class="turn-summary">{turn.summary}</span>
                    {#if turn.timeLabel}<span class="turn-time">{turn.timeLabel}</span>{/if}
                  </button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {/if}
    <button type="button" disabled={switching} aria-label={t("切换到终端兼容模式")} title={t("终端兼容模式")} onclick={onUseTerminal}><Terminal size={16} /></button>
  </header>
  <div class="transcript" bind:this={transcript} role="log" aria-label={t("任务对话")} aria-live="off"
    onscroll={updateScroll}>
    <div class="transcript-content" bind:this={transcriptContent}>
    {#if messageStart > 0 || historyOffset > 0}
      <button type="button" class="more-history" disabled={showingEarlier} onclick={showEarlier}>{t("显示更早的消息")}</button>
    {/if}
    {#each visibleMessages as message, index (historyOffset + conversation.messages.length - visibleMessages.length + index)}
      {@const messageIndex = historyOffset + conversation.messages.length - visibleMessages.length + index}
      <article data-message-index={messageIndex} class:user-message={message.role === "user"} class:tool-message={message.role === "toolResult"} class:jump-flash={flashTurnIndex === messageIndex}>
        <div class="message-label">
          {#if message.role === "user"}<UserRound size={14} />{t("你")}
          {:else if message.role === "toolResult"}<Terminal size={14} />{message.toolName ?? t("工具结果")}
          {:else}<Bot size={14} />Pi{/if}
        </div>
        {#if message.role === "toolResult"}
          <MessageDisclosure title={message.toolName ?? t("工具输出")}>
            {#if messageModule}
              {#await messageModule}
                <pre>{messageSource(message.content)}</pre>
              {:then module}<module.default {message} onOpenLink={openMessageLink} />
              {:catch}<pre>{messageSource(message.content)}</pre>{/await}
            {/if}
          </MessageDisclosure>
        {:else}
          <div class="message-content">
            {#if messageModule}
              {#await messageModule}
                <div class="plain-message">{messageSource(message.content)}</div>
              {:then module}<module.default {message} onOpenLink={openMessageLink} />
              {:catch}
                <div class="plain-message">{messageSource(message.content)}</div>
                <button type="button" title={t("重新加载消息显示")} aria-label={t("重新加载消息显示")} onclick={() => { messageModule = null; }}><RefreshCw size={15} /></button>
              {/await}
            {:else}<div class="plain-message">{messageSource(message.content)}</div>{/if}
          </div>
        {/if}
      </article>
    {/each}
    {#each liveTools as tool (tool.id)}
      <details class="tool-call" open={tool.running}>
        <summary><Terminal size={14} />{tool.name}<span>{tool.running ? t("执行中") : tool.isError ? t("失败") : t("完成")}</span></summary>
        <pre>{JSON.stringify(tool.args, null, 2)}</pre>
        {#if tool.result}<pre>{contentText(record(tool.result).content)}</pre>{/if}
      </details>
    {/each}
    {#if initializing}<p role="status">{t("正在连接 Pi 会话…")}</p>{/if}
    {#if !initializing && !conversation.messages.length && !error}
      <div class="empty-conversation"><Bot size={28} /><h2>{title}</h2></div>
    {/if}
    </div>
  </div>
  {#if turnCount >= 2}
    <div class="turn-rail" role="toolbar" aria-label={t("对话历史快速跳转")} aria-orientation="vertical">
      {#each turns as turn (turn.index)}
        <button type="button" class="turn-dot" class:active={turn.number === activeTurn}
          title={t("第 {number} 轮 · {summary}", { number: turn.number, summary: turn.summary })} aria-label={t("跳转到第 {number} 轮：{summary}", { number: turn.number, summary: turn.summary })}
          aria-current={turn.number === activeTurn ? "true" : undefined}
          onclick={() => void jumpToTurn(turn.index)}></button>
      {/each}
    </div>
  {/if}
  {#if !followScroll && conversation.messages.length}
    <div class="scroll-actions">
      <button type="button" title={t("回到最新消息")} aria-label={t("回到最新消息")} onclick={() => {
        followScroll = true;
        pinnedMessageStart = null;
        if (transcript) transcript.scrollTop = transcript.scrollHeight;
      }}><ArrowDown size={16} /></button>
    </div>
  {/if}
  {#if error || conversation.error}
    <div class="chat-error" role="alert"><span>{tm(error || conversation.error)}</span>
      <button type="button" title={t("重新连接会话")} aria-label={t("重新连接会话")} onclick={() => { reconnect++; }}><RefreshCw size={15} /></button>
    </div>
  {/if}
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if conversation.queue.length}<p class="notice" role="status">{t("{count} 条消息排队中", { count: conversation.queue.length })}</p>{/if}
  {#if restoredDraft}
    <button class="restore-draft" type="button" onclick={() => { draft = [draft, restoredDraft].filter(Boolean).join("\n\n"); restoredDraft = ""; }}>{t("恢复暂存文本")}</button>
  {/if}
  {#if connected && !initializing && !conversation.closed && models.length === 0}
    <p class="model-hint" role="status">
      <span>{t("未检测到可用模型：请在 设置 → 模型与凭据 中添加并保存模型，重启本任务后生效。")}</span>
      <span class="model-hint-actions">
        {#if onOpenModelSettings}<button type="button" onclick={onOpenModelSettings}>{t("打开模型设置")}</button>{/if}
        <button type="button" onclick={() => void refreshModels()}>{t("重试")}</button>
      </span>
    </p>
  {/if}
  <form class="composer" onsubmit={(event) => { event.preventDefault(); void send(); }}>
    <textarea bind:this={input} bind:value={draft} rows="3" maxlength="131072" aria-keyshortcuts={shortcutAria("composer")}
      aria-label={t("发送给 Pi")} placeholder={conversation.closed ? t("会话已停止") : t("发送消息")}
      disabled={conversation.closed}
      oncompositionstart={() => { composing = true; }} oncompositionend={() => { composing = false; }}
      onkeydown={(event) => {
        if (event.isComposing || composing) return;
        if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void send(); }
      }}></textarea>
    <div class="composer-actions">
      {#if models.length}
        <select aria-label={t("对话模型")} value={selectedModel} disabled={switching || !connected || changingModel || conversation.busy}
          onchange={(event) => {
            const select = event.currentTarget;
            void changeModel(select.value).then(() => { select.value = selectedModel; });
          }}>
          {#if !models.some((model) => `${model.provider}/${model.id}` === selectedModel)}
            <option value={selectedModel} disabled>{modelName}</option>
          {/if}
          {#each groupedModels as group (group.provider)}
            <optgroup label={group.label}>
              {#each group.models as model (`${model.provider}/${model.id}`)}
                <option value={`${model.provider}/${model.id}`}>{model.name}</option>
              {/each}
            </optgroup>
          {/each}
        </select>
      {:else if connected && !initializing && !conversation.closed}
        <select aria-label={t("对话模型")} disabled>
          <option value="">{t("无可用模型")}</option>
        </select>
      {/if}
      {#if reasoningLevels.length > 0}
        <select aria-label={t("推理强度")} value={thinkingLevel} disabled={switching || !connected || changingThinking || conversation.busy}
          onchange={(event) => {
            const select = event.currentTarget;
            void changeThinkingLevel(select.value).then(() => { select.value = thinkingLevel; });
          }}>
          {#if !thinkingLevels.includes(thinkingLevel)}
            <option value={thinkingLevel} disabled>{t(THINKING_LABELS[thinkingLevel] ?? (thinkingLevel || "默认"))}</option>
          {/if}
          {#each thinkingLevels as level (level)}
            <option value={level}>{t(THINKING_LABELS[level] ?? level)}</option>
          {/each}
        </select>
      {/if}
      {#if connected && !initializing}
        <button type="button" class="context-ring" title={ringTitle}
          aria-label={t("上下文用量 {percent}，点击压缩上下文", { percent: ringLabel })} disabled={!canCompact}
          onclick={() => void compactContext()}>
          <svg viewBox="0 0 32 32" aria-hidden="true">
            <circle class="ring-bg" cx="16" cy="16" r="13" />
            <circle class="ring-fg" cx="16" cy="16" r="13" style={`stroke-dasharray:${ringDash}`} />
          </svg>
        </button>
      {/if}
      <select aria-label={t("运行时消息处理方式")} bind:value={streamingBehavior}>
        <option value="followUp">{t("排队跟进")}</option><option value="steer">{t("优先引导")}</option>
      </select>
      <span class="connection-status" role="status">{statusText}</span>
      {#if conversation.busy || conversation.queue.length}
        <button type="button" class="send-button" disabled={switching || stopping} title={t("停止当前响应并清空队列")} aria-label={t("停止当前响应并清空队列")} onclick={() => void interrupt()}><Square size={16} /></button>
      {/if}
      <button type="button" class="send-button" title={t("增强提示词（改写为更清晰的结构化提示）")} aria-label={t("增强提示词")}
        disabled={!canEnhance} onclick={() => void enhanceDraft()}>
        {#if enhancing}<span class="spin"><RefreshCw size={14} /></span>{:else}<Sparkles size={15} />{/if}
      </button>
      <button type="submit" class="send-button primary-send" disabled={!canSend} title={t("发送消息")} aria-label={t("发送消息")}><ArrowUp size={18} /></button>
    </div>
  </form>
</section>

<style>
  .chat-pane { position: relative; display: flex; flex-direction: column; grid-template-rows: none; min-height: 0; min-width: 0; border: 1px solid var(--border); background: var(--page-bg); overflow: hidden; }
  .chat-pane.hidden { display: none; }
  header { display: flex; align-items: center; gap: 8px; flex-shrink: 0; min-height: 38px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--surface); }
  header strong { flex: 1; font-size: 12px; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .model-name { max-width: 35%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); font-size: 11px; }
  .stats { position: relative; }
  .stats summary { display: inline-flex; align-items: center; gap: 8px; padding: 2px 8px; border: 1px solid transparent; border-radius: 4px; cursor: pointer; list-style: none; }
  .stats summary::-webkit-details-marker { display: none; }
  .stats summary:hover { border-color: var(--border); background: var(--surface-hover); }
  .stat { color: var(--text-muted); font-size: 11px; font-family: var(--code-font); white-space: nowrap; }
  .stat.hit { color: var(--accent); }
  .stat.cost { color: #d8b45a; }
  .stats-panel { position: absolute; top: calc(100% + 6px); right: 0; z-index: 12; min-width: 220px; padding: 10px 12px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface-raised); box-shadow: 0 6px 20px #0005; }
  .stats-panel dl { display: grid; grid-template-columns: auto 1fr; gap: 4px 14px; margin: 0; font-size: 11px; }
  .stats-panel dt { color: var(--text-muted); }
  .stats-panel dd { margin: 0; text-align: right; font-family: var(--code-font); color: var(--text); }
  button { cursor: pointer; }
  header button, .chat-error button { display: grid; place-items: center; width: 26px; height: 26px; padding: 0; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); flex-shrink: 0; }
  .transcript { flex: 1 1 0; min-height: 0; overflow: auto; padding: 16px 24px 156px; overflow-anchor: none; }
  article { max-width: 900px; margin: 0 auto 24px; outline: 2px solid transparent; outline-offset: 3px; transition: outline-color .3s ease; }
  .message-label { display: flex; align-items: center; gap: 6px; margin-bottom: 8px; font-size: 12px; color: var(--text-muted); }
  .message-content { min-width: 0; overflow-wrap: anywhere; line-height: 1.7; color: var(--text); font-family: var(--session-font, var(--text-font)); font-size: var(--session-font-size, 13px); }
  .plain-message { white-space: pre-wrap; }
  .scroll-actions { height: 0; position: relative; display: flex; justify-content: center; z-index: 1; }
  .scroll-actions button { position: absolute; bottom: 150px; width: 30px; height: 30px; display: grid; place-items: center; padding: 0; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); }
  .user-message .message-content { padding: 12px; border-left: 2px solid var(--accent); background: var(--surface); }
  .tool-call { border: 1px solid var(--border); border-radius: 4px; padding: 8px 12px; margin-bottom: 8px; font-size: 12px; }
  summary { cursor: pointer; overflow-wrap: anywhere; }
  summary :global(svg) { vertical-align: middle; margin-right: 6px; }
  summary span { margin-left: 12px; color: var(--text-muted); }
  pre { overflow: auto; max-height: 280px; font: 12px/1.5 var(--code-font); tab-size: 4; }
  .empty-conversation { display: grid; place-content: center; justify-items: center; min-height: 180px; color: var(--text-muted); }
  h2 { font-size: 16px; font-weight: 500; overflow-wrap: anywhere; }
  /* 悬浮指令坞：磨砂胶囊，浮在会话内容之上，不挤压阅读区。 */
  .composer {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 16px;
    z-index: 6;
    width: min(760px, calc(100% - 32px));
    margin: 0 auto;
    padding: 8px 10px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 16px;
    background: color-mix(in srgb, var(--surface) 84%, transparent);
    backdrop-filter: blur(20px) saturate(180%);
    -webkit-backdrop-filter: blur(20px) saturate(180%);
    box-shadow: 0 16px 40px #0009;
  }
  .composer-actions { display: flex; align-items: center; gap: 8px; margin-top: 4px; padding: 6px 2px 2px; border-top: 1px solid color-mix(in srgb, var(--text) 8%, transparent); }
  textarea { display: block; width: 100%; min-height: 56px; max-height: 240px; resize: vertical; padding: 8px 6px; border: 0; background: transparent; color: var(--text); font: 13px/1.5 var(--session-font, var(--text-font)); }
  .spin { display: inline-grid; animation: chat-spin 0.8s linear infinite; }
  @keyframes chat-spin { to { transform: rotate(360deg); } }
  .context-ring { position: relative; display: grid; place-items: center; width: 17px; height: 17px; flex-shrink: 0; padding: 0; border: 0; border-radius: 50%; background: transparent; cursor: pointer; }
  .context-ring:disabled { cursor: default; opacity: .6; }
  .context-ring svg { width: 16px; height: 16px; transform: rotate(-90deg); }
  .ring-bg { fill: none; stroke: var(--surface-hover); stroke-width: 3.4; }
  .ring-fg { fill: none; stroke: var(--accent); stroke-linecap: round; transition: stroke-dasharray .4s ease, stroke .3s ease; }
  .context-ring:hover:not(:disabled) .ring-fg { stroke: var(--accent); filter: brightness(1.15); }
  select { min-width: 0; max-width: 130px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface-alt); padding: 4px; color: var(--text-muted); font-size: 11px; }
  .connection-status { flex: 1; font-size: 11px; color: var(--text-muted); }
  .send-button { display: grid; place-items: center; width: 30px; height: 30px; padding: 0; flex-shrink: 0; border: 0; border-radius: 4px; background: var(--surface-hover); color: var(--text); }
  .primary-send { background: var(--accent); color: var(--accent-ink); }
  button:disabled { opacity: .4; cursor: default; }
  .chat-error { flex-shrink: 0; display: flex; align-items: center; gap: 8px; margin: 4px 16px 8px; padding: 8px; border: 1px solid #bd5147; border-radius: 4px; color: #d46b61; overflow-wrap: anywhere; }
  .chat-error span { flex: 1; min-width: 0; }
  .notice { flex-shrink: 0; margin: 4px 16px 8px; color: var(--text-muted); white-space: pre-wrap; overflow-wrap: anywhere; font-size: 12px; }
  .model-hint { flex-shrink: 0; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0 16px 8px; padding: 6px 10px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text-muted); font-size: 12px; overflow-wrap: anywhere; }
  .model-hint-actions { display: inline-flex; gap: 6px; }
  .model-hint button { padding: 3px 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface-raised); color: var(--text); font: inherit; font-size: 11px; cursor: pointer; }
  .model-hint button:hover { background: var(--surface-hover); }
  .restore-draft, .more-history { flex-shrink: 0; padding: 6px 10px; margin: 4px 16px 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text); }
  .turn-nav { position: relative; display: flex; align-items: center; min-width: 0; flex: 0 1 auto; }
  header .turn-trigger { display: flex; align-items: center; gap: 5px; width: auto; height: 24px; max-width: 240px; padding: 0 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-alt); color: var(--text-muted); font-size: 11px; }
  header .turn-trigger:hover, header .turn-trigger[aria-expanded="true"] { background: var(--surface-hover); border-color: var(--border-strong); color: var(--text); }
  .turn-label { min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .turn-panel { position: absolute; top: calc(100% + 6px); right: 0; z-index: 14; width: 260px; max-height: 320px; overflow: auto; padding: 6px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface-raised); box-shadow: 0 10px 26px #0006; }
  .turn-panel-head { margin: 0; padding: 4px 8px 6px; color: var(--text-muted); font-size: 10px; }
  .turn-panel ul { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  header .turn-panel button { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 8px; width: 100%; height: auto; padding: 6px 8px; border: 1px solid transparent; border-radius: 6px; background: transparent; color: var(--text); font-size: 11px; text-align: left; }
  header .turn-panel button:hover { background: var(--surface-hover); }
  header .turn-panel button.active { border-color: var(--accent); background: var(--surface-alt); }
  .turn-no { color: var(--accent); font-family: var(--code-font); }
  .turn-summary { min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; color: var(--text-muted); }
  header .turn-panel button.active .turn-summary { color: var(--text-strong); }
  .turn-time { color: var(--text-muted); font-size: 10px; white-space: nowrap; }
  .turn-rail { position: absolute; top: 50%; right: 6px; transform: translateY(-50%); z-index: 3; display: flex; flex-direction: column; align-items: center; gap: 6px; max-height: calc(100% - 24px); overflow-y: auto; padding: 4px 2px; }
  .turn-dot { width: 8px; height: 8px; flex-shrink: 0; padding: 0; border: 0; border-radius: 999px; background: var(--border-strong); opacity: .75; transition: background .15s ease, height .15s ease, opacity .15s ease; }
  .turn-dot:hover { background: var(--accent); opacity: 1; }
  .turn-dot.active { height: 18px; background: var(--accent); opacity: 1; }
  article.jump-flash { outline-color: var(--accent); }
  @media (max-width: 620px) { .transcript { padding: 12px; } .composer { margin: 0 8px 8px; } }
</style>
