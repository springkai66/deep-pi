/**
 * 时长格式化，与 pi TUI 的 `formatDuration` 保持完全一致（dist/core/tools/renderers/bash.js）：
 *
 * - 不足 1 分钟：一位小数的秒，如 `42.3s`
 * - 不足 1 小时：分 + 秒，如 `2m 5s`
 * - 超过 1 小时：时 + 分 + 秒，如 `1h 2m 3s`
 *
 * DeepPi 在工具执行（Elapsed / Took）与每轮对话总耗时上使用同一格式，保证与 pi TUI 显示一致。
 */

export function formatDuration(ms: number): string {
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const totalSeconds = Math.floor(seconds);
  const minutes = Math.floor(totalSeconds / 60);
  const remainder = totalSeconds % 60;
  if (minutes < 60) return `${minutes}m ${remainder}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m ${remainder}s`;
}
