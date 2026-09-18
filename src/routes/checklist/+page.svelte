<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { ClipboardList, Plus, Trash2, X } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { applyAppearance } from "$lib/appearance";
  import { t, tm } from "$lib/i18n.svelte";
  import type { AppSettings } from "$lib/settings";

  const QUADRANTS = [
    { id: 1, title: "重要且紧急", subtitle: "现在处理" },
    { id: 2, title: "重要但不紧急", subtitle: "安排时间" },
    { id: 3, title: "紧急但不重要", subtitle: "尽快处理" },
    { id: 4, title: "不重要也不紧急", subtitle: "有空再做" },
  ] as const;
  type Quadrant = (typeof QUADRANTS)[number]["id"];

  type ChecklistItem = {
    id: string;
    text: string;
    quadrant: Quadrant;
    completed: boolean;
    createdAt: number;
  };

  type ResizeDirection = "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";

  let items = $state<ChecklistItem[]>([]);
  let drafts = $state<Record<Quadrant, string>>({ 1: "", 2: "", 3: "", 4: "" });
  let loadError = $state("");
  let busy = $state(false);

  function quadrantItems(quadrant: Quadrant) {
    return items.filter((item) => item.quadrant === quadrant);
  }

  async function loadItems() {
    try {
      items = await invoke<ChecklistItem[]>("list_checklist_items");
      loadError = "";
    } catch (cause) {
      loadError = tm(String(cause));
    }
  }

  async function refresh() {
    try {
      const [settings, nextItems] = await Promise.all([
        invoke<AppSettings>("get_settings"),
        invoke<ChecklistItem[]>("list_checklist_items"),
      ]);
      applyAppearance(settings);
      items = nextItems;
      await getCurrentWindow().setTitle(t("四象限清单"));
      loadError = "";
    } catch (cause) {
      loadError = tm(String(cause));
    }
  }

  async function addItem(quadrant: Quadrant) {
    const text = drafts[quadrant].trim();
    if (!text || busy) return;
    busy = true;
    loadError = "";
    try {
      await invoke("add_checklist_item", { request: { text, quadrant } });
      drafts[quadrant] = "";
      await loadItems();
    } catch (cause) {
      loadError = tm(String(cause));
    } finally {
      busy = false;
    }
  }

  async function toggleItem(id: string) {
    if (busy) return;
    busy = true;
    loadError = "";
    try {
      await invoke("toggle_checklist_item", { request: { id } });
      await loadItems();
    } catch (cause) {
      loadError = tm(String(cause));
    } finally {
      busy = false;
    }
  }

  async function deleteItem(id: string) {
    if (busy) return;
    busy = true;
    loadError = "";
    try {
      await invoke("delete_checklist_item", { request: { id } });
      await loadItems();
    } catch (cause) {
      loadError = tm(String(cause));
    } finally {
      busy = false;
    }
  }

  async function closeChecklist() {
    try { await getCurrentWindow().close(); } catch { /* Window may already be gone. */ }
  }

  function startResize(direction: ResizeDirection) {
    void getCurrentWindow().startResizeDragging(direction).catch(() => { /* Window may already be gone. */ });
  }

  onMount(() => {
    const appWindow = getCurrentWindow();
    void (async () => {
      try {
        await invoke<void>("await_startup");
        await refresh();
      } catch (cause) {
        loadError = tm(String(cause));
      }
    })();
    const focusListener = appWindow.onFocusChanged(({ payload: focused }) => {
      if (focused) void refresh();
    });
    return () => {
      void focusListener.then((dispose) => dispose());
    };
  });
</script>

<svelte:head><title>{t("四象限清单")}</title></svelte:head>

<div class="checklist-window">
  <header class="checklist-titlebar" data-tauri-drag-region>
    <ClipboardList size={15} data-tauri-drag-region aria-hidden="true" />
    <strong data-tauri-drag-region>{t("四象限清单")}</strong>
    <button type="button" class="checklist-close" aria-label={t("关闭四象限清单")} title={t("关闭四象限清单")} onclick={() => void closeChecklist()}>
      <X size={15} aria-hidden="true" />
    </button>
  </header>

  <main class="checklist-body">
    {#if loadError}
      <p class="checklist-error" role="alert">{tm(loadError)}</p>
    {/if}

    <div class="quadrant-grid" aria-label={t("四象限清单")}>
      {#each QUADRANTS as quadrant (quadrant.id)}
        <section class={`quadrant quadrant-${quadrant.id}`} aria-labelledby={`quadrant-title-${quadrant.id}`}>
          <header class="quadrant-header">
            <div class="quadrant-heading">
              <span class="quadrant-marker" aria-hidden="true"></span>
              <div>
                <h2 id={`quadrant-title-${quadrant.id}`}>{t(quadrant.title)}</h2>
                <p>{t(quadrant.subtitle)}</p>
              </div>
            </div>
            <span class="quadrant-count" aria-label={t("{count} 项", { count: quadrantItems(quadrant.id).length })}>{quadrantItems(quadrant.id).length}</span>
          </header>

          <form class="quadrant-form" onsubmit={(event) => { event.preventDefault(); void addItem(quadrant.id); }}>
            <input
              bind:value={drafts[quadrant.id]}
              maxlength="500"
              placeholder={t("添加一项")}
              aria-label={t("在{quadrant}添加任务", { quadrant: quadrant.title })}
              autocomplete="off"
              disabled={busy}
            />
            <button type="submit" class="quadrant-add" aria-label={t("添加清单任务")} title={t("添加清单任务")} disabled={busy || !drafts[quadrant.id].trim()}>
              <Plus size={16} aria-hidden="true" />
            </button>
          </form>

          {#if quadrantItems(quadrant.id).length === 0}
            <p class="quadrant-empty">{t("暂无任务")}</p>
          {:else}
            <ul class="quadrant-items" aria-label={t(quadrant.title)}>
              {#each quadrantItems(quadrant.id) as item (item.id)}
                <li class:completed={item.completed}>
                  <label class="checklist-item-label">
                    <input
                      type="checkbox"
                      checked={item.completed}
                      aria-label={t("完成任务：{text}", { text: item.text })}
                      disabled={busy}
                      onchange={() => void toggleItem(item.id)}
                    />
                    <span>{item.text}</span>
                  </label>
                  <button type="button" class="checklist-delete" aria-label={t("删除清单任务：{text}", { text: item.text })} title={t("删除清单任务")} disabled={busy} onclick={() => void deleteItem(item.id)}>
                    <Trash2 size={14} aria-hidden="true" />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/each}
    </div>
  </main>

  <div class="resize-handle n" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("North"); }}></div>
  <div class="resize-handle s" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("South"); }}></div>
  <div class="resize-handle w" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("West"); }}></div>
  <div class="resize-handle e" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("East"); }}></div>
  <div class="resize-handle nw" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("NorthWest"); }}></div>
  <div class="resize-handle ne" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("NorthEast"); }}></div>
  <div class="resize-handle sw" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("SouthWest"); }}></div>
  <div class="resize-handle se" aria-hidden="true" onpointerdown={(event) => { event.preventDefault(); startResize("SouthEast"); }}></div>
</div>

<style>
  .checklist-window {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 100vw;
    height: 100vh;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--page-bg);
    color: var(--text);
  }

  .checklist-titlebar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    min-height: 34px;
    padding: 4px 6px 4px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    user-select: none;
    cursor: default;
  }

  .checklist-titlebar strong {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .checklist-close,
  .quadrant-add,
  .checklist-delete {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    color: var(--text-muted);
    background: transparent;
    cursor: pointer;
  }

  .checklist-close:hover,
  .quadrant-add:hover:not(:disabled),
  .checklist-delete:hover:not(:disabled) {
    color: var(--text-strong);
    background: var(--surface-hover);
  }

  .checklist-close:disabled,
  .quadrant-add:disabled,
  .checklist-delete:disabled {
    cursor: default;
    opacity: .45;
  }

  .checklist-body {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    overflow: hidden;
  }

  .checklist-error {
    flex-shrink: 0;
    margin: 0 2px;
    overflow-wrap: anywhere;
    color: var(--status-failed);
    font-size: 12px;
  }

  .quadrant-grid {
    display: grid;
    flex: 1;
    min-width: 0;
    min-height: 0;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    grid-template-rows: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  .quadrant {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--border);
    border-top: 3px solid var(--quadrant-color);
    border-radius: 6px;
    background: var(--surface);
  }

  .quadrant-1 { --quadrant-color: var(--status-failed); }
  .quadrant-2 { --quadrant-color: var(--accent); }
  .quadrant-3 { --quadrant-color: #d39a48; }
  .quadrant-4 { --quadrant-color: #6e9bc5; }

  .quadrant-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    flex-shrink: 0;
    padding: 9px 10px 7px;
  }

  .quadrant-heading {
    display: flex;
    min-width: 0;
    align-items: flex-start;
    gap: 7px;
  }

  .quadrant-marker {
    width: 7px;
    height: 7px;
    flex-shrink: 0;
    margin-top: 4px;
    border-radius: 50%;
    background: var(--quadrant-color);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--quadrant-color) 15%, transparent);
  }

  .quadrant-header h2 {
    margin: 0;
    overflow: hidden;
    color: var(--text-strong);
    font-size: 12px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .quadrant-header p {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-size: 10px;
  }

  .quadrant-count {
    display: grid;
    min-width: 20px;
    height: 20px;
    flex-shrink: 0;
    place-items: center;
    border-radius: 10px;
    background: color-mix(in srgb, var(--quadrant-color) 14%, var(--surface-raised));
    color: var(--quadrant-color);
    font-size: 10px;
    font-weight: 700;
  }

  .quadrant-form {
    display: flex;
    flex-shrink: 0;
    gap: 5px;
    padding: 0 8px 8px;
  }

  .quadrant-form input {
    min-width: 0;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: var(--surface-alt);
    color: var(--text);
    font-size: 11px;
  }

  .quadrant-form input:focus {
    border-color: var(--quadrant-color);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--quadrant-color) 20%, transparent);
    outline: none;
  }

  .quadrant-add {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border-strong);
    background: var(--surface-raised);
    color: var(--text);
  }

  .quadrant-items {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0 8px 8px;
    overflow: auto;
    list-style: none;
  }

  .quadrant-items li {
    display: flex;
    min-width: 0;
    align-items: flex-start;
    gap: 4px;
    padding: 5px 3px 5px 4px;
    border-bottom: 1px solid var(--border);
  }

  .checklist-item-label {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: flex-start;
    gap: 7px;
    color: var(--text);
    cursor: pointer;
    font-size: 12px;
    line-height: 1.35;
  }

  .checklist-item-label input {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    margin: 1px 0 0;
    accent-color: var(--quadrant-color);
  }

  .checklist-item-label span {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .quadrant-items li.completed .checklist-item-label span {
    color: var(--text-muted);
    text-decoration: line-through;
  }

  .checklist-delete {
    width: 23px;
    height: 23px;
  }

  .quadrant-empty {
    margin: 5px 10px 10px;
    color: var(--text-muted);
    font-size: 11px;
  }

  .resize-handle {
    position: absolute;
    z-index: 40;
    touch-action: none;
  }

  .resize-handle.n { top: 0; left: 12px; right: 12px; height: 5px; cursor: ns-resize; }
  .resize-handle.s { bottom: 0; left: 12px; right: 12px; height: 5px; cursor: ns-resize; }
  .resize-handle.w { left: 0; top: 12px; bottom: 12px; width: 5px; cursor: ew-resize; }
  .resize-handle.e { right: 0; top: 12px; bottom: 12px; width: 5px; cursor: ew-resize; }
  .resize-handle.nw { left: 0; top: 0; width: 12px; height: 12px; cursor: nwse-resize; }
  .resize-handle.ne { right: 0; top: 0; width: 12px; height: 12px; cursor: nesw-resize; }
  .resize-handle.sw { left: 0; bottom: 0; width: 12px; height: 12px; cursor: nesw-resize; }
  .resize-handle.se { right: 0; bottom: 0; width: 12px; height: 12px; cursor: nwse-resize; }

  @media (max-width: 620px) {
    .quadrant-grid {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: none;
      overflow: auto;
    }

    .quadrant { min-height: 180px; }
  }
</style>
