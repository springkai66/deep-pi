<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { RotateCcw, Square } from "@lucide/svelte";
  import { t } from "$lib/i18n.svelte";

  interface Props {
    dshRunning: boolean;
    restartBusy: string | null;
    busyRuntime: string | null;
    onRestartDsh: () => void;
    onError: (error: unknown) => void;
  }

  let { dshRunning, restartBusy, busyRuntime, onRestartDsh, onError }: Props = $props();
  let stopping = $state(false);

  async function stopDsh() {
    if (stopping) return;
    stopping = true;
    try {
      await invoke("stop_dsh");
    } catch (error) {
      onError(error);
    } finally {
      stopping = false;
    }
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
      <button type="button" class="quiet-button" disabled={stopping || restartBusy !== null || busyRuntime !== null} onclick={onRestartDsh}>
        <RotateCcw size={14} />{dshRunning ? t("重启") : t("启动")}
      </button>
      <button type="button" class="quiet-button" disabled={stopping || !dshRunning || restartBusy !== null} onclick={() => void stopDsh()}>
        <Square size={13} />{t("停止")}
      </button>
    </div>
  </div>
  <p class="muted" role="status">{t("启动/重启会停止并重新拉起 DSH 服务与界面；停止只结束 DSH 进程。")}</p>
</section>

<style>
  .dsh-actions { display: flex; flex-wrap: wrap; align-items: center; justify-content: flex-end; gap: 6px; min-width: 0; }
</style>
