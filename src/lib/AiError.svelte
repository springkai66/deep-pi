<script lang="ts">
  import { describeAiError } from "./ai-error";
  import { t } from "./i18n.svelte";

  let { raw, compact = false }: { raw: string; compact?: boolean } = $props();
  const info = $derived(describeAiError(raw));
</script>

<div class:compact class="ai-error">
  <span class="ai-error-label">{info.code} · {info.description}</span>
  <details>
    <summary>{t("错误详情")}</summary>
    <pre>{raw}</pre>
  </details>
</div>

<style>
  .ai-error { min-width: 0; font-size: 12px; line-height: 1.5; overflow-wrap: anywhere; }
  .ai-error-label { font-weight: 500; }
  details { margin-top: 5px; }
  summary { width: fit-content; cursor: pointer; color: var(--text-muted); }
  pre { margin: 6px 0 0; max-height: 240px; overflow: auto; padding: 8px; border-radius: 4px; background: var(--surface-hover); color: var(--text); white-space: pre-wrap; overflow-wrap: anywhere; font: 11px/1.5 var(--code-font); }
  .compact { display: inline-flex; align-items: baseline; flex-wrap: wrap; gap: 4px 8px; }
  .compact details { margin: 0; }
  .compact pre { position: relative; max-width: min(600px, 80vw); }
</style>
