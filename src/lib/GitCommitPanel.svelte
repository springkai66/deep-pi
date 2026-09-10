<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Eye, GitCommitHorizontal } from "@lucide/svelte";
  import { onDestroy, untrack } from "svelte";
  import { createCommitController, validCommitMessage, type CommitPorts, type CommitState } from "./git-commit";

  let { projectId, visible, blocked, stagedCount, refreshToken, onBusyChange, onCommitted,
    ports = {
      prepare: (projectId, operationId) => invoke("project_git_prepare_commit", { projectId, operationId }),
      commit: (projectId, expected, message, operationId) => invoke("project_git_commit", { projectId, expected, message, operationId }),
    },
  }: {
    projectId: string | null; visible: boolean; blocked: boolean; stagedCount: number; refreshToken: number;
    onBusyChange: (busy: boolean) => void; onCommitted: (projectId: string) => void; ports?: CommitPorts;
  } = $props();
  let state = $state<CommitState>({ projectId: null, message: "", preview: null, busy: false, phase: "idle", error: "", result: null });
  const controller = createCommitController({
    prepare: (id, operationId) => ports.prepare(id, operationId),
    commit: (id, expected, message, operationId) => ports.commit(id, expected, message, operationId),
  }, (next) => { state = next; onBusyChange(next.busy); }, (id) => onCommitted(id));
  $effect(() => {
    const id = projectId;
    untrack(() => controller.setProject(id));
  });
  $effect(() => { void refreshToken; untrack(() => controller.invalidate()); });
  onDestroy(() => controller.dispose());
</script>

{#if visible}
  <section class="commit-panel" aria-label="Git 提交" aria-busy={state.busy}>
    <label for="git-commit-message">提交说明</label>
    <textarea id="git-commit-message" rows="3" maxlength="65536" value={state.message}
      disabled={state.busy || blocked} oninput={(event) => controller.setMessage(event.currentTarget.value)}></textarea>
    <div class="commit-actions">
      <button type="button" disabled={state.busy || blocked || !stagedCount}
        onclick={() => controller.prepare()}><Eye size={14} />审阅暂存</button>
      <button type="button" disabled={state.busy || blocked || !state.preview || !validCommitMessage(state.message)}
        onclick={() => controller.submit()}><GitCommitHorizontal size={14} />提交</button>
    </div>
    {#if state.busy}<p role="status">{state.phase === "preparing" ? "正在准备提交范围…" : "正在提交…"}</p>{/if}
    {#if state.error}<p role="alert">{state.error}</p>{/if}
    {#if state.preview}
      <div class="commit-scope">
        <strong>{state.preview.paths.length} 个暂存路径</strong>
        <span title={state.preview.reference}>{state.preview.reference === "HEAD" ? "分离 HEAD" : state.preview.reference.replace(/^refs\/heads\//, "")}</span>
        <span title={state.preview.author}>{state.preview.author}</span>
        <span>{state.preview.sign ? "签名提交" : "未启用签名"}</span>
        <ul aria-label="本次提交范围">
          {#each state.preview.paths as path (path)}<li title={path}>{path}</li>{/each}
        </ul>
      </div>
    {/if}
    {#if state.result}
      <p role={state.result.outcome === "committed" ? "status" : "alert"}>
        {state.result.outcome === "committed" ? "已提交" : state.result.outcome === "notCommitted" ? "未提交" : "提交结果待核实"}
        <code title={state.result.oid}>{state.result.oid.slice(0, 12)}</code>
        {#if state.result.detail}<span>{state.result.detail}</span>{/if}
      </p>
    {/if}
  </section>
{/if}

<style>
  .commit-panel { flex-shrink: 0; display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border-bottom: 1px solid var(--border); font-size: 12px; }
  label { font-size: 12px; font-weight: 600; }
  textarea { display: block; width: 100%; box-sizing: border-box; resize: vertical; min-height: 64px; max-height: 160px; border: 1px solid var(--border); border-radius: 4px; background: var(--page-bg); color: var(--text); padding: 6px; font: inherit; line-height: 1.5; }
  .commit-actions { display: flex; flex-wrap: wrap; gap: 6px; }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 28px; padding: 4px 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }
  button:disabled, textarea:disabled { opacity: .5; cursor: default; }
  button:hover:not(:disabled) { background: var(--surface-hover); }
  p { margin: 0; padding: 4px 0; font-size: 12px; color: var(--text-muted); overflow-wrap: anywhere; }
  p span { display: block; margin-top: 4px; }
  .commit-scope { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .commit-scope span { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  ul { list-style: none; padding: 0; margin: 2px 0; max-height: 110px; overflow: auto; }
  li { padding: 2px 0; overflow-wrap: anywhere; }
  code { font-family: var(--code-font); }
</style>
