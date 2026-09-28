<script lang="ts">
  import { untrack } from "svelte";
  import type { Snippet } from "svelte";
  let { title, children, variant, defaultOpen = false, resetKey }: { title: string; children: Snippet; variant?: "thinking"; defaultOpen?: boolean; resetKey?: string } = $props();
  /// 用户可手动开合；切换显示级别时重新采用新模式的默认展开状态。
  let expanded = $state(untrack(() => defaultOpen));
  $effect(() => { void resetKey; expanded = defaultOpen; });
</script>

<details bind:open={expanded} class:thinking={variant === "thinking"}>
  <summary>{title}</summary>
  {#if expanded}{@render children()}{/if}
</details>

<style>
  details { margin: 8px 0; border-left: 2px solid var(--border-strong); padding-left: 12px; }
  summary { cursor: pointer; font-size: 12px; color: var(--text-muted); overflow-wrap: anywhere; }
  details[open] summary { margin-bottom: 8px; }
  /* pi TUI 风格：思考内容斜体、暗淡色（默认仍折叠，展开后渲染）。 */
  details.thinking { border-left-color: transparent; font-style: italic; color: var(--text-muted); }
</style>
