<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { RefreshCw, RotateCcw, Square, Wrench } from "@lucide/svelte";
  import { getLocale, t, tm } from "$lib/i18n.svelte";

  export interface DshDiagnosis {
    hasFailure: boolean;
    timestamp: number | null;
    error: string | null;
    lines: string[];
    culprit: string | null;
    /** 等待服务而未能激活的插件（通常是运行时版本不匹配，不宜直接移除）。 */
    pending: string[];
    /** import-failure | activation-pending | unknown | null */
    kind: string | null;
  }

  export interface DshRepairReport {
    package: string;
    removedFromDependencies: boolean;
    removedFromBundles: boolean;
    nodeModulesRemoved: boolean;
  }
  interface Props {
    onUpdateDshRuntime?: () => void;
    dshRunning: boolean;
    restartBusy: string | null;
    busyRuntime: string | null;
    onRestartDsh: () => void;
    confirm: (title: string, message: string, confirmLabel?: string) => Promise<boolean>;
    onError: (error: unknown) => void;
    onBusyChange?: (busy: boolean) => void;
    invokeCommand?: typeof nativeInvoke;
  }

  let { dshRunning, restartBusy, busyRuntime, onRestartDsh, onUpdateDshRuntime, confirm, onError, onBusyChange, invokeCommand = nativeInvoke }: Props = $props();
  const invoke = <T,>(command: string, args?: Parameters<typeof nativeInvoke>[1]) => invokeCommand<T>(command, args);


  /// “未激活”型失败的正解是让运行时版本与插件期望对齐，而不是移除插件。
  function updateDshRuntime() {
    if (!onUpdateDshRuntime) {
      onError(t("请在“运行时与更新”里更新 DSH 运行时。"));
      return;
    }
    onUpdateDshRuntime();
  }
  let diagnosis = $state<DshDiagnosis | null>(null);
  let diagnosing = $state(false);
  let repairing = $state(false);
  let stopping = $state(false);
  let repairMessage = $state("");
  let statusMessage = $state("");
  let inspected = $state(false);

  const busy = $derived(diagnosing || repairing || stopping);
  $effect(() => onBusyChange?.(busy));

  async function runDiagnosis() {
    diagnosing = true;
    statusMessage = "";
    repairMessage = "";
    try {
      diagnosis = await invoke<DshDiagnosis>("dsh_diagnose");
      inspected = true;
    } catch (error) {
      onError(error);
    } finally {
      diagnosing = false;
    }
  }

  async function repair() {
    const pkg = diagnosis?.culprit;
    if (!pkg || repairing) return;
    if (
      !(await confirm(
        t("修复 DSH 插件"),
        t("从 web 配置中移除插件 “{package}” 并删除其文件吗？操作前会创建可回滚的快照，其余插件不受影响。", { package: pkg }),
        t("修复"),
      ))
    ) {
      return;
    }
    repairing = true;
    try {
      const report = await invoke<DshRepairReport>("dsh_repair", { request: { package: pkg } });
      repairMessage = t("已移除 {package}（依赖清单{dependencies} · 插件清单{bundles} · 模块目录{nodeModules}）", {
        package: report.package,
        dependencies: report.removedFromDependencies ? "✓" : "—",
        bundles: report.removedFromBundles ? "✓" : "—",
        nodeModules: report.nodeModulesRemoved ? "✓" : "—",
      });
      diagnosis = await invoke<DshDiagnosis>("dsh_diagnose");
      statusMessage = t("修复完成。点击“启动”重新拉起 DSH。");
    } catch (error) {
      onError(error);
    } finally {
      repairing = false;
    }
  }

  async function stopDsh() {
    if (stopping) return;
    stopping = true;
    try {
      await invoke("stop_dsh");
      statusMessage = t("DSH 已停止");
    } catch (error) {
      onError(error);
    } finally {
      stopping = false;
    }
  }

  function formatTimestamp(value: number | null): string {
    if (!value) return t("时间未知");
    return new Intl.DateTimeFormat(getLocale(), { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
  }
</script>

<section class="settings-group" aria-labelledby="dsh-service-heading">
  <div class="settings-group-header">
    <h3 id="dsh-service-heading">{t("DSH 服务")}</h3>
  </div>
  <div class="setting-control">
    <span class="setting-copy">
      <strong>DSH (DeepSeek Harness)</strong>
      <small>{dshRunning ? t("运行中") : t("未运行")}</small>
    </span>
    <div class="dsh-actions">
      {#if restartBusy === "dsh"}<span role="status" class="muted">{t("正在重启…")}</span>{/if}
      <button type="button" class="quiet-button" disabled={busy || restartBusy !== null || busyRuntime !== null} onclick={onRestartDsh}>
        <RotateCcw size={14} />{dshRunning ? t("重启") : t("启动")}
      </button>
      <button type="button" class="quiet-button" disabled={busy || !dshRunning || restartBusy !== null} onclick={() => void stopDsh()}>
        <Square size={13} />{t("停止")}
      </button>
    </div>
  </div>
  <p class="muted" role="status">{t("启动/重启会停止并重新拉起 DSH 服务与界面；停止只结束 DSH 进程。")}</p>
</section>

<section class="settings-group" aria-labelledby="dsh-repair-heading">
  <div class="settings-group-header">
    <h3 id="dsh-repair-heading">{t("启动诊断与修复")}</h3>
    <button type="button" class="quiet-button" disabled={busy} title={t("查看最近一次 DSH 启动失败原因，并按提示修复")} onclick={() => void runDiagnosis()}>
      {#if diagnosing}<span class="spin"><RefreshCw size={13} /></span>{:else}<Wrench size={13} />{/if}
      {t("修复")}
    </button>
  </div>
  {#if !inspected}
    <p class="muted" role="status">{t("DSH 启动失败后，DeepPi 会记录失败原因；点“修复”查看最近一次记录，并按提示处理。")}</p>
    <p class="muted">{t("常见原因是插件与当前 DSH 版本不兼容；DSH 正常运行时这里没有记录可看。")}</p>
  {:else if diagnosis && !diagnosis.hasFailure}
    <p class="muted" role="status">{t("未检测到启动失败记录。若 DSH 现在起不来，先点“启动”重现一次失败，再回到这里点“修复”。")}</p>
  {:else if diagnosis}
    <p class="muted" role="status">{t("最近一次启动失败 · {time}", { time: formatTimestamp(diagnosis.timestamp) })}{diagnosis.error ? ` · ${tm(diagnosis.error)}` : ""}</p>
    {#if diagnosis.lines.length}
      <pre class="dsh-log" role="figure" aria-label={t("DSH 启动失败日志")}>{diagnosis.lines.join("\n")}</pre>
    {/if}
    {#if diagnosis.kind === "activation-pending" && diagnosis.pending.length}
      <div class="setting-control">
        <span class="setting-copy">
          <strong>{t("有 {count} 个插件等不到服务，未能激活", { count: diagnosis.pending.length })}</strong>
          <small>
            {t("这些插件要求的服务（如 uiSession / remote.settings / sidebarRight）在“当前运行的 DSH 版本”里不存在，通常意味着")}
            <b>{t("DSH 运行时版本与该插件的期望版本不匹配")}</b>{t("。")}
          </small>
        </span>
        <div class="dsh-actions">
          <button type="button" class="quiet-button" disabled={busy || dshRunning || busyRuntime !== null} onclick={() => void updateDshRuntime()}>
            <Wrench size={13} />{t("更新 DSH 运行时")}
          </button>
        </div>
      </div>
      <ul class="dsh-plugins">
        {#each diagnosis.pending as name (name)}<li>{name}</li>{/each}
      </ul>
      <p class="muted" role="status">
        {t("处理顺序建议：1) 先更新 DSH 运行时（或把它升级到插件要求的最低版本）；2) 仍不行则在 DSH 插件市场里更新这些插件； 3) 最后才考虑禁用它们。此情形下直接移除插件并不能解决问题。")}
      </p>
      {#if dshRunning}
        <p class="muted" role="status">{t("DSH 正在运行；更新运行时前请先停止 DSH。")}</p>
      {/if}
    {:else if diagnosis.culprit}
      <div class="setting-control">
        <span class="setting-copy">
          <strong>{t("疑似不兼容插件：{name}", { name: diagnosis.culprit ?? "" })}</strong>
          <small>{t("移除该插件通常可恢复启动；之后可在 DSH 插件市场安装与当前 DSH 版本兼容的版本。")}</small>
        </span>
        <div class="dsh-actions">
          {#if repairing}<span role="status" class="muted">{t("正在修复…")}</span>{/if}
          <button type="button" class="quiet-button" disabled={busy || dshRunning} title={dshRunning ? t("请先停止 DSH") : ""} onclick={() => void repair()}>
            <Wrench size={13} />{t("一键修复")}
          </button>
        </div>
      </div>
      {#if dshRunning}
        <p class="muted" role="status">{t("DSH 正在运行；修复前请先停止 DSH。")}</p>
      {/if}
    {:else}
      <p class="muted" role="status">{t("日志中未识别出明确的插件包名。可尝试重启 DSH，或把下方日志提供给插件作者。")}</p>
    {/if}
  {/if}
  {#if repairMessage}<p class="dsh-status" role="status">{repairMessage}</p>{/if}
  {#if statusMessage}<p class="dsh-status" role="status">{statusMessage}</p>{/if}
</section>

<style>
  .dsh-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; }
  .dsh-log { max-height: 220px; overflow: auto; margin: 8px 0; padding: 10px 12px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: 12px/1.5 var(--code-font, monospace); white-space: pre-wrap; overflow-wrap: anywhere; }
  .dsh-status { margin: 8px 0 0; font-size: 12px; color: var(--accent, #2f7d4f); overflow-wrap: anywhere; }
  .dsh-plugins { margin: 6px 0 0; padding: 0 0 0 18px; display: grid; gap: 2px; font-family: var(--code-font); font-size: 11px; color: var(--text-muted); overflow-wrap: anywhere; }
  .spin { display: inline-grid; animation: dsh-spin 0.8s linear infinite; }
  @keyframes dsh-spin { to { transform: rotate(360deg); } }
</style>
