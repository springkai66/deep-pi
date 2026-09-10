<script lang="ts">
  import type { Snippet } from "svelte";
  let { title, children }: { title: string; children: Snippet } = $props();
  let expanded = $state(false);
</script>

<details bind:open={expanded}>
  <summary>{title}</summary>
  {#if expanded}{@render children()}{/if}
</details>

<style>
  details { margin: 8px 0; border-left: 2px solid var(--border-strong); padding-left: 12px; }
  summary { cursor: pointer; font-size: 12px; color: var(--text-muted); overflow-wrap: anywhere; }
  details[open] summary { margin-bottom: 8px; }
</style>
