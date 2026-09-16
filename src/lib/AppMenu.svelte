<script lang="ts">
  import { tick } from "svelte";
  import { t } from "$lib/i18n.svelte";

  export interface MenuItem {
    label: string;
    shortcut?: string;
    shortcutAria?: string;
    disabled?: boolean;
    action: () => void;
  }
  interface Props {
    menus: { label: string; items: MenuItem[] }[];
    onOpenChange: (open: boolean) => void;
  }
  let { menus, onOpenChange }: Props = $props();
  let openIndex = $state<number | null>(null);
  let root: HTMLElement;
  let popup = $state<HTMLDivElement>();
  let anchor: HTMLButtonElement | null = null;
  let left = $state(8);

  function close(restoreFocus = false) {
    if (openIndex === null) return;
    openIndex = null;
    onOpenChange(false);
    if (restoreFocus) anchor?.focus();
  }

  async function show(index: number, button: HTMLButtonElement) {
    if (openIndex === index) { close(); return; }
    anchor = button;
    left = Math.max(8, Math.min(button.getBoundingClientRect().left, window.innerWidth - 256));
    openIndex = index;
    onOpenChange(true);
    await tick();
    popup?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }

  function keydown(event: KeyboardEvent) {
    if (event.isComposing || openIndex === null) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const buttons = Array.from(popup?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []);
      if (!buttons.length) return;
      event.preventDefault();
      const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      buttons[(current + (event.key === "ArrowDown" ? 1 : buttons.length - 1)) % buttons.length]?.focus();
    } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      // 只有一个菜单项时没有可切换的目标（show() 的同索引分支会把菜单关掉）。
      if (menus.length < 2) return;
      event.preventDefault();
      const next = (openIndex + (event.key === "ArrowRight" ? 1 : menus.length - 1)) % menus.length;
      const button = root.querySelectorAll<HTMLButtonElement>(".menu-trigger")[next];
      if (button) void show(next, button);
    }
  }
</script>

<svelte:window
  onpointerdown={(event) => { if (!root?.contains(event.target as Node)) close(); }}
  onfocusin={(event) => { if (!root?.contains(event.target as Node)) close(); }}
  onblur={() => close()}
  onresize={() => close()}
/>

<nav class="app-menu" aria-label={t("应用菜单")} bind:this={root}>
  {#each menus as menu, index}
    <button class="menu-trigger" class:active={openIndex === index}
      type="button" aria-haspopup="menu" aria-expanded={openIndex === index}
      onclick={(event) => void show(index, event.currentTarget)}
      onkeydown={(event) => {
        if (event.key === "ArrowDown" && !event.isComposing) {
          event.preventDefault();
          void show(index, event.currentTarget);
        }
      }}>{menu.label}</button>
  {/each}
  {#if openIndex !== null}
    <div class="menu-popup" role="menu" tabindex="-1" aria-label={menus[openIndex].label}
      style:left={`${left}px`} bind:this={popup} onkeydown={keydown}>
      {#each menus[openIndex].items as item}
        <button type="button" role="menuitem" disabled={item.disabled} aria-keyshortcuts={item.shortcutAria}
          onclick={() => { close(true); item.action(); }}>
          <span>{item.label}</span>
          {#if item.shortcut}<kbd>{item.shortcut}</kbd>{/if}
        </button>
      {/each}
    </div>
  {/if}
</nav>

<style>
  .app-menu { display: flex; align-items: center; min-width: 0; }
  .menu-trigger { padding: 6px 8px; border: 0; border-radius: 4px; background: transparent; color: var(--text); cursor: pointer; white-space: nowrap; }
  .menu-trigger:hover, .menu-trigger.active { background: var(--surface-hover); }
  .menu-popup { position: fixed; top: 43px; z-index: 30; width: 248px; max-width: calc(100vw - 16px); max-height: calc(100vh - 60px); overflow: auto; padding: 4px; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 6px; box-shadow: 0 6px 24px #0004; }
  .menu-popup button { display: flex; width: 100%; gap: 12px; justify-content: space-between; align-items: center; padding: 8px; border: 0; border-radius: 3px; color: var(--text); background: transparent; text-align: left; cursor: pointer; }
  .menu-popup button:hover, .menu-popup button:focus-visible { background: var(--surface-hover); }
  .menu-popup button:disabled { opacity: .45; cursor: default; }
  kbd { font: 11px var(--code-font); color: var(--text-muted); white-space: nowrap; }
</style>
