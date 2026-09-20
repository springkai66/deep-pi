<script lang="ts">
  import { untrack } from "svelte";
  import type { Snippet } from "svelte";
  let { title, children, variant, defaultOpen = false }: { title: string; children: Snippet; variant?: "thinking"; defaultOpen?: boolean } = $props();
  /// 折叠状态只在挂载时取 defaultOpen 初值（「详细」档默认展开）；之后由用户手动开合，设置变更不回写已渲染的消息。
  let expanded = $state(untrack(() => defaultOpen));
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
