<script lang="ts">
  import { parseMessageMarkdown, type MarkdownNode } from "./message-markdown";
  import MessageCodeBlock from "./MessageCodeBlock.svelte";
  import { t } from "$lib/i18n.svelte";
  let { content, onOpenLink }: { content: string; onOpenLink: (url: string) => void } = $props();
  const parsed = $derived(parseMessageMarkdown(content));
</script>

{#snippet children(nodes: MarkdownNode[])}
  {#each nodes as node}
    {#if node.kind === "text"}{node.content}
    {:else if node.kind === "break"}<br />
    {:else if node.kind === "rule"}<hr />
    {:else if node.kind === "code"}
      {#if node.block}<MessageCodeBlock content={node.content} language={node.language} />
      {:else}<code>{node.content}</code>{/if}
    {:else if node.kind === "image"}
      {#if node.href}<a href={node.href} title={node.href} target="_blank" rel="noopener noreferrer" onclick={(event) => { event.preventDefault(); onOpenLink(node.href!); }}
        onauxclick={(event) => { if (event.button === 1) { event.preventDefault(); onOpenLink(node.href!); } }}>{t("图片：{source}", { source: node.content || node.href })}</a>
      {:else}<span>{t("图片：{source}", { source: node.content })}</span>{/if}
    {:else if node.kind === "element"}
      {#if node.tag === "a"}
        {#if node.href}<a href={node.href} title={node.href} target="_blank" rel="noopener noreferrer" onclick={(event) => { event.preventDefault(); onOpenLink(node.href!); }}
          onauxclick={(event) => { if (event.button === 1) { event.preventDefault(); onOpenLink(node.href!); } }}>{@render children(node.children)}</a>
        {:else}<span>{@render children(node.children)}</span>{/if}
      {:else if node.tag === "table"}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable tables need keyboard focus.) -->
        <div class="table-scroll" tabindex="0" role="region" aria-label={t("消息表格")}><table>{@render children(node.children)}</table></div>
      {:else}
        <svelte:element this={node.tag} start={node.start} style:text-align={node.align}>{@render children(node.children)}</svelte:element>
      {/if}
    {/if}
  {/each}
{/snippet}

<div class="message-markdown" class:plain={parsed.plain}>{@render children(parsed.nodes)}</div>

<style>
  .message-markdown { min-width: 0; overflow-wrap: anywhere; line-height: 1.7; color: var(--text); font-family: var(--text-font); }
  .plain { white-space: pre-wrap; }
  .message-markdown :global(p) { margin: 0 0 12px; }
  .message-markdown :global(p:last-child) { margin-bottom: 0; }
  .message-markdown :global(h1), .message-markdown :global(h2), .message-markdown :global(h3),
  .message-markdown :global(h4), .message-markdown :global(h5), .message-markdown :global(h6) { margin: 16px 0 8px; line-height: 1.4; font-weight: 600; font-size: 15px; }
  .message-markdown :global(h1) { font-size: 18px; }
  .message-markdown :global(h2) { font-size: 16px; }
  .message-markdown :global(ul), .message-markdown :global(ol) { padding-left: 24px; margin: 8px 0 12px; }
  .message-markdown :global(li > p) { margin-bottom: 4px; }
  .message-markdown :global(blockquote) { margin: 12px 0; padding-left: 12px; border-left: 3px solid var(--border-strong); color: var(--text-muted); }
  .message-markdown :global(hr) { border: 0; border-top: 1px solid var(--border); margin: 16px 0; }
  a { color: var(--accent); text-decoration: underline; text-underline-offset: 3px; }
  code { padding: 2px 4px; border-radius: 3px; background: var(--surface); font: .92em var(--code-font); white-space: pre-wrap; }
  .table-scroll { max-width: 100%; overflow: auto; margin: 12px 0; }
  table { width: max-content; min-width: 100%; border-collapse: collapse; font-size: 12px; }
  .message-markdown :global(th), .message-markdown :global(td) { border: 1px solid var(--border); padding: 6px 10px; max-width: 480px; }
  .message-markdown :global(th) { background: var(--surface); font-weight: 600; }
</style>
