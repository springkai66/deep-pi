<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, Save, X } from "@lucide/svelte";
  import type { ExternalEditor } from "./settings";
  import { t, tm } from "$lib/i18n.svelte";

  let { editor, onSaved }: { editor: ExternalEditor | null; onSaved: (editor: ExternalEditor | null) => void } = $props();
  let kind = $state<ExternalEditor["kind"]>("vscode");
  let executable = $state("");
  let busy = $state(false);
  let error = $state("");
  let status = $state("");
  const dirty = $derived(kind !== (editor?.kind ?? "vscode") || executable !== (editor?.executable ?? ""));
  $effect(() => { kind = editor?.kind ?? "vscode"; executable = editor?.executable ?? ""; });

  async function browse() {
    if (busy) return;
    busy = true;
    error = status = "";
    try {
      if (!isTauri()) throw new Error(t("请选择桌面应用中的编辑器路径"));
      const path = await open({ directory: false, multiple: false, title: t("选择外部编辑器"), filters: [{ name: "Executable", extensions: ["exe"] }] });
      if (typeof path === "string") executable = path;
    } catch (cause) { error = tm(String(cause)); }
    finally { busy = false; }
  }

  async function save(value: ExternalEditor | null) {
    if (busy) return;
    busy = true;
    error = status = "";
    try {
      await invoke("save_external_editor", { editor: value });
      onSaved(value);
      status = t("已保存");
    } catch (cause) { error = tm(String(cause)); }
    finally { busy = false; }
  }
</script>

<section class="editor-settings" aria-labelledby="external-editor-heading">
  <h3 id="external-editor-heading">{t("外部编辑器")}</h3>
  <form onsubmit={(event) => { event.preventDefault(); void save({ kind, executable }); }}>
    <label for="editor-kind">{t("编辑器")}</label>
    <select id="editor-kind" bind:value={kind} disabled={busy} onchange={() => { error = status = ""; }}>
      <option value="vscode">Visual Studio Code / Insiders</option>
      <option value="notepadPlusPlus">Notepad++</option>
    </select>
    <label for="editor-executable">{t("可执行文件")}</label>
    <div class="path-control">
      <input id="editor-executable" bind:value={executable} disabled={busy} maxlength="4096" spellcheck="false" oninput={() => { error = status = ""; }} />
      <button type="button" title={t("选择编辑器文件")} aria-label={t("选择编辑器文件")} disabled={busy} onclick={() => void browse()}><FolderOpen size={16} /></button>
    </div>
    <div class="editor-actions">
      {#if status}<span role="status">{status}</span>{/if}
      <button type="submit" title={t("保存外部编辑器")} aria-label={t("保存外部编辑器")} disabled={busy || !dirty || !executable.trim()}><Save size={16} /></button>
      <button type="button" title={t("清除外部编辑器配置")} aria-label={t("清除外部编辑器配置")} disabled={busy || !editor} onclick={() => void save(null)}><X size={16} /></button>
    </div>
  </form>
  {#if error}<p role="alert">{tm(error)}</p>{/if}
</section>

<style>
  .editor-settings { margin-bottom: 24px; }
  h3 { margin: 0; padding: 12px 2px; font-size: 13px; font-weight: 600; color: var(--text-muted); }
  form { display: grid; grid-template-columns: 110px minmax(0, 1fr); align-items: center; gap: 12px; padding: 4px; }
  label { font-size: 12px; color: var(--text); }
  input, select { width: 100%; min-width: 0; height: 32px; padding: 4px 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; font-size: 12px; }
  .path-control { display: flex; gap: 6px; min-width: 0; }
  button { width: 30px; height: 30px; padding: 0; flex-shrink: 0; display: grid; place-items: center; border: 1px solid var(--border); border-radius: 4px; color: var(--text); background: var(--surface); cursor: pointer; }
  button:hover:not(:disabled) { background: var(--surface-hover); }
  button:disabled { opacity: .4; cursor: default; }
  .editor-actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; align-items: center; gap: 8px; }
  .editor-actions span { color: var(--text-muted); font-size: 12px; }
  p { color: var(--text); border-left: 2px solid #bd5147; padding-left: 8px; font-size: 12px; overflow-wrap: anywhere; }
  @media (max-width: 620px) { form { grid-template-columns: minmax(0, 1fr); gap: 6px; } }
</style>
