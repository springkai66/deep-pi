<script lang="ts">
  import { Check, Copy, FileText } from "@lucide/svelte";
  import type { RpcMessage } from "./rpc-state";
  import { messageParts, messageSource } from "./message-parts";
  import type { ChatDetailLevel } from "./settings";
  import MessageMarkdown from "./MessageMarkdown.svelte";
  import MessageCodeBlock from "./MessageCodeBlock.svelte";
  import MessageDisclosure from "./MessageDisclosure.svelte";
  import { t, tm } from "$lib/i18n.svelte";
  let { message, onOpenLink, detail = "standard" }: { message: RpcMessage; onOpenLink: (url: string) => void; detail?: ChatDetailLevel } = $props();
  /// 「简洁」档只显示文本与图片（隐藏思考/工具调用等过程内容）；「详细」档过程内容默认展开。
  const parts = $derived(
    detail === "concise"
      ? messageParts(message.content).filter((part) => part.kind === "text" || part.kind === "image")
      : messageParts(message.content),
  );
  const detailOpen = $derived(detail === "verbose");
  let sourceMode = $state(false);
  let copied = $state(false);
  let copying = $state(false);
  let error = $state("");
  $effect(() => { void message.content; copied = false; });
  async function copy() {
    if (copying) return;
    const content = message.content;
    copying = true;
    error = "";
    try { await navigator.clipboard.writeText(messageSource(content)); copied = content === message.content; }
    catch (cause) { error = t("复制失败：{error}", { error: tm(String(cause)) }); }
    finally { copying = false; }
  }
</script>

<div class="chat-message-content">
  {#if sourceMode}
    <pre class="source">{messageSource(message.content)}</pre>
  {:else}
    {#each parts as part}
      {#if part.kind === "text"}
        {#if message.role === "toolResult"}<MessageCodeBlock content={part.content} />
        {:else if message.role === "user"}<div class="user-text">{part.content}</div>
        {:else}<MessageMarkdown content={part.content} {onOpenLink} />{/if}
      {:else if part.kind === "thinking"}
        <MessageDisclosure title={t("思考")} variant="thinking" defaultOpen={detailOpen}><MessageMarkdown content={part.content} {onOpenLink} /></MessageDisclosure>
      {:else if part.kind === "tool"}
        <MessageDisclosure title={t("{name} · 工具参数", { name: part.name })} defaultOpen={detailOpen}><MessageCodeBlock content={part.content} language="json" /></MessageDisclosure>
      {:else if part.kind === "image"}
        <MessageDisclosure title={part.content} defaultOpen={detailOpen}>
          {#if part.source}<img src={part.source} alt={t("会话图片")} loading="lazy" onerror={(event) => { event.currentTarget.setAttribute("alt", t("图片无法解码")); }} />
          {:else}<p>{t("图片类型或大小不受支持。")}</p>{/if}
        </MessageDisclosure>
      {:else}
        <MessageDisclosure title={t("未识别的消息内容")} defaultOpen={detailOpen}><MessageCodeBlock content={part.content} language="json" /></MessageDisclosure>
      {/if}
    {/each}
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <footer>
    <button type="button" title={copied ? t("已复制") : t("复制消息")} aria-label={copied ? t("已复制") : t("复制消息")} disabled={copying} onclick={copy}>
      {#if copied}<Check size={14} />{:else}<Copy size={14} />{/if}
    </button>
    {#if message.role !== "user"}
      <button type="button" title={t("原始文本")} aria-label={t("原始文本")} aria-pressed={sourceMode} onclick={() => { sourceMode = !sourceMode; }}><FileText size={14} /></button>
    {/if}
  </footer>
</div>

<style>
  .chat-message-content { min-width: 0; }
  .user-text, .source { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.7; color: var(--text); }
  .source { font: 12px/1.7 var(--code-font); margin: 0; }
  img { display: block; max-width: 100%; max-height: 480px; object-fit: contain; }
  p { font-size: 12px; overflow-wrap: anywhere; }
  footer { display: flex; gap: 4px; margin-top: 8px; }
  button { display: grid; place-items: center; width: 26px; height: 26px; padding: 0; border: 0; border-radius: 4px; color: var(--text-muted); background: transparent; cursor: pointer; }
  button:hover, button[aria-pressed="true"] { color: var(--text); background: var(--surface-hover); }
  button:disabled { opacity: .4; cursor: default; }
</style>
