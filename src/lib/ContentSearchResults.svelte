<script lang="ts">
  import { untrack } from "svelte";
  import { FileText } from "@lucide/svelte";
  import { groupSearchMatches, type SearchMatch } from "./search";

  let { matches, onOpen, onClear, focusToken }: {
    matches: SearchMatch[];
    onOpen: (path: string, line: number, column: number) => void;
    onClear: () => void;
    focusToken: number;
  } = $props();
  const groups = $derived(groupSearchMatches(matches));
  const ordered = $derived(groups.flatMap((group) => group.matches));
  const buttons = new Map<string, HTMLButtonElement>();
  let activeKey = $state("");
  function key(match: SearchMatch) { return `${match.path}:${match.line}:${match.column}`; }
  function remember(node: HTMLButtonElement, id: string) {
    buttons.set(id, node);
    return { destroy: () => buttons.delete(id) };
  }
  function focus(index: number) {
    const match = ordered[index];
    if (!match) return;
    activeKey = key(match);
    buttons.get(activeKey)?.focus();
  }
  $effect(() => {
    if (focusToken > 0) untrack(() => focus(0));
  });
  function keyboard(event: KeyboardEvent, match: SearchMatch) {
    if (event.isComposing || event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Escape") { event.preventDefault(); onClear(); return; }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const index = ordered.findIndex((item) => key(item) === key(match));
    const next = event.key === "Home" ? 0 : event.key === "End" ? ordered.length - 1
      : Math.max(0, Math.min(ordered.length - 1, index + (event.key === "ArrowDown" ? 1 : -1)));
    focus(next);
  }
</script>

<div class="content-results" aria-label="内容搜索结果">
  {#each groups as group (group.path)}
    <section aria-label={group.path}>
      <h3 title={group.path}><FileText size={13} /><span>{group.path}</span><small>{group.matches.length}</small></h3>
      {#each group.matches as match (key(match))}
        <button type="button" use:remember={key(match)} class:active={activeKey === key(match)}
          title={`${match.path}:${match.line}:${match.column}`}
          aria-label={`${match.path}:${match.line}:${match.column} ${match.text}`}
          onfocus={() => { activeKey = key(match); }}
          onkeydown={(event) => keyboard(event, match)}
          onclick={() => onOpen(match.path, match.line, match.column)}>
          <span class="location">{match.line}:{match.column}</span><span class="match-text">{match.text}</span>
        </button>
      {/each}
    </section>
  {/each}
</div>

<style>
  .content-results { flex: 1; min-height: 0; overflow: auto; }
  section { border-bottom: 1px solid var(--border); }
  h3 { display: flex; align-items: center; gap: 6px; margin: 0; padding: 8px 10px 4px; font-size: 11px; font-weight: 600; color: var(--text); }
  h3 span { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  h3 small { color: var(--text-muted); font-size: 10px; font-weight: 400; }
  h3 :global(svg) { flex-shrink: 0; }
  button { display: flex; gap: 8px; align-items: center; width: 100%; height: 28px; padding: 0 10px 0 28px; border: 0; background: transparent; color: var(--text); text-align: left; cursor: pointer; }
  button:hover, button.active { background: var(--surface-hover); }
  .location { flex-shrink: 0; min-width: 36px; color: var(--text-muted); font: 10px var(--code-font); }
  .match-text { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: pre; font: 11px var(--code-font); }
</style>
