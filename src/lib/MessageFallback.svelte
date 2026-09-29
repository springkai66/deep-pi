<script lang="ts">
  import { messageParts, messageSource } from "./message-parts";
  import type { RpcMessage } from "./rpc-state";
  import AiError from "./AiError.svelte";
  import type { ChatDetailLevel } from "./settings";
  import { t } from "$lib/i18n.svelte";

  let { message, detail }: { message: RpcMessage; detail: ChatDetailLevel } = $props();
  const parts = $derived(messageParts(message.content).filter((part) => part.kind === "text" || part.kind === "image"));
</script>

<div class="plain-message">
  {#if detail === "verbose"}
    <pre>{messageSource(message.content)}</pre>
  {:else}
    {#each parts as part}
      {#if part.kind === "text"}
        <div class="fallback-text">{part.content}</div>
      {:else if part.kind === "image" && part.source}
        <img src={part.source} alt={t("会话图片")} loading="lazy" onerror={(event) => { event.currentTarget.setAttribute("alt", t("图片无法解码")); }} />
      {:else if part.kind === "image"}
        <p>{t("图片类型或大小不受支持。")}</p>
      {/if}
    {/each}
  {/if}
  {#if message.role === "assistant" && message.errorMessage}
    <div role="alert"><AiError raw={message.errorMessage} /></div>
  {/if}
</div>

<style>
  .plain-message { white-space: pre-wrap; overflow-wrap: anywhere; }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; margin: 0; }
  img { display: block; max-width: 100%; max-height: 480px; object-fit: contain; }
</style>
