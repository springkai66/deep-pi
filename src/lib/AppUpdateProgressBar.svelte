<script lang="ts">
  /**
   * 应用更新的进度条（下载 + 安装）。
   *
   * 设置页与主界面全局浮层共用同一个组件、同一份进度状态；安装阶段的
   * 百分比由文案明确标注为估算（`describeAppUpdateProgress` 负责）。
   */
  import { t } from "$lib/i18n.svelte";
  import { describeAppUpdateProgress, type AppUpdateProgress } from "./app-update";

  let { progress, label }: { progress: AppUpdateProgress; label?: string } = $props();
  const text = $derived(describeAppUpdateProgress(progress));
</script>

<div class="app-update-progress" role="status" aria-live="polite" aria-label={label ?? t("应用更新进度")}>
  <div class="app-update-progress-bar" aria-hidden="true">
    <div
      class="app-update-progress-fill"
      class:indeterminate={progress.percent === null}
      style={progress.percent === null ? undefined : `width: ${progress.percent}%`}
    ></div>
  </div>
  <span class="app-update-progress-text">{text}</span>
</div>

<style>
  .app-update-progress { display: grid; gap: 4px; margin-top: 6px; }
  .app-update-progress-bar { height: 4px; border-radius: 2px; background: var(--surface-hover); overflow: hidden; }
  .app-update-progress-fill { height: 100%; border-radius: 2px; background: var(--accent); transition: width .3s ease; }
  .app-update-progress-fill.indeterminate { width: 40%; animation: app-update-progress-slide 1.2s ease-in-out infinite; }
  @keyframes app-update-progress-slide {
    0% { margin-left: -40%; }
    100% { margin-left: 100%; }
  }
  .app-update-progress-text { font-size: 11px; color: var(--text-muted); }
</style>
