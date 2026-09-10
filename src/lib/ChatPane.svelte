<script lang="ts">
  import { Channel, invoke } from "@tauri-apps/api/core";
  import { ArrowUp, ArrowDown, Bot, RefreshCw, Square, Terminal, UserRound } from "@lucide/svelte";
  import { tick, untrack } from "svelte";
  import type { DialogRequest, DialogValue } from "./dialog";
  import { applyRpcEvent, contentText, emptyConversation, loadHistory, record, type RpcEvent } from "./rpc-state";
  import { captureTranscriptAnchor, restoreTranscriptAnchor, messageWindowStart, transcriptPort } from "./transcript-scroll";
  import MessageDisclosure from "./MessageDisclosure.svelte";
  import { messageSource } from "./message-parts";
  import { createRpcHistorySession, prependRpcHistory } from "./rpc-history";
  import { shortcutAria } from "./shortcuts";

  interface Props {
    taskId: string;
    runId: string | null | undefined;
    title: string;
    visible: boolean;
    active: boolean;
    switching: boolean;
    focusToken: number;
    onUseTerminal: () => void;
    onDialog: (request: Omit<DialogRequest, "id" | "resolve">) => Promise<DialogValue>;
    onCancelDialogs: (scope: string) => void;
    onActivity: (busy: boolean) => void;
  }
  let { taskId, runId, title, visible, active, switching, focusToken, onUseTerminal, onDialog, onCancelDialogs, onActivity }: Props = $props();
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
  const canSend = $derived(!switching && connected && !initializing && !sending && !stopping && !conversation.closed && !!draft.trim());
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
          if (event.payload.type === "agent_settled") void call({ type: "get_state" }).catch(() => {});
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
        if (overflow) throw new Error("初始化期间事件过多，请重新连接以读取完整会话");
        let next = loadHistory(emptyConversation(), history.messages, history.eventSequence);
        next.busy = state.isStreaming === true || state.isCompacting === true;
        for (const event of buffered) next = applyRpcEvent(next, event);
        buffered = [];
        ready = true;
        connected = !next.closed;
        conversation = next;
        historyOffset = history.start;
        modelName = String(record(state.model).name ?? record(state.model).id ?? "未选择模型");
        selectedModel = `${String(record(state.model).provider ?? "")}/${String(record(state.model).id ?? "")}`;
        onActivity(next.busy);
        void call<{ models: unknown[] }>({ type: "get_available_models" }).then((result) => {
          if (!alive) return;
          models = result.models.map(record).filter((model) => typeof model.id === "string" && typeof model.provider === "string")
            .map((model) => ({ id: String(model.id), provider: String(model.provider), name: String(model.name ?? model.id) }));
        }).catch(() => {});
      } catch (cause) {
        if (alive) error = String(cause);
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
    } catch (cause) { if (current === generation) error = String(cause); }
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
          scope, kind: "confirm", title: "打开外部链接", message: destination, confirmLabel: "打开",
        }) === true,
        open: openUrl,
      });
    } catch (cause) { if (current === generation) error = String(cause); }
  }

  function updateScroll(event: Event & { currentTarget: HTMLDivElement }) {
    const element = event.currentTarget;
    const following = element.scrollHeight - element.scrollTop - element.clientHeight < 80;
    if (following) pinnedMessageStart = null;
    else if (pinnedMessageStart === null) pinnedMessageStart = messageStart;
    followScroll = following;
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
    error = String(cause);
    if (error.includes("RPC_OUTCOME_UNKNOWN:")) connected = false;
  }

  async function changeModel(value: string) {
    const model = models.find((model) => `${model.provider}/${model.id}` === value);
    if (!model || changingModel) return;
    const current = generation;
    changingModel = true;
    try {
      await call({ type: "set_model", provider: model.provider, modelId: model.id });
      if (current === generation) { selectedModel = value; modelName = model.name; error = ""; }
    } catch (cause) {
      if (current === generation) commandError(cause);
    } finally {
      if (current === generation) changingModel = false;
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
      notice = "此扩展交互需要终端兼容模式";
      response.cancelled = true;
    } else {
      const value = await onDialog({
        scope,
        kind: method === "select" ? "choice" : method === "confirm" ? "confirm" : "input",
        title: `${title} · ${String(request.title ?? "扩展请求")}`,
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
    try { await sendResponse(response); } catch (cause) { if (alive()) error = String(cause); }
  }
</script>

<section class="terminal-pane chat-pane" class:hidden={!visible} class:active aria-label={title}>
  <header>
    <Bot size={16} /><strong title={title}>{title}</strong>
    <span class="model-name" title={modelName}>{modelName}</span>
    <button type="button" disabled={switching} aria-label="切换到终端兼容模式" title="终端兼容模式" onclick={onUseTerminal}><Terminal size={16} /></button>
  </header>
  <div class="transcript" bind:this={transcript} role="log" aria-label="任务对话" aria-live="off"
    onscroll={updateScroll}>
    <div class="transcript-content" bind:this={transcriptContent}>
    {#if messageStart > 0 || historyOffset > 0}
      <button type="button" class="more-history" disabled={showingEarlier} onclick={showEarlier}>显示更早的消息</button>
    {/if}
    {#each visibleMessages as message, index (historyOffset + conversation.messages.length - visibleMessages.length + index)}
      <article data-message-index={historyOffset + conversation.messages.length - visibleMessages.length + index} class:user-message={message.role === "user"} class:tool-message={message.role === "toolResult"}>
        <div class="message-label">
          {#if message.role === "user"}<UserRound size={14} />你
          {:else if message.role === "toolResult"}<Terminal size={14} />{message.toolName ?? "工具结果"}
          {:else}<Bot size={14} />Pi{/if}
        </div>
        {#if message.role === "toolResult"}
          <MessageDisclosure title={message.toolName ?? "工具输出"}>
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
                <button type="button" title="重新加载消息显示" aria-label="重新加载消息显示" onclick={() => { messageModule = null; }}><RefreshCw size={15} /></button>
              {/await}
            {:else}<div class="plain-message">{messageSource(message.content)}</div>{/if}
          </div>
        {/if}
      </article>
    {/each}
    {#each liveTools as tool (tool.id)}
      <details class="tool-call" open={tool.running}>
        <summary><Terminal size={14} />{tool.name}<span>{tool.running ? "执行中" : tool.isError ? "失败" : "完成"}</span></summary>
        <pre>{JSON.stringify(tool.args, null, 2)}</pre>
        {#if tool.result}<pre>{contentText(record(tool.result).content)}</pre>{/if}
      </details>
    {/each}
    {#if initializing}<p role="status">正在连接 Pi 会话…</p>{/if}
    {#if !initializing && !conversation.messages.length && !error}
      <div class="empty-conversation"><Bot size={28} /><h2>{title}</h2></div>
    {/if}
    </div>
  </div>
  {#if !followScroll && conversation.messages.length}
    <div class="scroll-actions">
      <button type="button" title="回到最新消息" aria-label="回到最新消息" onclick={() => {
        followScroll = true;
        pinnedMessageStart = null;
        if (transcript) transcript.scrollTop = transcript.scrollHeight;
      }}><ArrowDown size={16} /></button>
    </div>
  {/if}
  {#if error || conversation.error}
    <div class="chat-error" role="alert"><span>{error || conversation.error}</span>
      <button type="button" title="重新连接会话" aria-label="重新连接会话" onclick={() => { reconnect++; }}><RefreshCw size={15} /></button>
    </div>
  {/if}
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if conversation.queue.length}<p class="notice" role="status">{conversation.queue.length} 条消息排队中</p>{/if}
  {#if restoredDraft}
    <button class="restore-draft" type="button" onclick={() => { draft = [draft, restoredDraft].filter(Boolean).join("\n\n"); restoredDraft = ""; }}>恢复暂存文本</button>
  {/if}
  <form class="composer" onsubmit={(event) => { event.preventDefault(); void send(); }}>
    <textarea bind:this={input} bind:value={draft} rows="3" maxlength="131072" aria-keyshortcuts={shortcutAria("composer")}
      aria-label="发送给 Pi" placeholder={conversation.closed ? "会话已停止" : "发送消息"}
      disabled={conversation.closed}
      oncompositionstart={() => { composing = true; }} oncompositionend={() => { composing = false; }}
      onkeydown={(event) => {
        if (event.isComposing || composing) return;
        if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void send(); }
      }}></textarea>
    <div class="composer-actions">
      {#if models.length}
        <select aria-label="对话模型" value={selectedModel} disabled={switching || !connected || changingModel || conversation.busy}
          onchange={(event) => {
            const select = event.currentTarget;
            void changeModel(select.value).then(() => { select.value = selectedModel; });
          }}>
          {#if !models.some((model) => `${model.provider}/${model.id}` === selectedModel)}
            <option value={selectedModel} disabled>{modelName}</option>
          {/if}
          {#each models as model (`${model.provider}/${model.id}`)}
            <option value={`${model.provider}/${model.id}`}>{model.name}</option>
          {/each}
        </select>
      {/if}
      <select aria-label="运行时消息处理方式" bind:value={streamingBehavior}>
        <option value="followUp">排队跟进</option><option value="steer">优先引导</option>
      </select>
      <span class="connection-status" role="status">{switching ? "正在切换模式" : conversation.closed ? "已停止" : initializing ? "连接中" : !connected ? "未连接" : conversation.busy ? "运行中" : "等待输入"}</span>
      {#if conversation.busy || conversation.queue.length}
        <button type="button" class="send-button" disabled={switching || stopping} title="停止当前响应并清空队列" aria-label="停止当前响应并清空队列" onclick={() => void interrupt()}><Square size={16} /></button>
      {/if}
      <button type="submit" class="send-button primary-send" disabled={!canSend} title="发送消息" aria-label="发送消息"><ArrowUp size={18} /></button>
    </div>
  </form>
</section>

<style>
  .chat-pane { display: flex; flex-direction: column; min-height: 0; min-width: 0; border: 1px solid var(--border); background: var(--page-bg); overflow: hidden; }
  .chat-pane.hidden { display: none; }
  header { display: flex; align-items: center; gap: 8px; min-height: 38px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--surface); }
  header strong { flex: 1; font-size: 12px; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .model-name { max-width: 35%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); font-size: 11px; }
  button { cursor: pointer; }
  header button, .chat-error button { display: grid; place-items: center; width: 26px; height: 26px; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted); flex-shrink: 0; }
  .transcript { flex: 1; min-height: 0; overflow: auto; padding: 16px 24px; overflow-anchor: none; }
  article { max-width: 900px; margin: 0 auto 24px; }
  .message-label { display: flex; align-items: center; gap: 6px; margin-bottom: 8px; font-size: 12px; color: var(--text-muted); }
  .message-content { min-width: 0; overflow-wrap: anywhere; line-height: 1.7; color: var(--text); font-family: var(--text-font); }
  .plain-message { white-space: pre-wrap; }
  .scroll-actions { height: 0; position: relative; display: flex; justify-content: center; z-index: 1; }
  .scroll-actions button { position: absolute; bottom: 12px; width: 30px; height: 30px; display: grid; place-items: center; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); }
  .user-message .message-content { padding: 12px; border-left: 2px solid var(--accent); background: var(--surface); }
  .tool-call { border: 1px solid var(--border); border-radius: 4px; padding: 8px 12px; margin-bottom: 8px; font-size: 12px; }
  summary { cursor: pointer; overflow-wrap: anywhere; }
  summary :global(svg) { vertical-align: middle; margin-right: 6px; }
  summary span { margin-left: 12px; color: var(--text-muted); }
  pre { overflow: auto; max-height: 280px; font: 12px/1.5 var(--code-font); tab-size: 4; }
  .empty-conversation { display: grid; place-content: center; justify-items: center; min-height: 180px; color: var(--text-muted); }
  h2 { font-size: 16px; font-weight: 500; overflow-wrap: anywhere; }
  .composer { margin: 0 16px 16px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface); }
  textarea { display: block; width: 100%; min-height: 72px; max-height: 240px; resize: vertical; padding: 12px; border: 0; background: transparent; color: var(--text); font: 13px/1.5 var(--text-font); }
  .composer-actions { display: flex; align-items: center; gap: 8px; padding: 6px 8px; }
  select { min-width: 0; max-width: 130px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface-alt); padding: 4px; color: var(--text-muted); font-size: 11px; }
  .connection-status { flex: 1; font-size: 11px; color: var(--text-muted); }
  .send-button { display: grid; place-items: center; width: 30px; height: 30px; flex-shrink: 0; border: 0; border-radius: 4px; background: var(--surface-hover); color: var(--text); }
  .primary-send { background: var(--accent); color: var(--accent-ink); }
  button:disabled { opacity: .4; cursor: default; }
  .chat-error { display: flex; align-items: center; gap: 8px; margin: 4px 16px 8px; padding: 8px; border: 1px solid #bd5147; border-radius: 4px; color: #d46b61; overflow-wrap: anywhere; }
  .chat-error span { flex: 1; min-width: 0; }
  .notice { margin: 4px 16px 8px; color: var(--text-muted); white-space: pre-wrap; overflow-wrap: anywhere; font-size: 12px; }
  .restore-draft, .more-history { padding: 6px 10px; margin: 4px 16px 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text); }
  @media (max-width: 620px) { .transcript { padding: 12px; } .composer { margin: 0 8px 8px; } }
</style>
