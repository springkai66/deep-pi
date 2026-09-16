<script lang="ts">
  import { Check, Copy, WrapText } from "@lucide/svelte";
  import { t, tm } from "$lib/i18n.svelte";
  let { content, language = "" }: { content: string; language?: string } = $props();
  let highlighted = $state<string | null>(null);
  let copied = $state(false);
  let copying = $state(false);
  let wrap = $state(false);
  let error = $state("");
  $effect(() => {
    const source = content;
    const mode = language;
    let alive = true;
    highlighted = null;
    copied = false;
    // Streaming updates stay readable as text; syntax work waits for a short pause.
    const timer = setTimeout(() => {
      if (source.length > 64 * 1024) return;
      void import("./message-code").then((module) => module.highlightMessageCode(source, mode))
        .then((html) => { if (alive) highlighted = html; }).catch(() => {});
    }, 150);
    return () => { alive = false; clearTimeout(timer); };
  });
  async function copy() {
    if (copying) return;
    const source = content;
    copying = true;
    error = "";
    try { await navigator.clipboard.writeText(source); copied = source === content; }
    catch (cause) { error = t("复制失败：{error}", { error: tm(String(cause)) }); }
    finally { copying = false; }
  }
</script>

<div class="message-code">
  <header>
    <span title={language}>{language || "text"}</span>
    <button type="button" title={t("代码自动换行")} aria-label={t("代码自动换行")} aria-pressed={wrap} onclick={() => { wrap = !wrap; }}><WrapText size={15} /></button>
    <button type="button" title={copied ? t("已复制") : t("复制代码")} aria-label={copied ? t("已复制") : t("复制代码")} disabled={copying} onclick={copy}>
      {#if copied}<Check size={15} />{:else}<Copy size={15} />{/if}
    </button>
  </header>
  {#if error}<p role="alert">{error}</p>{/if}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable code needs keyboard focus.) -->
  <pre class:wrap tabindex="0" role="region" aria-label={t("代码块")}><code>{#if highlighted !== null}{@html highlighted}{:else}{content}{/if}</code></pre>
</div>

<style>
  .message-code { margin: 12px 0; border: 1px solid var(--border); border-radius: 4px; overflow: hidden; background: var(--surface); min-width: 0; }
  header { display: flex; gap: 4px; align-items: center; padding: 4px 8px; border-bottom: 1px solid var(--border); }
  header span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: 11px var(--code-font); color: var(--text-muted); }
  button { display: grid; place-items: center; width: 28px; height: 28px; padding: 0; flex: 0 0 28px; border: 0; border-radius: 4px; color: var(--text-muted); background: transparent; cursor: pointer; }
  button:hover, button[aria-pressed="true"] { background: var(--surface-hover); color: var(--text); }
  button:disabled { opacity: .4; cursor: default; }
  pre { margin: 0; padding: 12px; max-height: 440px; overflow: auto; tab-size: 4; white-space: pre; color: var(--text); font: 12px/1.6 var(--code-font); }
  pre.wrap { white-space: pre-wrap; overflow-wrap: anywhere; }
  p { margin: 0; padding: 8px 12px; font-size: 12px; overflow-wrap: anywhere; }
  code :global(.tok-comment) { color: var(--text-muted); font-style: italic; }
  code :global(.tok-keyword) { color: #e898b2; }
  code :global(.tok-string), code :global(.tok-regexp) { color: #92d4ad; }
  code :global(.tok-number), code :global(.tok-bool), code :global(.tok-propertyName) { color: #e5bc83; }
  code :global(.tok-typeName), code :global(.tok-variableName2), code :global(.tok-definition) { color: #8ecbea; }
  :global(:root[data-color-scheme="light"]) code :global(.tok-keyword) { color: #92204b; }
  :global(:root[data-color-scheme="light"]) code :global(.tok-string),
  :global(:root[data-color-scheme="light"]) code :global(.tok-regexp) { color: #21613c; }
  :global(:root[data-color-scheme="light"]) code :global(.tok-number),
  :global(:root[data-color-scheme="light"]) code :global(.tok-bool),
  :global(:root[data-color-scheme="light"]) code :global(.tok-propertyName) { color: #825115; }
  :global(:root[data-color-scheme="light"]) code :global(.tok-typeName),
  :global(:root[data-color-scheme="light"]) code :global(.tok-variableName2),
  :global(:root[data-color-scheme="light"]) code :global(.tok-definition) { color: #175d82; }
</style>
