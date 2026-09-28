<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ExternalLink, FileText, RefreshCw, X } from "@lucide/svelte";
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { sourceOffset } from "./search";
  import type { FilePreview } from "./files";
  import { t, tm } from "$lib/i18n.svelte";

  let { projectId, path, line, column, editorConfigured, refreshToken: externalRefreshToken, onConfigureEditor, onClose }: {
    projectId: string; path: string; line?: number; column?: number;
    editorConfigured: boolean; onConfigureEditor: () => void; onClose: () => void;
    refreshToken: number;
  } = $props();
  let code = $state<HTMLElement>();
  let viewport = $state<HTMLDivElement>();
  let preview = $state<FilePreview | null>(null);
  let loading = $state(false);
  let error = $state("");
  let generation = 0;
  let refreshToken = $state(0);
  let launching = $state(false);
  let editorNotice = $state("");
  let appliedLocation = "";
  let loadedFile = "";
  $effect(() => {
    const id = projectId;
    const relativePath = path;
    void refreshToken;
    void externalRefreshToken;
    const current = ++generation;
    const key = `${id}:${relativePath}`;
    const scroll = untrack(() => loadedFile === key ? { top: viewport?.scrollTop ?? 0, left: viewport?.scrollLeft ?? 0 } : null);
    loadedFile = key;
    preview = null;
    editorNotice = "";
    error = "";
    loading = true;
    void invoke<FilePreview>("read_project_file", { projectId: id, relativePath })
      .then(async (result) => {
        if (current !== generation) return;
        preview = result;
        loading = false;
        if (scroll) {
          await tick();
          if (current === generation && viewport) {
            viewport.scrollTop = scroll.top;
            viewport.scrollLeft = scroll.left;
          }
        }
      })
      .catch((cause) => { if (current === generation) error = tm(String(cause)); })
      .finally(() => { if (current === generation) loading = false; });
    return () => { generation++; };
  });
  onDestroy(() => { generation++; });
  onMount(() => {
    const refresh = () => { if (!loading) refreshToken++; };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });
  async function openEditor() {
    if (!editorConfigured) { onConfigureEditor(); return; }
    if (launching) return;
    const current = generation;
    launching = true;
    try {
      await invoke("open_project_in_editor", { projectId, relativePath: path, line: line ?? null, column: column ?? null });
      if (current === generation) editorNotice = t("已发送到外部编辑器");
    } catch (cause) { if (current === generation) editorNotice = tm(String(cause)); }
    finally { launching = false; }
  }
  $effect(() => {
    const content = preview?.content;
    const row = line;
    const col = column ?? 1;
    const location = `${projectId}:${path}:${row}:${col}`;
    if (content === undefined || !row || appliedLocation === location) return;
    let alive = true;
    void tick().then(() => {
      if (!alive || !code?.firstChild || !viewport) return;
      appliedLocation = location;
      const offset = sourceOffset(content, row, col);
      const range = document.createRange();
      range.setStart(code.firstChild, offset);
      range.setEnd(code.firstChild, Math.min(content.length, offset + (content.codePointAt(offset)! > 0xffff ? 2 : 1)));
      const bounds = range.getBoundingClientRect();
      const container = viewport.getBoundingClientRect();
      viewport.scrollTop += bounds.top - container.top - viewport.clientHeight / 3;
      viewport.scrollLeft += bounds.left - container.left - viewport.clientWidth / 3;
      window.getSelection()?.removeAllRanges();
      window.getSelection()?.addRange(range);
    });
    return () => { alive = false; };
  });
</script>

<section class="file-preview" aria-label={t("文件预览")}>
  <header>
    <FileText size={16} />
    <strong title={path}>{path}</strong>
    <span>{t("只读")}</span>
    <button type="button" disabled={launching} title={editorConfigured ? t("在外部编辑器中打开") : t("配置外部编辑器")}
      aria-label={editorConfigured ? t("在外部编辑器中打开") : t("配置外部编辑器")} onclick={() => void openEditor()}><ExternalLink size={15} /></button>
    <button type="button" disabled={loading} title={t("重新读取")} aria-label={t("重新读取文件")} onclick={() => { refreshToken++; }}><RefreshCw size={15} /></button>
    <button type="button" title={t("关闭预览")} aria-label={t("关闭文件预览")} onclick={onClose}><X size={16} /></button>
  </header>
  {#if editorNotice}<p role="status">{tm(editorNotice)}</p>{/if}
  {#if loading}
    <p role="status">{t("正在读取文件…")}</p>
  {:else if error}
    <p role="alert">{tm(error)}</p>
  {:else if preview}
    <div class="preview-code" bind:this={viewport} tabindex="0" role="textbox" aria-readonly="true" aria-multiline="true" aria-label={path}>
      <pre><code bind:this={code}>{preview.content}</code></pre>
    </div>
    <footer>UTF-8 · {t("{size} 字节", { size: preview.size.toLocaleString() })}{#if line} · {line}:{column ?? 1}{/if}</footer>
  {/if}
</section>

<style>
  .file-preview { display: flex; flex-direction: column; position: absolute; inset: 0; z-index: 2; min-width: 0; background: var(--page-bg); }
  header { display: flex; align-items: center; gap: 8px; min-height: 40px; padding: 4px 12px; border-bottom: 1px solid var(--border); background: var(--surface); }
  header :global(svg) { flex-shrink: 0; }
  strong { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  header span, footer { color: var(--text-muted); font-size: 11px; }
  button { display: grid; place-items: center; flex-shrink: 0; width: 28px; height: 28px; padding: 0; border: 0; border-radius: 4px; background: transparent; color: var(--text); cursor: pointer; }
  button:hover { background: var(--surface-hover); }
  button:disabled { opacity: .4; cursor: default; }
  .preview-code { flex: 1; min-height: 0; overflow: auto; }
  pre { margin: 0; padding: 16px; font: 13px/1.6 var(--code-font); tab-size: 4; }
  code { font: inherit; }
  p { padding: 16px; color: var(--text-muted); overflow-wrap: anywhere; }
  footer { padding: 6px 12px; border-top: 1px solid var(--border); }
</style>
