<script lang="ts">
  import { Channel, invoke } from "@tauri-apps/api/core";
  import { ArrowDown, ArrowDownUp, ArrowUp, Bot, ChevronDown, CircleAlert, History, Pencil, RefreshCw, Shrink, Sparkles, Square, Terminal, Undo2, UserRound, X } from "@lucide/svelte";
  import { onMount, tick, untrack, type Snippet } from "svelte";
  import type { DialogRequest, DialogValue } from "./dialog";
  import { applyRpcEvent, canSubmitPrompt, contentText, emptyConversation, loadHistory, messageRenderable, readableRpcError, record, type RpcEvent, type RpcMessage, type RpcTool } from "./rpc-state";
  import { onModelsChanged } from "./model-config-sync";
  import { findSavedModel, parseModelKey, shouldApplySavedChoice, validSavedThinkingLevel, type SavedModelChoice } from "./model-memory";
import type { ChatDetailLevel } from "./settings";
  import { captureTranscriptAnchor, restoreTranscriptAnchor, messageWindowStart, transcriptPort } from "./transcript-scroll";
  import MessageDisclosure from "./MessageDisclosure.svelte";
  import { messageSource } from "./message-parts";
  import { formatDuration } from "./duration";
  import { createDormantHistorySession, createRpcHistorySession, prependRpcHistory } from "./rpc-history";
  import { deriveSessionTitle, isTruncatedTitleUpgrade } from "./session-title";
  import { BUILTIN_SLASH_COMMANDS, filterSlashCommands, parseSlashCommand, slashSourceLabel, type PiCommand, type SlashCommand } from "./slash-commands";
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

  interface Props {
    taskId: string;
    runId: string | null | undefined;
    title: string;
    tabs?: Snippet;
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
    onReloadSession?: () => void;
    /** AI 对话内容显示详细程度（简洁/标准/详细），来自全局设置。 */
    chatDetailLevel?: ChatDetailLevel;
  }
  let { taskId, runId, title, tabs, visible, active, switching, focusToken, autoName, onUseTerminal, onDialog, onCancelDialogs, onActivity, onAutoRename, onOpenModelSettings, onReloadSession, chatDetailLevel = "standard" }: Props = $props();
  let conversation = $state(emptyConversation());
  let draft = $state("");
  let restoredDraft = $state("");
  let initializing = $state(true);
  let connected = $state(false);
  let sending = $state(false);
  let stopping = $state(false);
  let error = $state("");
  /// 扩展通过 ui.notify(…, "error") 上报的错误：展示在会话流内（与失败回合同处），
  /// 不再飘在输入框上方的提示条里。
  let extensionError = $state("");
  let notice = $state("");
  /// 输入框 `/` 命令建议：连接后从 pi 拉取扩展/模板/技能命令，与内置命令合并。
  let piCommands = $state<PiCommand[]>([]);
  let slashIndex = $state(0);
  let slashDismissed = $state(false);
  const slashQuery = $derived.by(() => {
    const match = /^\/([^\s]*)$/.exec(draft);
    return match ? match[1] : null;
  });
  const slashItems = $derived(slashQuery !== null && !slashDismissed
    ? filterSlashCommands(slashQuery, piCommands)
    : []);
  // 建议列表变化时回到第一项；查询变化时重新打开被 Esc 关闭的列表。
  $effect(() => { void slashItems; slashIndex = 0; });
  $effect(() => { void slashQuery; slashDismissed = false; });
  let modelName = $state("");
  let models = $state<{ id: string; provider: string; name: string }[]>([]);
  /// 模型列表读取失败与「确实无模型」分开提示：失败时显示错误与重试/重载入口。
  let modelsLoadFailed = $state(false);
  let modelsLoadError = $state("");
  let modelsStale = $state(false);
  let selectedModel = $state("");
  let changingModel = $state(false);
  let thinkingLevels = $state<string[]>([]);
  let thinkingLevel = $state("");
  let compacting = $state(false);
  let enhancing = $state(false);
  let changingThinking = $state(false);
  let autoNamed = $state(false);
  let sessionStats = $state<SessionStats | null>(null);
  let turnStartedAt = $state<number | null>(null);
  let elapsedNow = $state(Date.now());
  /// 每轮对话的执行耗时（毫秒），key = `${runId ?? ""}#${轮次号}`；轮次结束后冻结为固定值。
  let turnDurations = $state<Record<string, number>>({});
  /// 正在执行的轮次号：同一条 busy 周期内被排队消息开启新一轮时，会先结算上一轮再重新计时。
  let runningTurn = $state<number | null>(null);
  let statsGeneration = 0;

  interface SessionStats {
    tokens: { input: number; output: number; cacheRead: number; cacheWrite: number; total: number } | null;
    cost: number | null;
    contextUsage: { tokens: number | null; contextWindow: number | null; percent: number | null } | null;
  }
  let streamingBehavior = $state<"followUp" | "steer">("followUp");
  /// 工作流（agenticskills.io 打包的「技能 + MCP」）：技能安装后按任务自动激活，选择器只负责把站点的起手提示填进草稿。
  interface InstalledWorkflow {
    slug: string;
    name: string;
    category?: string | null;
    skillCount?: number | null;
    mcpCount?: number | null;
    installedAt?: string | null;
    components?: { kind?: string; slug?: string; name?: string; url?: string }[];
  }
  interface WorkflowDetail {
    slug: string;
    name: string;
    description?: string | null;
    category?: string | null;
    level?: string | null;
    setupTime?: string | null;
    components?: { kind?: string; slug?: string; name?: string; url?: string }[];
    steps?: { name?: string; text?: string }[];
    kickoffPrompt?: string | null;
    sourceUrl?: string | null;
  }
  let workflows = $state<InstalledWorkflow[]>([]);
  let workflowBusy = $state(false);
  /// slug → 详情：重复选择同一个工作流时不再请求后端。
  const workflowDetails = new Map<string, WorkflowDetail>();
  let input = $state<HTMLTextAreaElement>();
  /// 粘贴/拖入的图片附件：随 prompt 命令的 `images` 字段以 base64 发给 Pi（RPC 协议支持 prompt/steer/follow_up 携带图片）。
  interface ChatImageAttachment {
    id: number;
    name: string;
    mimeType: string;
    data: string; /// base64，不含 `data:` 前缀
    size: number; /// 原始字节数
  }
  /// Pi/Provider 对图片类型的支持范围与会话渲染校验保持一致；单个附件与总量都设上限，
  /// 避免超出后端命令大小上限（8MB，含文本与 JSON 开销）。
  const MAX_IMAGE_ATTACHMENTS = 6;
  const MAX_IMAGE_FILE_BYTES = 4 * 1024 * 1024;
  const MAX_IMAGES_BASE64_TOTAL = 6 * 1024 * 1024;
  const SUPPORTED_IMAGE_MIME_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];
  let attachments = $state<ChatImageAttachment[]>([]);
  let attachmentError = $state("");
  let attachmentSeq = 0;
  let draggingFiles = $state(false);
  const base64LengthOf = (bytes: number): number => Math.ceil(bytes / 3) * 4;
  const attachmentsBase64Total = $derived(attachments.reduce((total, file) => total + file.data.length, 0));

  function removeAttachment(id: number) {
    attachments = attachments.filter((file) => file.id !== id);
    attachmentError = "";
  }

  /// 已开始读取、尚未完成的图片数量与预估 base64 字节数（响应式：读取期间禁用发送）。
  let pendingImageReads = $state(0);
  let pendingImageReadBytes = $state(0);

  /// 把粘贴/拖入的图片文件加入附件列表；读取完成后异步追加。
  function addImageFiles(incoming: Iterable<File>) {
    attachmentError = "";
    for (const file of incoming) {
      if (!file.type.startsWith("image/")) continue;
      if (attachments.length + pendingImageReads >= MAX_IMAGE_ATTACHMENTS) {
        attachmentError = t("最多附加 {count} 张图片", { count: MAX_IMAGE_ATTACHMENTS });
        return;
      }
      if (!SUPPORTED_IMAGE_MIME_TYPES.includes(file.type)) {
        attachmentError = t("图片格式不支持：仅支持 PNG、JPEG、GIF、WebP");
        continue;
      }
      if (file.size > MAX_IMAGE_FILE_BYTES) {
        attachmentError = t("图片「{name}」超过 {max} 的大小限制", { name: file.name, max: formatBytes(MAX_IMAGE_FILE_BYTES) });
        continue;
      }
      if (attachmentsBase64Total + pendingImageReadBytes + base64LengthOf(file.size) > MAX_IMAGES_BASE64_TOTAL) {
        attachmentError = t("图片总大小超出限制，请先移除部分图片");
        return;
      }
      const reader = new FileReader();
      /// 同步预算：读取期间先把 pending 计入，完成后归还，避免连贴/连拖超额。
      pendingImageReads += 1;
      pendingImageReadBytes += base64LengthOf(file.size);
      const settle = () => {
        pendingImageReads -= 1;
        pendingImageReadBytes -= base64LengthOf(file.size);
      };
      reader.onerror = () => {
        settle();
        attachmentError = t("图片读取失败：{name}", { name: file.name });
      };
      reader.onload = () => {
        settle();
        if (typeof reader.result !== "string") return;
        const comma = reader.result.indexOf(",");
        const header = reader.result.slice(0, comma >= 0 ? comma : 0).toLowerCase();
        if (!reader.result.startsWith("data:") || comma < 0 || !header.includes("base64")) return;
        const data = reader.result.slice(comma + 1);
        if (!data) return;
        attachments = [...attachments, { id: ++attachmentSeq, name: file.name, mimeType: file.type, data, size: file.size }];
      };
      reader.readAsDataURL(file);
    }
  }

  function onComposerPaste(event: ClipboardEvent) {
    const files = event.clipboardData?.files;
    if (!files?.length) return;
    const images = Array.from(files).filter((file) => file.type.startsWith("image/"));
    if (!images.length) return;
    event.preventDefault();
    addImageFiles(images);
  }

  function hasDraggedFiles(event: DragEvent): boolean {
    return Array.from(event.dataTransfer?.types ?? []).includes("Files");
  }

  function onComposerDragOver(event: DragEvent) {
    if (!hasDraggedFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    draggingFiles = true;
  }

  function onComposerDragLeave(event: DragEvent) {
    if (event.currentTarget === event.target) draggingFiles = false;
  }

  function onComposerDrop(event: DragEvent) {
    draggingFiles = false;
    const files = event.dataTransfer?.files;
    if (!files?.length) return;
    event.preventDefault();
    addImageFiles(files);
  }

  function formatBytes(size: number): string {
    return size >= 1024 * 1024 ? `${(size / (1024 * 1024)).toFixed(1)} MB` : `${Math.max(1, Math.round(size / 1024))} KB`;
  }

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
  /// 休眠会话：应用重启后直接点开的未启动 RPC 会话——不连进程、不占额度，
  /// 输入提示词回车后才启动 pi；启动成功连接后自动发出草稿。
  let dormant = $state(false);
  let autoSendOnConnect = false;
  let generation = 0;
  let composing = false;
  /// 用户点过「重载会话」后，下一次模型列表加载成功时提示一次（非响应式标记）。
  let reloadRequested = false;
  const messageStart = $derived(messageWindowStart(conversation.messages.length, historyLimit, pinnedMessageStart));
  const visibleMessages = $derived(conversation.messages.slice(messageStart));
  /// 空白 assistant 占位轮次（失败请求在会话历史里的残留）不渲染：错误已由横幅展示，
  /// 空轮次只剩「Pi」标签和一对复制按钮；带上原始下标，data-message-index 保持正确。
  const visibleEntries = $derived(
    visibleMessages
      .map((message, offset) => ({ message, index: messageStart + offset }))
      .filter((entry) => messageRenderable(entry.message)),
  );
  const toolResults = $derived(new Set(conversation.messages.map((message) => message.toolCallId).filter(Boolean)));
  const liveTools = $derived(Object.values(conversation.tools).filter((tool) => !toolResults.has(tool.id)));
  /// 正在执行的工具：工具卡与状态栏共用；通常只有一个，多个时展示最新的一个。
  const runningTools = $derived(liveTools.filter((tool) => tool.running));
  const finishedLiveTools = $derived(liveTools.filter((tool) => !tool.running));
  const activeTool = $derived(runningTools.length ? runningTools[runningTools.length - 1] : null);

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

  function turnDurationKey(turnNumber: number): string {
    return `${runId ?? ""}#${turnNumber}`;
  }

  /// pi TUI 风格的工具标题：工具名 + 首个有意义参数摘要（command/file_path/pattern 等），单行截断。
  const TOOL_SUMMARY_KEYS = ["command", "file_path", "path", "pattern", "query", "url", "skill", "name", "id"] as const;

  function toolCallSummary(name: string, args: unknown): string {
    const argsRecord = record(args);
    for (const key of TOOL_SUMMARY_KEYS) {
      const value = argsRecord[key];
      if (typeof value === "string" && value.trim()) {
        const line = value.split("\n", 1)[0].trim();
        if (line) return `${name} ${line.length > 80 ? `${line.slice(0, 80)}…` : line}`;
      }
    }
    return name;
  }

  /// —— 输出活跃度分档 ——
  /// tool_execution_update 事件即心跳：< 10s 视为输出更新中；< 2min 静默；再久提示疑似停滞。
  /// 注意措辞：长静默 ≠ 卡住（npm install、sleep 等本来就长时间无输出），只客观陈述无输出多久。
  const FRESH_OUTPUT_MS = 10_000;
  const STALE_OUTPUT_MS = 120_000;
  /// 思考/生成阶段：流式增量超过 30s 未到才提示无响应，避免正常停顿误报。
  const SILENT_MODEL_MS = 30_000;
  /// 工具总耗时超过 5 分钟标记「长时间运行」徽标。
  const LONG_TOOL_MS = 300_000;
  /// 实时输出预览保留的尾部行数。
  const LIVE_OUTPUT_LINES = 10;

  type OutputActivity = "fresh" | "quiet" | "stale";

  /// 距上次输出事件的静默时长；从未更新过（如等待首个输出的 bash / 不产出的工具）按开始时间算。
  function toolSilenceMs(tool: RpcTool, now: number): number {
    return Math.max(0, now - (tool.lastUpdateAt ?? tool.startedAt ?? now));
  }

  function outputActivity(tool: RpcTool, now: number): OutputActivity {
    const silence = toolSilenceMs(tool, now);
    return silence < FRESH_OUTPUT_MS ? "fresh" : silence < STALE_OUTPUT_MS ? "quiet" : "stale";
  }

  function silenceLabel(tool: RpcTool, now: number): string {
    const silence = toolSilenceMs(tool, now);
    return silence < FRESH_OUTPUT_MS ? "" : ` · ${t("无输出 {duration}", { duration: formatDuration(silence) })}`;
  }

  /// 累计输出取尾部若干行做实时预览（partialResult 是累计值，直接替换展示即可）；无输出返回空串。
  function liveOutputTail(tool: RpcTool): string {
    const text = contentText(typeof tool.result === "string" ? tool.result : record(tool.result).content);
    if (!text) return "";
    const lines = text.replace(/\s+$/, "").split("\n");
    return lines.slice(-LIVE_OUTPUT_LINES).join("\n");
  }

  function liveOutputLastLine(tool: RpcTool): string {
    const tail = liveOutputTail(tool);
    return tail ? tail.split("\n").pop() ?? "" : "";
  }

  /// 把实时输出滚动钉在底部（tail -f 效果）；依赖的文本变化时触发，rAF 等 DOM 更新后再定位。
  function pinOutputBottom(node: HTMLPreElement, _text: string) {
    const scrollToEnd = () => { requestAnimationFrame(() => { node.scrollTop = node.scrollHeight; }); };
    scrollToEnd();
    return { update: scrollToEnd };
  }

  /// 上滚时悬浮胶囊的文案：工具摘要 + 已运行 + 静默时长。
  function activeToolChipLabel(tool: RpcTool): string {
    const elapsed = tool.startedAt !== undefined ? formatDuration(elapsedNow - tool.startedAt) : "";
    return `${t("{name} 执行中", { name: toolCallSummary(tool.name, tool.args) })}${elapsed ? ` · ${elapsed}` : ""}${silenceLabel(tool, elapsedNow)}`;
  }

  /// 回到最新消息并恢复自动跟随；悬浮胶囊与回底按钮共用。
  function returnToLatest() {
    followScroll = true;
    pinnedMessageStart = null;
    if (transcript) transcript.scrollTop = transcript.scrollHeight;
  }

  /// 把正在计时的轮次耗时累加进 turnDurations（排队续跑时同一轮可能多次累加）。
  function flushTurnDuration(turnNumber: number, startedAt: number | null) {
    if (turnNumber < 1 || startedAt === null) return;
    const key = turnDurationKey(turnNumber);
    turnDurations = { ...turnDurations, [key]: (turnDurations[key] ?? 0) + Math.max(0, Date.now() - startedAt) };
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
  /// 消息下标 → 所属轮次号（用户消息即每轮起点，其后所有消息都归属该轮）。
  const turnNumberByMessageIndex = $derived.by(() => {
    const map = new Map<number, number>();
    let current = 0;
    conversation.messages.forEach((message, index) => {
      if (message.role === "user") current += 1;
      if (current > 0) map.set(index, current);
    });
    return map;
  });
  /// 每轮的执行时间文案：进行中的轮次用实时计时器，已完成的用冻结的记录值（pi formatDuration 格式）。
  const turnDurationLabels = $derived.by(() => {
    const labels = new Map<number, string>();
    for (const turn of turns) {
      if (turn.number === runningTurn && turnStartedAt !== null) {
        labels.set(turn.number, formatDuration(elapsedNow - turnStartedAt));
      } else {
        const recorded = turnDurations[turnDurationKey(turn.number)];
        if (recorded !== undefined) labels.set(turn.number, formatDuration(recorded));
      }
    }
    return labels;
  });

  /// 取某条消息所在轮次的执行时间文案；非用户消息或尚未计时返回空串。
  function turnDurationFor(messageIndex: number): string {
    const number = turnNumberByMessageIndex.get(messageIndex);
    if (number === undefined) return "";
    return turnDurationLabels.get(number) ?? "";
  }

  /// 轮末判定：下一条消息不存在或是新用户消息，则当前条目是该轮末尾。
  function isTurnBoundary(offset: number): boolean {
    const next = visibleEntries[offset + 1];
    return !next || next.message.role === "user";
  }

  /// 轮末总执行时间：结束后显示 Took（耗时）；进行中的当前轮由底部实时状态行
  /// 显示行为状态与「本轮」耗时，不再重复显示「已用时」；无计时的历史轮返回空串。
  function turnTotalLabel(messageIndex: number): string {
    const turnNumber = turnNumberByMessageIndex.get(messageIndex);
    if (turnNumber === undefined) return "";
    const recorded = turnDurations[turnDurationKey(turnNumber)];
    return recorded === undefined ? "" : t("耗时 {duration}", { duration: formatDuration(recorded) });
  }

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
    const timer = window.setInterval(() => { elapsedNow = Date.now(); }, 250);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("pointerdown", closeTurnMenu, true);
      window.removeEventListener("keydown", handleWindowKeydown);
      if (flashTimer) { clearTimeout(flashTimer); flashTimer = null; }
    };
  });

  /// 每轮执行时间归属：busy 周期与轮次对齐；同一 busy 周期内出现新用户消息（排队/插话）
  /// 时先结算上一轮耗时再重新计时，busy 结束把最后一轮耗时冻结进 turnDurations。
  $effect(() => {
    const latestTurn = turns.length;
    if (conversation.busy) {
      if (runningTurn === null) {
        runningTurn = latestTurn;
        turnStartedAt = Date.now();
      } else if (latestTurn > runningTurn) {
        flushTurnDuration(runningTurn, turnStartedAt);
        runningTurn = latestTurn;
        turnStartedAt = Date.now();
      }
    } else if (runningTurn !== null) {
      flushTurnDuration(runningTurn, turnStartedAt);
      runningTurn = null;
      turnStartedAt = null;
    }
  });
  /// 当前任务的项目路径：`list_tasks` 的记录里带 projectPath，用来顺带读取项目范围的工作流。
  /// 任务没有项目上下文（或查询失败）时返回 null，此时只读全局配置。
  async function taskProjectPath(): Promise<string | null> {
    const tasks = await invoke<{ id?: string; projectPath?: string }[]>("list_tasks");
    const path = tasks.find((task) => task.id === taskId)?.projectPath;
    return typeof path === "string" && path.trim() ? path : null;
  }

  /// 已安装工作流：全局 + 当前项目各读一次，按 slug 去重（项目覆盖全局）。
  /// 读取失败或没有安装任何工作流时保持空列表 → 选择器不渲染（不留空下拉）；失败只记录日志，不打断对话。
  $effect(() => {
    void taskId;                       // 切换任务时重新读取
    void runId;                        // 会话重启后项目上下文可能变化
    let alive = true;
    void (async () => {
      try {
        const globalList = await invoke<InstalledWorkflow[]>("list_installed_workflows", { scope: "global", projectPath: null });
        if (!alive) return;
        const projectPath = await taskProjectPath().catch(() => null);
        if (!alive) return;
        const projectList = projectPath
          ? await invoke<InstalledWorkflow[]>("list_installed_workflows", { scope: "project", projectPath }).catch((cause) => {
              console.warn("list_installed_workflows (project) failed:", cause);
              return [] as InstalledWorkflow[];
            })
          : [];
        if (!alive) return;
        const merged = new Map<string, InstalledWorkflow>();
        for (const item of [...(Array.isArray(globalList) ? globalList : []), ...(Array.isArray(projectList) ? projectList : [])]) {
          if (!item || typeof item.slug !== "string" || !item.slug.trim()) continue;
          merged.set(item.slug, item);
        }
        workflows = [...merged.values()];
      } catch (cause) {
        if (!alive) return;
        workflows = [];
        console.warn("list_installed_workflows failed:", cause);
      }
    })();
    return () => { alive = false; };
  });

  /// 选项副文本：只显示存在的技能 / MCP 数量。
  function workflowMeta(workflow: InstalledWorkflow): string {
    const parts: string[] = [];
    const skills = typeof workflow.skillCount === "number" ? workflow.skillCount : 0;
    const mcps = typeof workflow.mcpCount === "number" ? workflow.mcpCount : 0;
    if (skills > 0) parts.push(t("{count} 个技能", { count: skills }));
    if (mcps > 0) parts.push(`${mcps} MCP`);
    return parts.length ? ` · ${parts.join(" / ")}` : "";
  }

  /// 站点没给起手提示时的中性兜底：用步骤名拼一句话，最多 6 个步骤名，超出加省略号。
  function workflowStepsPrompt(detail: WorkflowDetail): string {
    const steps = (detail.steps ?? [])
      .map((step) => (typeof step?.name === "string" ? step.name.trim() : ""))
      .filter(Boolean);
    if (!steps.length) return t("按「{name}」工作流执行。", { name: detail.name });
    const shown = steps.slice(0, 6);
    const more = steps.length > shown.length ? "…" : "";
    return t("按「{name}」工作流执行：{steps}", { name: detail.name, steps: `${shown.join(" → ")}${more}` });
  }

  /// 选中工作流：拉取（或复用缓存的）详情，把起手提示填进草稿——只填草稿，不自动发送。
  /// 草稿为空时直接填入；草稿非空时不覆盖用户已写内容，而是换行接在后面。
  async function applyWorkflow(slug: string) {
    const workflow = workflows.find((item) => item.slug === slug);
    if (!workflow) return;
    const current = generation;
    workflowBusy = true;
    try {
      let detail = workflowDetails.get(slug);
      if (!detail) {
        detail = await invoke<WorkflowDetail>("agentic_workflow_detail", { request: { slug } });
        workflowDetails.set(slug, detail);
      }
      if (current !== generation) return;
      const prompt = typeof detail.kickoffPrompt === "string" && detail.kickoffPrompt.trim()
        ? detail.kickoffPrompt.trim()
        : workflowStepsPrompt(detail);
      draft = draft.trim() ? `${draft.trimEnd()}\n\n${prompt}` : prompt;
      notice = t("已把「{name}」的起手提示填入输入框，可编辑后发送", { name: workflow.name });
      if (visible) {
        await tick();
        input?.focus();
        input?.setSelectionRange(draft.length, draft.length);
      }
    } catch (cause) {
      if (current === generation) notice = tm(String(cause));
    } finally {
      if (current === generation) workflowBusy = false;
    }
  }
  const canSend = $derived(!switching && (connected || dormant) && !initializing && !stopping && !conversation.closed
    && canSubmitPrompt(sending, draft, attachments.length, pendingImageReads));
  /// 发送按钮提示：执行中说明本条消息将按所选方式引导/排队，而非开启新回复。
  const sendHint = $derived(
    dormant ? t("启动会话并发送")
      : conversation.busy
      ? streamingBehavior === "steer" ? t("执行中：本条将在当前工具调用后优先引导") : t("执行中：本条将排队，本轮结束后执行")
      : !draft.trim() && attachments.length ? t("发送图片") : t("发送消息"),
  );
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
  /// pi 原生 compact 会先中止当前回合再压缩，忙碌时也允许点击。
  const canCompact = $derived(connected && !switching && !conversation.closed && !compacting);
  /// 只有“关闭”一档时说明该模型没有可调推理强度，此时不显示推理强度选择器。
  const reasoningLevels = $derived(thinkingLevels.filter((level) => level !== "off"));
  const phaseText = $derived(
    conversation.phase === "thinking" ? t("思考中")
      : conversation.phase === "generating" ? t("生成回答")
        : conversation.phase === "tool" ? t("正在执行工具")
          : conversation.phase === "failed" ? t("执行失败")
            : conversation.phase === "waiting" ? t("等待输入")
              : "",
  );
  /// 状态栏：具体到正在执行的工具（名称 + 参数摘要）而不是笼统的「正在执行工具」。
  /// 工具耗时与本轮耗时分开展示，消除「960s 到底是谁的耗时」的歧义；静默超阈适时提示。
  const statusText = $derived.by(() => {
    if (compacting) return t("正在压缩上下文");
    if (switching) return t("正在切换模式");
    if (dormant) return t("未启动");
    if (conversation.closed) return t("已停止");
    if (initializing) return t("连接中");
    if (!connected) return t("未连接");
    const turnPart = turnStartedAt !== null ? ` · ${t("本轮 {duration}", { duration: formatDuration(elapsedNow - turnStartedAt) })}` : "";
    if (conversation.phase === "tool" && activeTool) {
      const summary = toolCallSummary(activeTool.name, activeTool.args);
      const toolPart = activeTool.startedAt !== undefined ? formatDuration(elapsedNow - activeTool.startedAt) : "";
      return `${summary}${toolPart ? ` · ${toolPart}` : ""}${silenceLabel(activeTool, elapsedNow)}${turnPart}`;
    }
    let text = phaseText;
    if (conversation.busy && conversation.lastEventAt !== undefined
      && (conversation.phase === "thinking" || conversation.phase === "generating")) {
      const silentFor = Math.max(0, elapsedNow - conversation.lastEventAt);
      if (silentFor >= SILENT_MODEL_MS) text += ` · ${t("已 {duration} 无响应", { duration: formatDuration(silentFor) })}`;
    }
    return `${text}${turnPart}`;
  });
  /// 对话底部的实时状态行：AI 行为状态 + 本轮耗时，原输入框操作行状态栏迁入此处；
  /// 仅在 AI 实际忙硌（生成/执行工具/压缩）时显示，空闲与停止时不占位。
  const liveStatusVisible = $derived(
    connected && !initializing && !dormant && !conversation.closed && (conversation.busy || compacting),
  );
  /// 工具长时间无输出：状态栏文字转琥珀色 + 停止按钮高亮，提示用户可中断。
  const toolStalled = $derived(activeTool !== null && outputActivity(activeTool, elapsedNow) === "stale");
  const canEnhance = $derived(connected && !switching && !conversation.busy && !conversation.closed && !!draft.trim() && !enhancing);

  /// 待发送提示词（输入框上方）：引导/排队分开列出，支持编辑、撤回与切换投递方式。
  interface PendingEntry { mode: "steer" | "followUp"; index: number; text: string; key: string; }
  const pendingEntries = $derived([
    ...conversation.queueSteering.map((text, index) => ({ mode: "steer" as const, index, text, key: `steer-${index}` })),
    ...conversation.queueFollowUp.map((text, index) => ({ mode: "followUp" as const, index, text, key: `follow-up-${index}` })),
  ]);

  /// 取回全部待发送文本（clear_queue），重建除目标之外的队列；返回被取下的文本。
  /// 重建用 steer/follow_up 命令只带文本：pi 的 clear_queue 不返回图片附件，重建后图片会丢失。
  async function takePending(entry: PendingEntry, retarget: "steer" | "followUp" | null): Promise<string | null> {
    const current = generation;
    try {
      const queue = await call<{ steering?: string[]; followUp?: string[] }>({ type: "clear_queue" });
      if (current !== generation) return null;
      const steering = [...queue.steering ?? []];
      const followUp = [...queue.followUp ?? []];
      const source = entry.mode === "steer" ? steering : followUp;
      const text = source[entry.index] ?? "";
      if (!text) return null;
      source.splice(entry.index, 1);
      if (retarget) (retarget === "steer" ? steering : followUp).push(text);
      for (const item of steering) await call({ type: "steer", message: item });
      for (const item of followUp) await call({ type: "follow_up", message: item });
      return text;
    } catch (cause) {
      if (current === generation) commandError(cause);
      return null;
    }
  }

  async function withdrawPending(entry: PendingEntry) {
    await takePending(entry, null);
  }

  async function movePending(entry: PendingEntry) {
    await takePending(entry, entry.mode === "steer" ? "followUp" : "steer");
  }

  /// 编辑：把待发送文本取回输入框；已有草稿先暂存（可用「恢复暂存文本」找回）。
  async function editPending(entry: PendingEntry) {
    const text = await takePending(entry, null);
    if (!text || text === draft) return;
    if (draft) restoredDraft = draft;
    draft = text;
    await tick();
    input?.focus();
  }

  /// 压缩上下文：调用 Pi 原生 `compact` RPC，把历史消息总结为摘要。
  async function compactContext() {
    if (!canCompact) return;
    const current = generation;
    const scope = `rpc:${taskId}:${runId ?? ""}`;
    try {
      const accepted = await onDialog({
        scope, kind: "confirm", title: t("压缩上下文"),
        message: conversation.busy
          ? t("压缩会先中止当前正在进行的回合，再把历史消息总结为摘要以释放上下文窗口。继续吗？")
          : t("压缩会把历史消息总结为摘要以释放上下文窗口。继续吗？"),
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
    // 会话重建（切换任务/重连/重启）时清空每轮计时，避免旧会话的耗时挂到新一轮。
    turnDurations = {};
    runningTurn = null;
    turnStartedAt = null;
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
    modelsLoadFailed = false;
    modelsLoadError = "";
    modelsStale = false;
    piCommands = [];
    modelName = "";
    selectedModel = "";
    conversation = emptyConversation();
    historyOffset = 0;
    error = "";
    extensionError = "";
    if (!run) {
      // 未启动的休眠会话：不连进程、不占额度，直接读 pi 会话文件展示历史；
      // 输入框保持可用，回车发送时才启动。文件不可读（从未启动或已删除）时保持空白。
      initializing = false;
      dormant = true;
      const reader = createDormantHistorySession(invoke, id);
      historySession = reader;
      void (async () => {
        try {
          const history = await reader.open();
          if (!alive || current !== generation || !history) return;
          conversation = loadHistory(emptyConversation(), history.messages, 0);
          historyOffset = history.start;
          // 恢复历史会话的模型与推理强度显示：进程未启动时选择器只读，
          // 回车启动 pi 后由 get_state / 模型列表覆盖为权威值。
          if (history.model) {
            models = [{ provider: history.model.provider, id: history.model.id, name: history.model.id }];
            selectedModel = `${history.model.provider}/${history.model.id}`;
            modelName = history.model.id;
          }
          if (history.thinkingLevel && history.thinkingLevel !== "off") {
            thinkingLevel = history.thinkingLevel;
            thinkingLevels = [history.thinkingLevel];
          }
        } catch {
          // 保持空白会话：输入提示词并回车仍可启动。
        }
      })();
      return () => {
        alive = false;
        void reader.dispose();
        if (current === generation) generation++;
        untrack(() => { if (historySession === reader) historySession = null; });
      };
    }
    dormant = false;
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
        if (event.payload.type === "agent_start") extensionError = "";
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
        next.compacting = state.isCompacting === true;
        next.phase = next.busy ? "generating" : "waiting";
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
        // pi 的扩展命令、提示词模板与技能命令：输入 `/` 时与内置命令一起建议，
        // 发送后由 pi 的 prompt 管道展开执行。
        void call<Record<string, unknown>>({ type: "get_commands" }).then((result) => {
          if (!alive) return;
          const list = Array.isArray(result.commands) ? result.commands : [];
          piCommands = list.flatMap((item) => {
            const value = record(item);
            const name = typeof value.name === "string" ? value.name.trim() : "";
            if (!name) return [];
            return [{
              name,
              description: typeof value.description === "string" ? value.description : "",
              source: typeof value.source === "string" ? value.source : "extension",
            }];
          });
        }).catch(() => {});
        void applySavedModelChoice(history.messages.length, next.busy, () => alive);
        // 回车启动的休眠会话：连接就绪（含历史加载与模型恢复）后自动发出草稿。
        if (autoSendOnConnect && alive) { autoSendOnConnect = false; void send(); }
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

  /// 错误卡片渲染在会话流末尾：出现时若正跟随滚动，则一并滚到底部让错误可见。
  $effect(() => {
    if (!(error || conversation.error || extensionError) || !followScroll) return;
    void tick().then(() => {
      if (transcript && followScroll) transcript.scrollTop = transcript.scrollHeight;
    });
  });

  function call<T>(command: Record<string, unknown>): Promise<T> {
    return invoke<T>("rpc_command", { taskId, runId, command });
  }

  function commandError(cause: unknown) {
    // 先把 `429: {json}` 之类的原始错误收敛成可读文本，再过一遍消息码渲染。
    error = tm(readableRpcError(String(cause)));
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

  /// 上下文用量轮询：pi 的估算 = 最近一次真实 usage + 其后消息的估算，轮询即可在回合进行中实时反映增长。
  $effect(() => {
    if (!connected || conversation.closed) return;
    refreshStats();
    const timer = window.setInterval(refreshStats, 2500);
    return () => window.clearInterval(timer);
  });

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

  /// 百分比最多保留两位小数并去掉尾随零（避免 5.490600000000001% 这类浮点残留）。
  function formatPercent(value: number): string {
    return `${Number(value.toFixed(2))}%`;
  }

  /// 压缩卡片徽标：live 事件带前后 tokens 显示「压缩前 → 压缩后 · 释放」，历史消息只有压缩前。
  function compactionStatLabel(message: RpcMessage): string {
    const before = message.tokensBefore;
    const after = message.tokensAfter;
    if (typeof before === "number" && before > 0 && typeof after === "number" && after > 0) {
      const freed = before - after;
      if (freed > 0) {
        return t("压缩前 {before} → 压缩后 {after} · 释放 {freed}", { before: formatTokens(before), after: formatTokens(after), freed: formatTokens(freed) });
      }
      return t("压缩前 {before} → 压缩后 {after}", { before: formatTokens(before), after: formatTokens(after) });
    }
    if (typeof before === "number" && before > 0) return t("压缩前 {tokens}", { tokens: formatTokens(before) });
    return "";
  }

  function formatCost(value: number | null): string {
    if (value === null) return "-";
    if (value >= 1) return `$${value.toFixed(2)}`;
    if (value > 0) return `$${value.toFixed(4)}`;
    return "$0";
  }

  $effect(() => {
    if (autoNamed || conversation.busy) return;
    const messages = conversation.messages;
    if (!messages.some((message) => message.role === "user")) return;
    if (!messages.some((message) => message.role !== "user" && message.role !== "toolResult")) return;
    const next = deriveSessionTitle(messages);
    if (!next) return;
    // 默认标题匹配自动命名；另外允许把旧版 24 字截断的自动标题升级为完整标题。
    if (!autoName && !isTruncatedTitleUpgrade(title, next)) return;
    autoNamed = true;
    // 休眠态没有进程可同步名称；pi 启动时会经 --name 带上新标题。
    if (connected) void call({ type: "set_session_name", name: next }).catch(() => {});
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
        // 记住手动选择：档位按新模型可用列表调整后一并落库，随项目沿用。
        persistModelChoice(model.provider, model.id);
      }).catch(() => {
        // 档位列表读取失败也记住模型（档位维持会话当前值）。
        persistModelChoice(model.provider, model.id);
      });
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) changingModel = false;
    }
  }

  /// 空状态下重载会话：交给父组件重启 RPC 任务（保留会话与历史），新会话初始化时会重新读取模型列表。
  function reloadSession() {
    if (!onReloadSession) return;
    reloadRequested = true;
    onReloadSession();
  }

  /// 设置页保存了 Provider/模型/凭据后的处理：空闲时自动重载（Pi 一次性读取
  /// models.json，没有 reload 命令，重载是拿到新配置的唯一途径）；忙时提示用户手动重载。
  function handleModelsChanged() {
    if (reloadRequested) return; // 已经在重载中，新会话自然带上最新配置
    const idle = connected && !initializing && !conversation.busy && !conversation.closed &&
      !sending && !stopping && conversation.queue.length === 0;
    if (idle) {
      reloadSession();
      return;
    }
    modelsStale = true;
  }

  $effect(() => onModelsChanged(() => { void handleModelsChanged(); }));

  /// 读取当前会话可用的模型列表；列表为空时由空状态提示兜底。返回解析后的列表
  /// （失败时返回读取前的值，通常为空数组），供新会话应用上次模型选择时使用。
  async function refreshModels(alive?: () => boolean): Promise<typeof models> {
    const current = generation;
    try {
      const result = await call<{ models: unknown[] }>({ type: "get_available_models" });
      if (alive && !alive()) return models;
      if (current !== generation) return models;
      models = result.models
        .map(record)
        .filter((model) => typeof model.id === "string" && typeof model.provider === "string")
        .map((model) => ({ id: String(model.id), provider: String(model.provider), name: String(model.name ?? model.id) }));
      modelsLoadFailed = false;
      modelsLoadError = "";
      if (models.length && reloadRequested) { reloadRequested = false; notice = t("已重载，请检查上方模型选择器"); }
      return models;
    } catch (cause) {
      // 读取失败不再完全静默：打上标记，空状态据此显示错误，与「确实无模型」的提示区分开。
      if (alive && !alive()) return models;
      if (current !== generation) return models;
      modelsLoadFailed = true;
      modelsLoadError = tm(String(cause));
      return models;
    }
  }

  /// 读取该项目上次手动选择的模型/推理强度；没有记录或读取失败（如旧版本数据库）时按无记录处理。
  async function readSavedModelChoice(): Promise<SavedModelChoice | null> {
    try {
      const data = record(await invoke<unknown>("get_last_model_choice", { taskId }));
      const provider = typeof data.provider === "string" ? data.provider : "";
      const modelId = typeof data.modelId === "string" ? data.modelId : "";
      if (!provider || !modelId) return null;
      const thinkingLevel = typeof data.thinkingLevel === "string" && data.thinkingLevel ? data.thinkingLevel : null;
      return { provider, modelId, thinkingLevel };
    } catch {
      return null;
    }
  }

  /// 把当前（模型, 推理强度）记到任务数据库，随项目沿用；失败静默，不影响会话本身。
  function persistModelChoice(provider: string, modelId: string): void {
    void invoke("save_last_model_choice", {
      taskId,
      provider,
      modelId,
      thinkingLevel: thinkingLevel || null,
    }).catch(() => {});
  }

  /// 连接完成后应用项目上次手动选择的模型与推理强度：仅对没有任何历史消息且空闲的新
  /// 会话生效；模型已不在可用列表或档位已失效时静默跳过，恢复的会话一律不覆盖。
  async function applySavedModelChoice(historyCount: number, busy: boolean, alive: () => boolean): Promise<void> {
    const current = generation;
    const saved = shouldApplySavedChoice(historyCount, busy) ? await readSavedModelChoice() : null;
    if (!alive() || current !== generation) return;
    if (!saved) {
      // 恢复的会话或该项目从未选择过：保持原初始化，只拉取档位与模型列表供选择器展示。
      void call<{ levels: unknown[] }>({ type: "get_available_thinking_levels" }).then((result) => {
        if (!alive()) return;
        thinkingLevels = result.levels.map((level) => String(level)).filter(Boolean);
      }).catch(() => {});
      void refreshModels(alive);
      return;
    }
    const available = await refreshModels(alive);
    if (!alive() || current !== generation) return;
    const model = findSavedModel(saved, available);
    if (model) {
      try {
        await call({ type: "set_model", provider: model.provider, modelId: model.id });
      } catch {
        return; // 应用失败不提示：新会话保持 Pi 默认行为即可。
      }
      if (!alive() || current !== generation) return;
      selectedModel = `${model.provider}/${model.id}`;
      modelName = model.name;
    }
    const levels = await call<{ levels: unknown[] }>({ type: "get_available_thinking_levels" }).catch(() => null);
    if (!alive() || current !== generation || !levels) return;
    thinkingLevels = levels.levels.map((level) => String(level)).filter(Boolean);
    const level = validSavedThinkingLevel(saved.thinkingLevel, thinkingLevels);
    if (!level) return;
    try {
      await call({ type: "set_thinking_level", level });
    } catch {
      return;
    }
    if (alive() && current === generation) thinkingLevel = level;
  }

  async function changeThinkingLevel(level: string) {
    if (!level || changingThinking) return;
    const current = generation;
    changingThinking = true;
    try {
      await call({ type: "set_thinking_level", level });
      if (current === generation) {
        thinkingLevel = level;
        error = "";
        // 记住手动选择：档位与当前模型一并落库，随项目沿用。
        const key = parseModelKey(selectedModel);
        if (key) persistModelChoice(key.provider, key.modelId);
      }
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) changingThinking = false;
    }
  }

  /// 把 /model 参数解析为 `provider/id`：支持完整键、唯一模型 id、唯一显示名。
  function resolveModelChoice(args: string): string | null {
    const value = args.trim();
    if (!value) return null;
    const exact = models.find((model) => `${model.provider}/${model.id}` === value);
    if (exact) return `${exact.provider}/${exact.id}`;
    const byId = models.filter((model) => model.id === value);
    if (byId.length === 1) return `${byId[0].provider}/${byId[0].id}`;
    const byName = models.filter((model) => model.name === value);
    if (byName.length === 1) return `${byName[0].provider}/${byName[0].id}`;
    return null;
  }

  /// 选中命令建议：填入 `/name ` 保留参数位置，由回车触发执行/发送。
  function acceptSlash(item: SlashCommand) {
    draft = `/${item.name} `;
    slashDismissed = true;
    void tick().then(() => input?.focus());
  }

  function moveSlash(delta: number) {
    if (!slashItems.length) return;
    slashIndex = (slashIndex + delta + slashItems.length) % slashItems.length;
  }

  /// 执行 `/` 命令：内置命令映射到 RPC/界面动作，pi 命令交由 prompt 管道展开。
  /// 返回 "blocked" 时保留草稿（错误提示可读、可直接改）。
  async function runSlashCommand(name: string, args: string): Promise<"handled" | "prompt" | "blocked"> {
    const builtin = BUILTIN_SLASH_COMMANDS.find((command) => command.name === name);
    if (builtin) {
      if (!builtin.available) { notice = t("该命令仅在终端兼容模式可用"); return "blocked"; }
      if (!connected || conversation.closed) { notice = t("请先启动会话后再使用命令"); return "blocked"; }
      const scope = `rpc:${taskId}:${runId}`;
      try {
        switch (name) {
          case "compact": await compactContext(); break;
          case "model": {
            if (args) {
              const value = resolveModelChoice(args);
              if (!value) { notice = t("未找到模型「{name}」；可用 /model 查看列表", { name: args }); return "blocked"; }
              await changeModel(value);
            } else {
              const value = await onDialog({ scope, kind: "choice", title: t("选择模型"),
                message: t("选择本会话使用的模型。"),
                choices: models.map((model) => ({ value: `${model.provider}/${model.id}`, label: `${model.name} · ${model.provider}` })) });
              if (typeof value === "string") await changeModel(value);
            }
            break;
          }
          case "thinking": {
            if (args) {
              if (!thinkingLevels.includes(args)) {
                notice = t("未知推理强度「{level}」；可用值：{levels}", { level: args, levels: thinkingLevels.join(" / ") || t("无") });
                return "blocked";
              }
              await changeThinkingLevel(args);
            } else if (thinkingLevels.length) {
              const value = await onDialog({ scope, kind: "choice", title: t("选择推理强度"),
                message: t("选择本会话使用的推理强度。"),
                choices: thinkingLevels.map((level) => ({ value: level, label: level })) });
              if (typeof value === "string") await changeThinkingLevel(value);
            } else {
              notice = t("当前模型没有可用的推理强度");
              return "blocked";
            }
            break;
          }
          case "name": {
            if (!args) { notice = t("用法：/name <会话名称>"); return "blocked"; }
            await call({ type: "set_session_name", name: args });
            onAutoRename(args);
            notice = t("会话已重命名为「{name}」", { name: args });
            break;
          }
          case "copy": {
            const result = await call<Record<string, unknown>>({ type: "get_last_assistant_text" });
            const text = typeof result.text === "string" ? result.text : "";
            if (!text) { notice = t("没有可复制的回复"); return "blocked"; }
            await navigator.clipboard.writeText(text);
            notice = t("已复制最后一条回复");
            break;
          }
          case "session": {
            const parts = [t("会话 {id}", { id: taskId }), t("{count} 条消息", { count: conversation.messages.length })];
            const tokens = sessionStats?.tokens;
            if (tokens) parts.push(`↑${formatTokens(tokens.input)} ↓${formatTokens(tokens.output)}`);
            if (sessionStats?.cost !== null && sessionStats?.cost !== undefined) parts.push(formatCost(sessionStats.cost));
            notice = parts.join(" · ");
            break;
          }
          case "export": {
            const result = await call<Record<string, unknown>>({ type: "export_html" });
            const path = typeof result.path === "string" ? result.path : "";
            notice = path ? t("已导出会话：{path}", { path }) : t("已导出会话");
            break;
          }
          case "tree": turnMenuOpen = true; break;
          default: notice = t("该命令仅在终端兼容模式可用"); return "blocked";
        }
        draft = "";
        return "handled";
      } catch (cause) {
        commandError(cause);
        return "handled";
      }
    }
    if (piCommands.some((command) => command.name === name)) return "prompt";
    notice = t("未知命令 /{name}；输入 / 查看可用命令", { name });
    return "blocked";
  }

  /// 发送语义：空闲时作为新提示；AI 执行中由后端按 streamingBehavior 处理
  /// （steer=当前工具调用后优先引导，followUp=本轮结束后排队执行），因此执行中也可连续发送。
  /// 粘贴的图片附件以 RPC `images` 字段（ImageContent：base64 + mimeType）随消息一并发送。
  async function send() {
    if (!canSend) return;
    const command = parseSlashCommand(draft);
    if (command) {
      const outcome = await runSlashCommand(command.name, command.args);
      if (outcome !== "prompt") return;
    }
    if (dormant) {
      // 第一条提示词即启动信号：保留草稿，交父组件启动 pi，连接就绪后自动发送。
      // 启动失败时草稿留在输入框、错误提示可见，可直接重试。
      if (!onReloadSession) return;
      autoSendOnConnect = true;
      onReloadSession();
      return;
    }
    const text = draft;
    const images = attachments.map((file) => ({ type: "image", data: file.data, mimeType: file.mimeType }));
    const current = generation;
    sending = true;
    error = "";
    try {
      await call({ type: "prompt", message: text, streamingBehavior, ...(images.length ? { images } : {}) });
      if (current !== generation) return;
      if (draft === text) { draft = ""; attachments = []; attachmentError = ""; }
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
    if (method === "notify") {
      const text = String(request.message ?? "");
      // 错误级通知进入会话流（与失败的工具/回合同处）；普通通知仍走提示条。
      if (request.notifyType === "error") { notice = ""; extensionError = text; }
      else notice = text;
      return;
    }
    // pi 的字段名是 statusText/widgetLines；兼容旧字段避免漏显。
    if (method === "setStatus") { notice = String(request.statusText ?? request.text ?? ""); return; }
    if (method === "setWidget") {
      const lines = Array.isArray(request.widgetLines) ? request.widgetLines : Array.isArray(request.lines) ? request.lines : [];
      notice = lines.join("\n");
      return;
    }
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
    {#if tabs}
      <div class="header-tabs">{@render tabs()}</div>
    {:else}
      <Bot size={16} /><strong title={title}>{title}</strong>
    {/if}
    {#if sessionStats?.tokens && (Object.values(sessionStats.tokens).some((value) => value > 0) || (sessionStats.cost ?? 0) > 0)}
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
              <dt>{t("上下文占用")}</dt><dd>{formatPercent(sessionStats.contextUsage.percent)}{sessionStats.contextUsage.tokens ? ` · ${formatTokens(sessionStats.contextUsage.tokens)}` : ""}</dd>
            {/if}
          </dl>
        </div>
      </details>
    {/if}
    {#if modelName.trim() && modelName.trim().toLowerCase() !== "unknown"}
      <span class="model-name" title={modelName}>{modelName}</span>
    {/if}
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
                    <span class="turn-body">
                      <span class="turn-summary">{turn.summary}</span>
                      {#if turnDurationLabels.get(turn.number)}
                        <span class="turn-duration">{t("耗时 {duration}", { duration: turnDurationLabels.get(turn.number) ?? "" })}</span>
                      {/if}
                    </span>
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
    {#each visibleEntries as entry, offset (historyOffset + entry.index)}
      {@const message = entry.message}
      {@const messageIndex = historyOffset + entry.index}
      <article data-message-index={messageIndex} class:user-message={message.role === "user"} class:tool-message={message.role === "toolResult"} class:jump-flash={flashTurnIndex === messageIndex}>
        <div class="message-label">
          {#if message.role === "user"}<UserRound size={14} />{t("你")}
            {#if turnDurationFor(entry.index)}<span class="turn-duration">{t("耗时 {duration}", { duration: turnDurationFor(entry.index) })}</span>{/if}
          {:else if message.role === "compactionSummary"}<Shrink size={14} />{t("上下文压缩")}
            {#if compactionStatLabel(message)}<span class="compaction-tokens">{compactionStatLabel(message)}</span>{/if}
          {:else if message.role === "toolResult"}<Terminal size={14} />{message.toolName ?? t("工具结果")}
            {#if message.toolStartedAt !== undefined}<span class="tool-took">{t("耗时 {duration}", { duration: formatDuration((message.toolEndedAt ?? Date.now()) - message.toolStartedAt) })}</span>{/if}
          {:else}<Bot size={14} />Pi{/if}
        </div>
        {#if message.role === "compactionSummary"}
          <div class="compaction-summary">{typeof message.summary === "string" ? message.summary : contentText(message.content)}</div>
        {:else if message.role === "toolResult"}
          <MessageDisclosure title={message.toolName ?? t("工具输出")} defaultOpen={chatDetailLevel === "verbose"}>
            {#if messageModule}
              {#await messageModule}
                <pre>{messageSource(message.content)}</pre>
              {:then module}<module.default {message} onOpenLink={openMessageLink} detail={chatDetailLevel} />
              {:catch}<pre>{messageSource(message.content)}</pre>{/await}
            {/if}
          </MessageDisclosure>
        {:else}
          <div class="message-content">
            {#if messageModule}
              {#await messageModule}
                <div class="plain-message">{messageSource(message.content)}</div>
              {:then module}<module.default {message} onOpenLink={openMessageLink} detail={chatDetailLevel} />
              {:catch}
                <div class="plain-message">{messageSource(message.content)}</div>
                <button type="button" title={t("重新加载消息显示")} aria-label={t("重新加载消息显示")} onclick={() => { messageModule = null; }}><RefreshCw size={15} /></button>
              {/await}
            {:else}<div class="plain-message">{messageSource(message.content)}</div>{/if}
          </div>
        {/if}
      </article>
      {#if isTurnBoundary(offset) && turnTotalLabel(entry.index)}
        <p class="turn-total">{turnTotalLabel(entry.index)}</p>
      {/if}
    {/each}
    {#if conversation.compacting}
      <article class="compaction-live">
        <div class="message-label"><Shrink size={14} />{t("上下文压缩")}</div>
        <p class="compaction-running"><span class="spin"><RefreshCw size={13} /></span>{t("正在压缩上下文，历史消息将被总结为摘要…")}</p>
      </article>
    {/if}
    {#if extensionError}
      <article class="chat-error extension-error" role="alert">
        <div class="message-label"><CircleAlert size={14} />{t("扩展错误")}</div>
        <p class="chat-error-text">{extensionError}</p>
      </article>
    {/if}
    {#if error || conversation.error}
      <article class="chat-error" role="alert">
        <div class="message-label"><CircleAlert size={14} />{t("错误")}</div>
        <p class="chat-error-text">{tm(error || conversation.error)}</p>
        <button type="button" class="chat-error-retry" onclick={() => { reconnect++; }}><RefreshCw size={13} />{t("重新连接会话")}</button>
      </article>
    {/if}
    {#each runningTools as tool (tool.id)}
      <div class="tool-call live">
        <div class="tool-call-head">
          <span class="activity-dot {outputActivity(tool, elapsedNow)}" aria-hidden="true"></span>
          <Terminal size={14} />
          <span class="tool-call-name">{toolCallSummary(tool.name, tool.args)}</span>
          {#if tool.startedAt !== undefined && elapsedNow - tool.startedAt >= LONG_TOOL_MS}
            <span class="tool-call-badge">{t("长时间运行")}</span>
          {/if}
          <span class="tool-call-state">
            {#if tool.startedAt !== undefined}{formatDuration(elapsedNow - tool.startedAt)}{/if}{silenceLabel(tool, elapsedNow)}
          </span>
          <button type="button" class="tool-call-interrupt" disabled={stopping} title={t("中断当前执行")} aria-label={t("中断当前执行")}
            onclick={() => void interrupt()}><Square size={11} />{t("中断")}</button>
        </div>
        {#if chatDetailLevel === "concise"}
          {#if liveOutputLastLine(tool)}<p class="tool-call-preview">{liveOutputLastLine(tool)}</p>{/if}
        {:else if liveOutputTail(tool)}
          <pre class="tool-call-live-output" use:pinOutputBottom={liveOutputTail(tool)}>{liveOutputTail(tool)}</pre>
        {:else}
          <p class="tool-call-waiting">{t("等待工具输出…")}</p>
        {/if}
      </div>
    {/each}
    {#if chatDetailLevel !== "concise"}
      {#each finishedLiveTools as tool (tool.id)}
        <details class="tool-call" open={chatDetailLevel === "verbose"}>
          <summary>
            <Terminal size={14} /><span class="tool-call-name">{toolCallSummary(tool.name, tool.args)}</span>
            <span class="tool-call-state">{tool.isError ? t("失败") : t("完成")}</span>
          </summary>
          <pre>{JSON.stringify(tool.args, null, 2)}</pre>
          {#if tool.result}<pre>{contentText(record(tool.result).content)}</pre>{/if}
        </details>
      {/each}
    {/if}
    {#if liveStatusVisible}
      <p class="turn-total live-status" role="status" class:stalled={toolStalled}>
        {#if activeTool}<span class="activity-dot {outputActivity(activeTool, elapsedNow)}" aria-hidden="true"></span>{/if}
        {statusText}
      </p>
    {/if}
    {#if initializing}<p role="status">{t("正在连接 Pi 会话…")}</p>{/if}
    {#if !initializing && !visibleEntries.length && !error}
      <div class="empty-conversation"><Bot size={28} /><h2>{title}</h2>
        {#if dormant}<p class="empty-hint">{t("输入提示词并回车，启动会话")}</p>{/if}
      </div>
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
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if restoredDraft}
    <button class="restore-draft" type="button" onclick={() => { draft = [draft, restoredDraft].filter(Boolean).join("\n\n"); restoredDraft = ""; }}>{t("恢复暂存文本")}</button>
  {/if}
  {#if connected && !initializing && !conversation.closed && models.length === 0}
    <p class="model-hint" role="status">
      {#if modelsLoadFailed}
        <span>{t("模型列表读取失败：{error}", { error: modelsLoadError })}</span>
      {:else}
        <span>{t("未检测到可用模型：请在 设置 → 模型设置 中添加并保存模型，重启本任务后生效。")}</span>
      {/if}
      <span class="model-hint-actions">
        {#if onOpenModelSettings && !modelsLoadFailed}<button type="button" onclick={onOpenModelSettings}>{t("打开模型设置")}</button>{/if}
        <button type="button" onclick={() => void refreshModels()}>{t("重试")}</button>
        {#if onReloadSession}<button type="button" onclick={reloadSession}>{t("重载会话")}</button>{/if}
      </span>
    </p>
  {/if}
  {#if modelsStale && onReloadSession}
    <p class="model-hint" role="status">
      <span>{t("模型配置已在设置中更新，重载会话后生效")}</span>
      <span class="model-hint-actions">
        <button type="button" onclick={reloadSession}>{t("重载会话")}</button>
      </span>
    </p>
  {/if}
  {#if !followScroll && conversation.messages.length}
    <div class="scroll-actions">
      <div class="scroll-actions-row">
        <button type="button" title={t("回到最新消息")} aria-label={t("回到最新消息")} onclick={returnToLatest}><ArrowDown size={16} /></button>
        {#if activeTool}
          <button type="button" class="live-chip" title={t("回到实时输出")} aria-label={t("回到实时输出")} onclick={returnToLatest}>
            <span class="activity-dot {outputActivity(activeTool, elapsedNow)}" aria-hidden="true"></span>
            {activeToolChipLabel(activeTool)}
            <ArrowDown size={13} />
          </button>
        {/if}
      </div>
    </div>
  {/if}

  <form class="composer" class:dragging={draggingFiles} onsubmit={(event) => { event.preventDefault(); void send(); }}
    ondragover={onComposerDragOver} ondragleave={onComposerDragLeave} ondrop={onComposerDrop}>
    {#if pendingEntries.length}
      <ul class="pending" role="list" aria-label={t("待发送提示词")}>
        {#each pendingEntries as entry (entry.key)}
          <li class="pending-item">
            <span class="pending-mode" class:steer={entry.mode === "steer"} title={entry.mode === "steer" ? t("优先引导") : t("排队跟进")}>{entry.mode === "steer" ? t("引导") : t("排队")}</span>
            <span class="pending-text" title={entry.text}>{entry.text}</span>
            <span class="pending-actions">
              <button type="button" title={t("编辑这条提示词")} aria-label={t("编辑这条提示词")} onclick={() => void editPending(entry)}><Pencil size={13} /></button>
              <button type="button" title={t("撤回这条提示词")} aria-label={t("撤回这条提示词")} onclick={() => void withdrawPending(entry)}><Undo2 size={13} /></button>
              <button type="button" title={entry.mode === "steer" ? t("改为排队跟进") : t("改为优先引导")} aria-label={entry.mode === "steer" ? t("改为排队跟进") : t("改为优先引导")} onclick={() => void movePending(entry)}><ArrowDownUp size={13} /></button>
            </span>
          </li>
        {/each}
      </ul>
    {/if}
    {#if attachments.length}
      <div class="attachments">
        {#each attachments as file (file.id)}
          <figure class="attachment">
            <img src={`data:${file.mimeType};base64,${file.data}`} alt={file.name} loading="lazy" />
            <button type="button" title={t("移除图片")} aria-label={t("移除图片 {name}", { name: file.name })} onclick={() => removeAttachment(file.id)}><X size={12} /></button>
            <figcaption title={file.name}>{file.name}<small>{formatBytes(file.size)}</small></figcaption>
          </figure>
        {/each}
      </div>
    {/if}
    {#if attachmentError}<p class="attachment-error" role="alert">{attachmentError}</p>{/if}
    {#if slashItems.length}
      <ul class="slash-menu" role="listbox" aria-label={t("命令建议")}>
        {#each slashItems as item, index (`${item.source}:${item.name}`)}
          <li role="option" aria-selected={index === slashIndex}>
            <button type="button" class:active={index === slashIndex}
              onmousedown={(event) => { event.preventDefault(); acceptSlash(item); }}>
              <span class="slash-name">/{item.name}</span>
              <span class="slash-desc">{t(item.description)}</span>
              <span class="slash-source" class:terminal={!item.available}>{slashSourceLabel(item)}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <textarea bind:this={input} bind:value={draft} rows="3" maxlength="131072" aria-keyshortcuts={shortcutAria("composer")}
      aria-label={t("发送给 Pi")} placeholder={dormant ? t("输入提示词并回车，启动会话") : conversation.closed ? t("会话已停止") : conversation.busy ? t("AI 执行中，可继续发送新消息") : t("发送消息；输入 / 使用命令")}
      disabled={conversation.closed}
      oncompositionstart={() => { composing = true; }} oncompositionend={() => { composing = false; }}
      onpaste={onComposerPaste}
      onkeydown={(event) => {
        if (event.isComposing || composing) return;
        // 命令建议弹层打开时，方向键/回车/Tab/Esc 先交给弹层。
        if (slashItems.length) {
          if (event.key === "ArrowDown") { event.preventDefault(); moveSlash(1); return; }
          if (event.key === "ArrowUp") { event.preventDefault(); moveSlash(-1); return; }
          if (event.key === "Enter" || event.key === "Tab") { event.preventDefault(); acceptSlash(slashItems[Math.min(slashIndex, slashItems.length - 1)]); return; }
          if (event.key === "Escape") { event.preventDefault(); slashDismissed = true; return; }
        }
        if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void send(); }
      }}></textarea>
    <div class="composer-actions">
      {#if models.length}
        <select aria-label={t("对话模型")} value={selectedModel} disabled={switching || !connected || changingModel}
          title={conversation.busy ? t("当前回复生成中；改动对下一条消息生效") : undefined}
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
        <select aria-label={t("推理强度")} value={thinkingLevel} disabled={switching || !connected || changingThinking}
          title={conversation.busy ? t("当前回复按发送时的档位生成；改动对下一条消息生效") : undefined}
          onchange={(event) => {
            const select = event.currentTarget;
            void changeThinkingLevel(select.value).then(() => { select.value = thinkingLevel; });
          }}>
          {#if !thinkingLevels.includes(thinkingLevel)}
            <option value={thinkingLevel} disabled>{thinkingLevel || t("默认")}</option>
          {/if}
          {#each thinkingLevels as level (level)}
            <option value={level}>{level}</option>
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
      {#if workflows.length}
        <select aria-label={t("工作流")} value="" disabled={workflowBusy}
          title={t("技能安装后按任务自动激活；选中工作流只把站点的起手提示填进输入框")}
          onchange={(event) => {
            const select = event.currentTarget;
            const slug = select.value;
            select.value = "";             // 提示已填入草稿：恢复为「不使用工作流」，避免误以为选择持续生效
            void applyWorkflow(slug);
          }}>
          <option value="">{t("不使用工作流")}</option>
          {#each workflows as workflow (workflow.slug)}
            <option value={workflow.slug}>{workflow.name}{workflowMeta(workflow)}</option>
          {/each}
        </select>
        <small class="workflow-hint" title={t("技能安装后按任务自动激活；选中工作流只把站点的起手提示填进输入框")}>{t("技能安装后按任务自动激活；选中工作流只把站点的起手提示填进输入框")}</small>
      {/if}
      <div class="primary-actions">
        <button type="button" class="send-button" title={t("增强提示词（改写为更清晰的结构化提示）")} aria-label={t("增强提示词")}
          disabled={!canEnhance} onclick={() => void enhanceDraft()}>
          {#if enhancing}<span class="spin"><RefreshCw size={14} /></span>{:else}<Sparkles size={15} />{/if}
        </button>
        {#if conversation.busy || conversation.queue.length}
          <button type="button" class="send-button stop-button" class:stalled={toolStalled} disabled={switching || stopping} title={t("停止当前响应并清空队列")} aria-label={t("停止当前响应并清空队列")} onclick={() => void interrupt()}><Square size={16} /></button>
        {/if}
        <button type="submit" class="send-button primary-send" disabled={!canSend} title={sendHint} aria-label={sendHint}><ArrowUp size={18} /></button>
      </div>
    </div>
  </form>
</section>

<style>
  .chat-pane { position: relative; display: flex; flex-direction: column; grid-template-rows: none; min-height: 0; min-width: 0; border: 1px solid var(--border); background: var(--page-bg); overflow: hidden; }
  .chat-pane.hidden { display: none; }
  header { display: flex; align-items: center; gap: 4px; flex-shrink: 0; min-width: 0; min-height: 34px; padding: 2px 8px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .header-tabs { flex: 1; min-width: 0; }
  .chat-pane { container-type: inline-size; }
  header strong { flex: 1; font-size: 12px; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .model-name { max-width: 20%; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); font-size: 11px; }
  .stats { position: relative; flex-shrink: 0; }
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
  header button { display: grid; place-items: center; width: 26px; height: 26px; padding: 0; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); flex-shrink: 0; }
  .transcript { flex: 1 1 0; min-height: 0; overflow: auto; padding: 16px 24px 8px; overflow-anchor: none; }
  article { max-width: 900px; margin: 0 auto 24px; outline: 2px solid transparent; outline-offset: 3px; transition: outline-color .3s ease; }
  /* 压缩摘要卡片：与消息同宽（900px 居中），摘要默认全文展示、过长时内部滚动。 */
  .compaction-summary {
    max-height: 300px; overflow-y: auto; overflow-wrap: anywhere; white-space: pre-wrap;
    font-size: 12.5px; line-height: 1.6; color: var(--text-muted);
    border-left: 2px solid var(--border-strong); padding: 2px 0 2px 12px;
  }
  .compaction-tokens { font-size: 11px; color: var(--text-muted); opacity: .85; }
  .compaction-live { max-width: 900px; margin: 0 auto 24px; }
  .compaction-running { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 12.5px; color: var(--text-muted); }
  .message-label { display: flex; align-items: center; gap: 6px; margin-bottom: 8px; font-size: 12px; color: var(--text-muted); }
  .turn-duration { padding: 1px 8px; border: 1px solid var(--border); border-radius: 999px; font-size: 11px; line-height: 1.4; color: var(--text-muted); font-family: var(--code-font); white-space: nowrap; }
  .tool-took { color: var(--text-muted); font-size: 11px; font-family: var(--code-font); white-space: nowrap; }
  .turn-total { max-width: 900px; margin: -14px auto 24px; padding-left: 2px; color: var(--text-muted); font-size: 11px; font-family: var(--code-font); }
  .message-content { min-width: 0; overflow-wrap: anywhere; line-height: 1.7; color: var(--text); font-family: var(--session-font, var(--text-font)); font-size: var(--session-font-size, 13px); }
  .plain-message { white-space: pre-wrap; }
  .scroll-actions { height: 0; position: relative; display: flex; justify-content: center; z-index: 1; }
  .scroll-actions-row { position: absolute; bottom: 10px; display: flex; align-items: center; gap: 8px; }
  .scroll-actions-row button { display: grid; place-items: center; padding: 0; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); }
  .scroll-actions-row > button:first-child { width: 30px; height: 30px; flex-shrink: 0; }
  /* 上滚时的实时胶囊：正在执行的工具 + 耗时 + 活跃点，点击回到底部。 */
  .live-chip { display: inline-flex; align-items: center; gap: 6px; height: 30px; padding: 0 12px; border-radius: 999px !important; font-size: 11px; font-family: var(--code-font); max-width: min(560px, 80vw); }
  .live-chip > span:last-child { flex-shrink: 0; color: var(--text-muted); }
  .live-chip { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  /* 输出活跃度点：绿=输出更新中，灰=静默，琥珀=长时间无输出（疑似停滞）。 */
  .activity-dot { width: 7px; height: 7px; flex-shrink: 0; border-radius: 50%; background: var(--text-muted); }
  .activity-dot.fresh { background: #58a661; animation: activity-pulse 1.6s ease-in-out infinite; }
  .activity-dot.stale { background: #d8a04a; animation: activity-pulse 1.6s ease-in-out infinite; }
  @keyframes activity-pulse { 0%, 100% { opacity: 1; } 50% { opacity: .3; } }
  .user-message .message-content { padding: 12px; border-left: 2px solid var(--accent); background: var(--surface); }
  .tool-call { max-width: 900px; border: 1px solid var(--border); border-radius: 4px; padding: 8px 12px; margin: 0 auto 8px; font-size: 12px; }
  /* 运行中的工具卡：具体参数摘要 + 活跃点 + 实时输出尾部，让「AI 在做什么」可见。 */
  .tool-call.live { border-color: color-mix(in srgb, var(--accent) 30%, var(--border)); }
  .tool-call-head { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .tool-call-badge { flex-shrink: 0; padding: 1px 8px; border: 1px solid color-mix(in srgb, #d8a04a 55%, transparent); border-radius: 999px; font-size: 10px; color: #d8a04a; white-space: nowrap; }
  .tool-call-interrupt { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; padding: 2px 8px; border: 1px solid var(--border); border-radius: 4px; background: transparent; color: var(--text-muted); font-size: 11px; }
  .tool-call-interrupt:hover:not(:disabled) { color: var(--text); background: var(--surface-hover); }
  .tool-call-live-output { max-height: 200px; margin: 6px 0 0; padding: 6px 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--page-bg); color: var(--text-muted); white-space: pre-wrap; overflow-wrap: anywhere; }
  .tool-call-preview { margin: 6px 0 0; color: var(--text-muted); font: 11px/1.5 var(--code-font); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tool-call-waiting { margin: 6px 0 0; font-size: 11px; color: var(--text-muted); }
  summary { cursor: pointer; overflow-wrap: anywhere; display: flex; align-items: center; gap: 8px; }
  summary :global(svg) { flex-shrink: 0; }
  .tool-call-name { min-width: 0; font-weight: 700; color: var(--text); overflow-wrap: anywhere; }
  .tool-call-state { margin-left: auto; flex-shrink: 0; color: var(--text-muted); font-family: var(--code-font); font-size: 11px; white-space: nowrap; }
  pre { overflow: auto; max-height: 280px; font: 12px/1.5 var(--code-font); tab-size: 4; }
  .empty-conversation { display: grid; place-content: center; justify-items: center; min-height: 180px; color: var(--text-muted); }
  .empty-hint { margin: 8px 0 0; font-size: 12px; }
  h2 { font-size: 16px; font-weight: 500; overflow-wrap: anywhere; }
  /* 指令坞：常规流式布局的磨砂胶囊——会话内容始终结束于输入框上方，错误与提示条排在输入框上方，不会被遮挡。 */
  .composer {
    position: relative;
    flex-shrink: 0;
    width: min(760px, calc(100% - 32px));
    margin: 0 auto 16px;
    padding: 8px 10px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 16px;
    background: color-mix(in srgb, var(--surface) 88%, transparent);
    box-shadow: 0 10px 30px #0006;
  }
  /* `/` 命令建议：浮于输入框上方，不挤压布局。 */
  .slash-menu {
    position: absolute; left: 6px; right: 6px; bottom: calc(100% + 6px); z-index: 24;
    max-height: 264px; overflow-y: auto; margin: 0; padding: 4px; list-style: none;
    border: 1px solid var(--border-strong); border-radius: 8px; background: var(--surface-raised);
    box-shadow: 0 10px 28px #0007;
  }
  .slash-menu button { display: flex; align-items: baseline; gap: 8px; width: 100%; padding: 6px 8px; border: 0; border-radius: 5px; background: transparent; color: var(--text); text-align: left; cursor: pointer; }
  .slash-menu button.active, .slash-menu button:hover { background: var(--surface-hover); }
  .slash-name { flex-shrink: 0; font-family: var(--code-font); font-size: 12px; color: var(--accent); }
  .slash-desc { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 11px; color: var(--text-muted); }
  .slash-source { flex-shrink: 0; padding: 1px 6px; border: 1px solid var(--border); border-radius: 999px; font-size: 10px; color: var(--text-muted); }
  .slash-source.terminal { opacity: .65; }
  /* 三个主操作（增强/停止/发送）统一靠右。 */
  .primary-actions { display: flex; align-items: center; gap: 6px; flex-shrink: 0; margin-left: auto; }
  .composer-actions { display: flex; align-items: center; gap: 8px; margin-top: 4px; padding: 6px 2px 2px; border-top: 1px solid color-mix(in srgb, var(--text) 8%, transparent); }
  /* 粘贴/拖入的图片附件：缩略图胶囊；拖拽悬停时输入框高亮。 */
  .composer.dragging { border-color: var(--accent); box-shadow: 0 10px 30px #0006, 0 0 0 2px color-mix(in srgb, var(--accent) 35%, transparent); }
  .attachments { display: flex; flex-wrap: wrap; gap: 8px; padding: 2px 2px 6px; }
  .attachment { position: relative; display: flex; align-items: center; gap: 6px; margin: 0; padding: 4px 26px 4px 4px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface-alt); max-width: 220px; }
  .attachment img { width: 32px; height: 32px; flex-shrink: 0; border-radius: 5px; object-fit: cover; border: 1px solid var(--border); }
  .attachment figcaption { min-width: 0; display: grid; line-height: 1.3; font-size: 11px; color: var(--text); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .attachment small { color: var(--text-muted); font-family: var(--code-font); font-size: 10px; }
  .attachment button { position: absolute; top: 2px; right: 2px; display: grid; place-items: center; width: 18px; height: 18px; padding: 0; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; }
  .attachment button:hover { color: var(--text); background: var(--surface-hover); }
  .attachment-error { margin: 0 2px 4px; color: #d46b61; font-size: 11px; overflow-wrap: anywhere; }
  /* 待发送提示词：输入框上方的引导/排队列表，每条右侧可编辑、撤回、切换投递方式。 */
  .pending { display: flex; flex-direction: column; gap: 4px; margin: 0 0 6px; padding: 0; list-style: none; }
  .pending-item { display: flex; align-items: center; gap: 8px; padding: 5px 8px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface-alt); font-size: 12px; }
  .pending-mode { flex-shrink: 0; padding: 1px 8px; border: 1px solid var(--border); border-radius: 999px; font-size: 10px; color: var(--text-muted); white-space: nowrap; }
  .pending-mode.steer { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 45%, transparent); }
  .pending-text { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; color: var(--text); }
  .pending-actions { display: inline-flex; gap: 2px; flex-shrink: 0; }
  .pending-actions button { display: grid; place-items: center; width: 24px; height: 24px; padding: 0; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); cursor: pointer; }
  .pending-actions button:hover { color: var(--text); background: var(--surface-hover); }
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
  .workflow-hint { flex: 0 1 auto; min-width: 0; max-width: 240px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; color: var(--text-muted); font-size: 10px; }
  /* AI 行为状态与耗时：迁入对话流底部（turn-total 同位置），不再显示在输入框操作行。 */
  .live-status { display: flex; align-items: center; gap: 6px; }
  .live-status.stalled { color: #d8a04a; }
  .send-button { display: grid; place-items: center; width: 30px; height: 30px; padding: 0; flex-shrink: 0; border: 0; border-radius: 4px; background: var(--surface-hover); color: var(--text); }
  .send-button.stop-button { background: color-mix(in srgb, var(--status-failed) 18%, var(--surface-hover)); color: var(--status-failed); }
  /* 工具长时间无输出：停止按钮琥珀色呼吸，就近提示可中断。 */
  .send-button.stalled { background: color-mix(in srgb, #d8a04a 22%, var(--surface-hover)); color: #d8a04a; animation: activity-pulse 1.6s ease-in-out infinite; }
  .primary-send { background: var(--accent); color: var(--accent-ink); }
  button:disabled { opacity: .4; cursor: default; }
  /* 错误卡片：与其他内容同宽（900px 居中），显示在会话流内随内容滚动，不再常驻输入框上方。 */
  .chat-error { max-width: 900px; margin: 0 auto 24px; padding: 10px 12px; border: 1px solid #bd5147; border-radius: 6px; background: rgb(189 81 71 / 8%); color: #d46b61; overflow-wrap: anywhere; }
  .chat-error .message-label { margin-bottom: 6px; color: #d46b61; }
  .chat-error-text { margin: 0 0 8px; font-size: 12.5px; line-height: 1.55; white-space: pre-wrap; }
  .chat-error-retry { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border: 1px solid #bd5147; border-radius: 4px; background: transparent; color: #d46b61; font-size: 12px; cursor: pointer; }
  .chat-error-retry:hover { background: rgb(189 81 71 / 15%); }
  .notice { flex-shrink: 0; margin: 4px 16px 8px; color: var(--text-muted); white-space: pre-wrap; overflow-wrap: anywhere; font-size: 12px; }
  .model-hint { flex-shrink: 0; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0 16px 8px; padding: 6px 10px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text-muted); font-size: 12px; overflow-wrap: anywhere; }
  .model-hint-actions { display: inline-flex; gap: 6px; }
  .model-hint button { padding: 3px 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface-raised); color: var(--text); font: inherit; font-size: 11px; cursor: pointer; }
  .model-hint button:hover { background: var(--surface-hover); }
  .restore-draft { flex-shrink: 0; padding: 6px 10px; margin: 4px 16px 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text); }
  .more-history { display: block; width: max-content; padding: 6px 10px; margin: 4px auto 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text); }
  .turn-nav { position: relative; display: flex; align-items: center; min-width: 0; flex: 0 1 auto; }
  .turn-trigger { display: flex; align-items: center; gap: 5px; width: auto; height: 24px; max-width: 240px; padding: 0 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-alt); color: var(--text-muted); font-size: 11px; }
  .turn-trigger:hover, .turn-trigger[aria-expanded="true"] { background: var(--surface-hover); border-color: var(--border-strong); color: var(--text); }
  .turn-label { min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .turn-panel { position: absolute; top: calc(100% + 6px); right: 0; z-index: 40; width: 300px; max-width: min(320px, 90vw); max-height: 340px; overflow-y: auto; overflow-x: hidden; padding: 6px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface-raised); box-shadow: 0 10px 26px #0006; }
  .turn-panel-head { margin: 0; padding: 4px 8px 6px; color: var(--text-muted); font-size: 10px; }
  .turn-panel ul { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .turn-panel button { display: flex; align-items: flex-start; gap: 8px; width: 100%; padding: 6px 8px; border: 1px solid transparent; border-radius: 6px; background: transparent; color: var(--text); font: inherit; font-size: 11px; line-height: 1.4; text-align: left; cursor: pointer; }
  .turn-panel button:hover { background: var(--surface-hover); }
  .turn-panel button.active { border-color: var(--accent); background: var(--surface-alt); }
  .turn-no { flex-shrink: 0; min-width: 20px; color: var(--accent); font-family: var(--code-font); }
  .turn-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .turn-summary { min-width: 0; white-space: normal; word-break: break-word; color: var(--text-muted); }
  .turn-panel button.active .turn-summary { color: var(--text-strong); }
  .turn-duration { white-space: normal; color: var(--text-muted); opacity: .75; font-size: 10px; line-height: 1.2; }
  .turn-time { flex-shrink: 0; margin-left: auto; color: var(--text-muted); font-size: 10px; white-space: nowrap; }
  .turn-rail { position: absolute; top: 50%; right: 6px; transform: translateY(-50%); z-index: 3; display: flex; flex-direction: column; align-items: center; gap: 6px; max-height: calc(100% - 24px); overflow-y: auto; padding: 4px 2px; }
  .turn-dot { width: 8px; height: 8px; flex-shrink: 0; padding: 0; border: 0; border-radius: 999px; background: var(--border-strong); opacity: .75; transition: background .15s ease, height .15s ease, opacity .15s ease; }
  @media (max-width: 620px) { .transcript { padding: 12px; } .composer { margin: 8px auto 8px; width: calc(100% - 16px); } }
  .turn-dot:hover { background: var(--accent); opacity: 1; }
  .turn-dot.active { height: 18px; background: var(--accent); opacity: 1; }
  article.jump-flash { outline-color: var(--accent); }
  @container (max-width: 600px) {
    .model-name, .turn-label { display: none; }
    .stats summary { gap: 4px; padding: 2px; }
    .stats summary .hit, .stats summary .cost { display: none; }
    .turn-trigger { padding: 0 4px; }
  }
</style>
