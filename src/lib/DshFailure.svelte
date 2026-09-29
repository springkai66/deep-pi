<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { RefreshCw, Trash2, Download } from "@lucide/svelte";
  import { t } from "$lib/i18n.svelte";

  interface DshDiagnosis {
    hasFailure: boolean;
    timestamp: number | null;
    lines: string[];
    culprits: string[];
    pending: string[];
    kind: string | null;
  }
  interface DshPlugin { name: string; official: boolean }
  interface Props {
    dshRunning: boolean;
    busyRuntime: string | null;
    confirm: (title: string, message: string, label?: string) => Promise<boolean>;
    onError: (error: unknown) => void;
    onUpdateDshRuntime: () => void;
    onBusyChange: (busy: boolean) => void;
  }
  let { dshRunning, busyRuntime, confirm, onError, onUpdateDshRuntime, onBusyChange }: Props = $props();
  let diagnosis = $state<DshDiagnosis | null>(null);
  let plugins = $state<DshPlugin[]>([]);
  let loading = $state(false);
  let repairing = $state(false);
  let notice = $state("");
  const busy = $derived(loading || repairing);
  $effect(() => onBusyChange(busy));

  async function load() {
    loading = true;
    try {
      const [result, configured] = await Promise.all([
        invoke<DshDiagnosis>("dsh_diagnose"),
        invoke<DshPlugin[]>("dsh_plugins"),
      ]);
      diagnosis = result;
      plugins = configured;
    } catch (error) {
      diagnosis = null;
      onError(error);
    } finally {
      loading = false;
    }
  }
  onMount(() => { void load(); });

  async function removePlugin(plugin: DshPlugin) {
    if (busy || dshRunning || busyRuntime !== null || plugin.official || !diagnosis?.hasFailure || !diagnosis.culprits.includes(plugin.name)) return;
    if (!(await confirm(
      t("移除插件"),
      t("从 web 配置中移除插件 “{package}” 并删除其文件吗？操作前会创建可回滚的快照，其余插件不受影响。", { package: plugin.name }),
      t("移除插件"),
    ))) return;
    repairing = true;
    try {
      await invoke("dsh_repair", { request: { package: plugin.name } });
      notice = t("插件已移除。点击“启动”重新拉起 DSH。");
      const [result, configured] = await Promise.all([
        invoke<DshDiagnosis>("dsh_diagnose"), invoke<DshPlugin[]>("dsh_plugins"),
      ]);
      diagnosis = result;
      plugins = configured;
    } catch (error) {
      onError(error);
    } finally {
      repairing = false;
    }
  }
</script>

<section class="failure-diagnosis" aria-label={t("DSH 插件兼容性")}>
  <div class="failure-heading">
    <h2>{t("DSH 插件兼容性")}</h2>
    <button type="button" title={t("诊断")} aria-label={t("诊断")} disabled={busy} onclick={() => void load()}><RefreshCw size={14} /></button>
  </div>
  {#if loading}<p role="status">{t("检查中")}</p>{/if}
  {#if !loading && diagnosis?.hasFailure}
    <p>{t("最近一次启动失败 · {time}", { time: diagnosis.timestamp ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(diagnosis.timestamp)) : t("时间未知") })}</p>
    {#if diagnosis.kind === "activation-pending" && diagnosis.pending.length}
      <p>{t("有 {count} 个插件等不到服务，未能激活", { count: diagnosis.pending.length })}</p>
      <button type="button" class="quiet-button" disabled={busy || dshRunning || busyRuntime !== null} onclick={onUpdateDshRuntime}><Download size={14} />{t("更新 DSH 运行时")}</button>
    {/if}
  {:else if !loading}
    <p>{t("未检测到启动失败记录。插件兼容性未经验证。")}</p>
  {/if}
  {#if !loading && plugins.length === 0}<p>{t("未找到已配置的 DSH 插件")}</p>{/if}
  {#if plugins.length}
    <ul>
      {#each plugins as plugin (plugin.name)}
        {@const suspected = diagnosis?.hasFailure && diagnosis.culprits.includes(plugin.name) && !plugin.official}
        <li>
          <span class="plugin-name">{plugin.name}</span>
          <span class:suspected>{suspected ? t("疑似不兼容") : t("未验证")}</span>
          {#if suspected}
            <button type="button" class="quiet-button" disabled={busy || dshRunning || busyRuntime !== null} onclick={() => void removePlugin(plugin)}><Trash2 size={14} />{t("移除插件")}</button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if diagnosis?.hasFailure && diagnosis.lines.length}
    <details><summary>{t("DSH 启动失败日志")}</summary><pre>{diagnosis.lines.join("\n")}</pre></details>
  {/if}
  {#if notice}<p role="status">{notice}</p>{/if}
</section>

<style>
  .failure-diagnosis { width: min(720px, 100%); max-width: 100%; max-height: min(52vh, 440px); overflow: auto; padding: 12px 14px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface); color: var(--text); font-size: 12px; }
  .failure-heading { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  h2 { font-size: 14px; margin: 0; }
  .failure-heading button { display: grid; place-items: center; width: 28px; height: 28px; border: 0; background: transparent; color: var(--text); }
  ul { margin: 8px 0; padding: 0; list-style: none; }
  li { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 12px; padding: 7px 0; border-top: 1px solid var(--border); }
  .plugin-name { flex: 1 1 160px; min-width: 0; overflow-wrap: anywhere; font-family: var(--code-font); }
  .suspected { color: #d46b61; }
  .quiet-button { display: inline-flex; align-items: center; gap: 5px; flex-shrink: 0; }
  p { margin: 8px 0; overflow-wrap: anywhere; color: var(--text-muted); }
  details { margin-top: 8px; }
  pre { max-height: 180px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
