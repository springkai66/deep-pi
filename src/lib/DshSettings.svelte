<script lang="ts">
  /**
   * DSH 服务设置。
   *
   * 只显示运行状态：启动/重启/停止入口在工作区页面（DSH 视图的「重启 DSH」），
   * 设置里不再提供按钮。
   */
  import { t } from "$lib/i18n.svelte";

  interface Props {
    dshRunning: boolean;
  }

  let { dshRunning }: Props = $props();
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
  </div>
</section>
