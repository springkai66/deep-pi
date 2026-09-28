<script lang="ts">
  /// 字体选择下拉：原生 <select> 的 <option> 在 Windows 上无法按选项渲染自定义字体，
  /// 这里用触发按钮 + listbox 弹层自绘，每个选项以对应字体族实时预览。
  import { tick } from "svelte";
  import { ChevronDown } from "@lucide/svelte";

  export interface FontOption {
    value: string;
    label: string;
    /// 该选项预览渲染用的 CSS font-family 栈；缺省回落界面默认字体。
    previewFamily?: string;
  }

  let {
    value,
    options,
    ariaLabel,
    onchange,
  }: {
    value: string;
    options: FontOption[];
    ariaLabel: string;
    onchange: (value: string) => void;
  } = $props();

  let open = $state(false);
  let highlighted = $state(0);
  let trigger = $state<HTMLButtonElement | null>(null);
  let listbox = $state<HTMLUListElement | null>(null);
  const listId = `font-list-${Math.random().toString(36).slice(2, 10)}`;
  /// 弹层用 fixed 定位（不受设置页滚动容器的 overflow 裁剪），打开时按触发器位置摆放。
  let listStyle = $state<{ top: string; left: string; minWidth: string; maxWidth: string; maxHeight: string } | null>(null);

  /// 当前选中项；值不在选项里（例如字体已被卸载）时按原始值显示。
  const selected = $derived(options.find((option) => option.value === value) ?? { value, label: value });

  function positionList() {
    const rect = trigger?.getBoundingClientRect();
    if (!rect) return;
    // 优先向下展开；下方空间不足时向上弹，高度随可用空间收缩。
    const below = window.innerHeight - rect.bottom;
    const above = rect.top;
    const openUp = below < 160 && above > below;
    const maxHeight = Math.max(120, Math.min(320, (openUp ? above : below) - 8));
    listStyle = {
      top: openUp ? `${rect.top - maxHeight - 4}px` : `${rect.bottom + 4}px`,
      left: `${rect.left}px`,
      minWidth: `${Math.max(rect.width, 210)}px`,
      maxWidth: "360px",
      maxHeight: `${maxHeight}px`,
    };
  }

  async function openMenu() {
    highlighted = Math.max(0, options.findIndex((option) => option.value === value));
    open = true;
    await tick();
    positionList();
    listbox?.children[highlighted]?.querySelector("button")?.scrollIntoView({ block: "nearest" });
  }

  function closeMenu(refocus = true) {
    open = false;
    listStyle = null;
    if (refocus) trigger?.focus();
  }

  function choose(option: FontOption) {
    onchange(option.value);
    closeMenu();
  }

  function move(delta: number) {
    highlighted = Math.min(options.length - 1, Math.max(0, highlighted + delta));
  }

  function onTriggerKeydown(event: KeyboardEvent) {
    if (!open) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        void openMenu();
      }
      return;
    }
    switch (event.key) {
      case "ArrowDown": event.preventDefault(); move(1); break;
      case "ArrowUp": event.preventDefault(); move(-1); break;
      case "Home": event.preventDefault(); highlighted = 0; break;
      case "End": event.preventDefault(); highlighted = options.length - 1; break;
      case "Enter": case " ": event.preventDefault(); { const option = options[highlighted]; if (option) choose(option); } break;
      case "Escape": event.preventDefault(); closeMenu(); break;
      case "Tab": closeMenu(false); break;
    }
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (!open) return;
    const target = event.target instanceof Node ? event.target : null;
    if (target && !trigger?.contains(target) && !listbox?.contains(target)) closeMenu(false);
  }

  /// 设置页/页面滚动或窗口缩放时直接收起，避免 fixed 弹层与触发器错位；弹层自身滚动除外。
  function onScroll(event: Event) {
    if (event.target === listbox) return;
    if (open) closeMenu(false);
  }

  $effect(() => {
    if (!open) return;
    document.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    return () => {
      document.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
    };
  });
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div class="font-select">
  <button type="button" bind:this={trigger} class="font-trigger" aria-haspopup="listbox" aria-expanded={open}
    aria-controls={listId} aria-label={ariaLabel} onclick={() => (open ? closeMenu() : void openMenu())}
    onkeydown={onTriggerKeydown}>
    <span class="font-preview" style:font-family={selected.previewFamily}>{selected.label}</span>
    <ChevronDown size={14} />
  </button>
  {#if open && listStyle}
    <ul class="font-list" bind:this={listbox} role="listbox" id={listId} aria-label={ariaLabel}
      style:top={listStyle.top} style:left={listStyle.left} style:min-width={listStyle.minWidth}
      style:max-width={listStyle.maxWidth} style:max-height={listStyle.maxHeight}>
      {#each options as option, index (option.value || "default")}
        <li role="presentation" class:highlighted={index === highlighted}>
          <button type="button" role="option" aria-selected={option.value === value} title={option.label}
            style:font-family={option.previewFamily}
            onpointerenter={() => (highlighted = index)} onclick={() => choose(option)}>
            {option.label}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .font-select { position: relative; }
  .font-trigger {
    display: flex; align-items: center; justify-content: space-between; gap: 6px;
    width: 210px; max-width: 100%; height: 32px; padding: 4px 8px;
    border: 1px solid var(--border-strong); border-radius: 4px; color: var(--text); background: var(--surface);
    font: inherit; cursor: pointer; text-align: left;
  }
  .font-trigger:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .font-preview { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .font-list {
    position: fixed; z-index: 60; margin: 0; padding: 4px; list-style: none;
    overflow-y: auto; overscroll-behavior: contain;
    border: 1px solid var(--border-strong); border-radius: 6px;
    background: var(--surface-raised); box-shadow: 0 8px 24px rgb(0 0 0 / 25%);
  }
  .font-list li { padding: 0; }
  .font-list li.highlighted { background: var(--surface-hover); }
  .font-list li button {
    display: block; width: 100%; padding: 5px 8px; border: 0; border-radius: 4px;
    background: transparent; color: var(--text); font-size: 13px; cursor: pointer; text-align: left;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .font-list li button[aria-selected="true"] { color: var(--accent); }
</style>
