<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Download, RefreshCw, SearchCheck, Square, Upload } from "@lucide/svelte";
  import { onDestroy, untrack } from "svelte";
  import { createPushController, type PushPorts, type PushState } from "./git-push";
  import { t, tm, getLocale } from "$lib/i18n.svelte";
  import { appConfirmDialog } from "./app-messages";

  let { projectId, visible, blocked, refreshToken, onBusyChange, onPushed,
    ports = {
      load: (projectId, operationId) => invoke("project_git_push_targets", { projectId, operationId }),
      push: (projectId, expected, operationId) => invoke("project_git_push", {
        projectId, expected, operationId,
        dialog: appConfirmDialog("git.push.dialog", getLocale()),
      }),
      cancel: (operationId) => invoke("cancel_git_read", { operationId }),
      verify: (projectId, expected, operationId) => invoke("project_git_verify_remote", { projectId, expected, operationId }),
      sync: (projectId, expected, operationId) => invoke("project_git_sync_tracking", {
        projectId, expected, operationId,
        dialog: appConfirmDialog("git.sync.dialog", getLocale(), ["new_ref"]),
      }),
    },
  }: {
    projectId: string | null; visible: boolean; blocked: boolean; refreshToken: number;
    onBusyChange: (busy: boolean) => void; onPushed: (projectId: string) => void; ports?: PushPorts;
  } = $props();
  let state = $state<PushState>({ projectId: null, targets: null, selected: -1, branch: "", busy: false, phase: "idle", cancelling: false, error: "", result: null, attempted: null, verification: null, syncResult: null });
  const controller = createPushController({
    load: (id, operationId) => ports.load(id, operationId),
    push: (id, expected, operationId) => ports.push(id, expected, operationId),
    cancel: (operationId) => ports.cancel(operationId),
    verify: (id, expected, operationId) => ports.verify(id, expected, operationId),
    sync: (id, expected, operationId) => ports.sync(id, expected, operationId),
  }, (next) => { state = next; onBusyChange(next.busy); }, (id) => onPushed(id));
  $effect(() => { const id = projectId; untrack(() => controller.setProject(id)); });
  $effect(() => { void refreshToken; untrack(() => controller.invalidate()); });
  onDestroy(() => controller.dispose());
  const cancelLabel = $derived(state.phase === "syncing" ? t("取消跟踪同步") : state.phase === "verifying" ? t("取消远程核对") : t("取消推送操作"));
</script>

{#if visible}
  <section class="push-panel" aria-label={t("Git 推送")} aria-busy={state.busy}>
    <header>
      <strong>{t("推送")}</strong>
      {#if state.busy}<button class="icon" type="button" aria-label={cancelLabel} title={cancelLabel}
        disabled={state.cancelling} onclick={() => controller.cancel()}><Square size={14} /></button>{/if}
      <button type="button" disabled={blocked || state.busy} onclick={() => controller.load()}><RefreshCw size={14} />{t("加载远程")}</button>
    </header>
    {#if state.targets}
      {#if !state.targets.targets.length}<p role="status">{t("未配置远程仓库")}</p>
      {:else}
        <label for="git-push-target">{t("远程目的地")}</label>
        <select id="git-push-target" value={state.selected} disabled={blocked || state.busy}
          onchange={(event) => controller.selectTarget(Number(event.currentTarget.value))}>
          <option value={-1}>{t("选择目的地")}</option>
          {#each state.targets.targets as target, index (`${target.remote}·${target.destination}`)}
            <option value={index}>{target.remote} · {target.destination}</option>
          {/each}
        </select>
        {#if state.selected >= 0}<p class="destination">{state.targets.targets[state.selected]?.destination}</p>{/if}
        <label for="git-push-branch">{t("目标分支")}</label>
        <input id="git-push-branch" value={state.branch} maxlength="512" disabled={blocked || state.busy}
          oninput={(event) => controller.setBranch(event.currentTarget.value)} />
        <p title={state.targets.sourceOid}>{t("源提交")} <code>{state.targets.sourceOid.slice(0, 12)}</code></p>
        <button type="button" disabled={blocked || state.busy || state.selected < 0 || !state.branch.trim()}
          onclick={() => controller.push()}><Upload size={14} />{t("推送")}</button>
      {/if}
    {/if}
    {#if state.busy}<p role="status">{state.cancelling ? t("正在请求取消…") : state.phase === "loading" ? t("正在读取远程配置…") : state.phase === "verifying" ? t("正在核对远程目标…") : state.phase === "syncing" ? t("正在同步跟踪引用…") : t("正在推送…")}</p>{/if}
    {#if state.error}<p role="alert">{tm(state.error)}</p>{/if}
    {#if state.result}<p role={state.result.outcome === "pushed" || state.result.outcome === "upToDate" ? "status" : "alert"}>
      {tm(state.result.detail)}<code title={state.result.sourceOid}>{state.result.sourceOid.slice(0, 12)}</code>
      <span>{state.result.targetRef}</span>
    </p>{/if}
    {#if state.attempted}
      <p class="destination" title={state.attempted.destination}>{state.attempted.remote} · {state.attempted.destination}</p>
      <p>{t("核对分支")} <span>{state.attempted.targetRef}</span></p>
      <p>{t("核对提交")} <code title={state.attempted.sourceOid}>{state.attempted.sourceOid.slice(0, 12)}</code></p>
      <button type="button" disabled={blocked || state.busy} onclick={() => controller.verify()}><SearchCheck size={14} />{t("核对远程结果")}</button>
      <button type="button" disabled={blocked || state.busy} onclick={() => controller.sync()}><Download size={14} />{t("同步跟踪引用")}</button>
    {/if}
    {#if state.syncResult}
      <p role={state.syncResult.outcome === "synced" ? "status" : "alert"}>
        {state.syncResult.outcome === "synced" ? t("本次跟踪引用已同步") : t("同步结果未知，请刷新并核对本地引用")}
        <code title={state.syncResult.oid}>{state.syncResult.oid.slice(0, 12)}</code>
        {#each state.syncResult.references as reference}<span>{reference}</span>{/each}
      </p>
    {/if}
    {#if state.verification}
      <p role="status">
        {state.verification.outcome === "matches" ? t("远程当前公布的目标指向本次推送提交") : state.verification.outcome === "missing" ? t("查询未返回目标分支") : t("远程当前公布的目标指向其他提交")}
        {#if state.verification.remoteOid}<code title={state.verification.remoteOid}>{state.verification.remoteOid.slice(0, 12)}</code>{/if}
      </p>
    {/if}
  </section>
{/if}

<style>
  .push-panel { flex-shrink: 0; display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border-bottom: 1px solid var(--border); font-size: 12px; }
  header { display: flex; align-items: center; gap: 6px; }
  strong { flex: 1; }
  label { font-size: 12px; font-weight: 600; }
  input, select { width: 100%; min-width: 0; box-sizing: border-box; min-height: 28px; padding: 4px 6px; border: 1px solid var(--border); border-radius: 4px; background: var(--page-bg); color: var(--text); font: inherit; }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 28px; padding: 4px 8px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }
  button.icon { width: 28px; padding: 0; }
  button:disabled, input:disabled, select:disabled { opacity: .5; cursor: default; }
  button:hover:not(:disabled) { background: var(--surface-hover); }
  p { margin: 0; padding: 2px 0; color: var(--text-muted); font-size: 12px; overflow-wrap: anywhere; }
  p span { display: block; margin-top: 4px; }
  code { font-family: var(--code-font); margin-left: 4px; }
</style>
