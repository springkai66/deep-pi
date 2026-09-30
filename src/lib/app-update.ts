import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { t } from "$lib/i18n.svelte";

export type { DownloadEvent, Update } from "@tauri-apps/plugin-updater";

type AppUpdateStatus = "idle" | "checking" | "available" | "current" | "installing" | "error";

export interface AppUpdateState {
  status: AppUpdateStatus;
  version: string | null;
  notes: string | null;
  error: string | null;
}

/// 应用更新检查的网络重试：瞬断自动重试一次；非网络类错误（签名/清单
/// 解析等）直接抛出。null 表示“无更新”（成功），不触发重试。
/// 覆盖 reqwest 传输层错误的常见文案：error sending request（连接被
/// 重置/拒约/DNS 失败的顶层消息）、connection closed、lookup（DNS）等；
/// 检查是幂等 GET，多试一次无副作用。
const UPDATE_CHECK_RETRIES = 2;
const UPDATE_CHECK_RETRY_DELAY_MS = 1_500;

function isRetryableUpdateError(error: unknown): boolean {
  return /network|fetch|timed? ?out|timeout|connection|temporary|sending|closed|reset|lookup|5\d\d/i.test(
    String(error ?? ""),
  );
}

/// 把更新检查/安装的错误转成可操作的提示：传输层失败时提示
/// 对照系统网络状态和手动代理设置排查；其余原样透出。
export function describeUpdateError(error: unknown): string {
  const detail = String(error ?? "").trim();
  if (!detail) return t("未知错误");
  if (!isRetryableUpdateError(detail)) return detail;
  return t(
    "无法连接更新服务器：{detail}。请确认系统本身能访问更新服务器；若使用手动代理，请检查代理地址及服务是否可用。",
    { detail },
  );
}

export async function checkAppUpdate(): Promise<Update | null> {
  // A signed stable channel may intentionally point to an older version during rollback.
  for (let attempt = 0; ; attempt += 1) {
    try {
      return await check({ timeout: 15_000, allowDowngrades: true });
    } catch (error) {
      if (attempt >= UPDATE_CHECK_RETRIES || !isRetryableUpdateError(error)) throw error;
      await new Promise((resolve) => setTimeout(resolve, UPDATE_CHECK_RETRY_DELAY_MS));
    }
  }
}

/* ------------------------------------------------------------------ *
 * 更新进度
 *
 * 下载阶段的百分比来自 updater 插件的真实事件（Started 给总量，
 * Progress 给增量）；安装阶段插件**没有任何进度事件**（Windows 上会
 * 启动静默安装器并让应用退出），因此只能用按时间估算的百分比，并且
 * 必须在界面上如实标注为估算。
 * ------------------------------------------------------------------ */

export type AppUpdatePhase = "downloading" | "installing";

export interface AppUpdateProgress {
  phase: AppUpdatePhase;
  /// 百分比；null 表示总量未知（不确定进度条），而不是“卡住”。
  percent: number | null;
  /// 安装阶段为 true：百分比是按时间估算的，不是真实安装进度。
  estimated: boolean;
  /// 已下载字节数。
  transferred: number;
  /// 总字节数；插件未给出总量时为 null。
  total: number | null;
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/// 人类可读的字节数：1024 进制，非整数单位保留一位小数，便于显示
/// 「12.3 MB / 27.4 MB」这类对照信息。
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes <= 0) return "0 B";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rounded = unit === 0 ? Math.round(value) : Math.round(value * 10) / 10;
  return `${rounded} ${BYTE_UNITS[unit]}`;
}

/// 把 updater 插件的下载事件累计成进度：`Started.contentLength` 是总量，
/// `Progress.chunkLength` 是增量。总量缺失时百分比保持 null（不确定态），
/// 不编造数字。
export class DownloadProgressTracker {
  #total: number | null = null;
  #transferred = 0;

  apply(event: DownloadEvent): AppUpdateProgress {
    if (event.event === "Started") {
      const length = event.data.contentLength;
      this.#total = typeof length === "number" && length > 0 ? length : null;
      this.#transferred = 0;
    } else if (event.event === "Progress") {
      const chunk = event.data.chunkLength;
      if (typeof chunk === "number" && chunk > 0) this.#transferred += chunk;
    } else if (this.#total !== null) {
      // Finished：直接落到总量，避免最后一点因取整停在 99%。
      this.#transferred = Math.max(this.#transferred, this.#total);
    }
    return this.snapshot();
  }

  snapshot(): AppUpdateProgress {
    const total = this.#total;
    const percent = total === null
      ? null
      : Math.max(0, Math.min(100, Math.floor((this.#transferred / total) * 100)));
    return { phase: "downloading", percent, estimated: false, transferred: this.#transferred, total };
  }
}

/// 估算进度上限：安装由安装器接管，这里最多显示到 99%，不假装已完成。
export const INSTALL_ESTIMATE_CEILING = 99;
/// 估算节奏：每 750ms 涨 1%。
export const INSTALL_ESTIMATE_MS_PER_PERCENT = 750;
/// 估算进度的刷新间隔。
export const INSTALL_ESTIMATE_TICK_MS = 750;

/// 安装阶段的时间估算器：按已过去的毫秒数线性推进，单调不减、封顶 99%。
export class InstallProgressEstimator {
  #startedAt: number | null = null;
  #highest = 0;

  constructor(readonly msPerPercent: number = INSTALL_ESTIMATE_MS_PER_PERCENT) {}

  start(now: number = Date.now()): void {
    this.#startedAt = now;
    this.#highest = 0;
  }

  stop(): void {
    this.#startedAt = null;
  }

  percent(now: number = Date.now()): number {
    if (this.#startedAt === null) return 0;
    const elapsed = Math.max(0, now - this.#startedAt);
    const estimate = Math.min(INSTALL_ESTIMATE_CEILING, Math.floor(elapsed / Math.max(1, this.msPerPercent)));
    // 系统对时或休眠恢复可能把时钟拉回过去，已显示的进度不能因此倒退。
    this.#highest = Math.max(this.#highest, estimate);
    return this.#highest;
  }
}

/// 进度文案：下载阶段是真实百分比与字节数；安装阶段明确标注为按时间估算。
export function describeAppUpdateProgress(progress: AppUpdateProgress): string {
  if (progress.phase === "installing") {
    return t("安装中 约 {percent}%（按时间估算）", { percent: progress.percent ?? 0 });
  }
  if (progress.percent === null) {
    return t("下载中 · 已下载 {done}", { done: formatBytes(progress.transferred) });
  }
  return t("下载中 {percent}% · {done} / {total}", {
    percent: progress.percent,
    done: formatBytes(progress.transferred),
    total: formatBytes(progress.total),
  });
}

export interface AppUpdateInstallHooks {
  /// 进度回调：先下载（真实事件），再安装（按时间估算）。
  onProgress?: (progress: AppUpdateProgress) => void;
  /// 安装阶段估算进度的刷新间隔，测试可调小。
  tickMs?: number;
}

/// 下载 + 安装：拆成两步是为了能上报进度（`downloadAndInstall` 不暴露
/// 安装阶段，但下载阶段的事件一样可用）。安装成功后重启应用。
export async function installAppUpdate(update: Update, hooks: AppUpdateInstallHooks = {}): Promise<void> {
  const tracker = new DownloadProgressTracker();
  const estimator = new InstallProgressEstimator();
  const report = (progress: AppUpdateProgress) => hooks.onProgress?.(progress);

  report(tracker.snapshot());
  await update.download((event) => report(tracker.apply(event)));

  const installSnapshot = (): AppUpdateProgress => {
    const downloaded = tracker.snapshot();
    return {
      phase: "installing",
      percent: estimator.percent(),
      estimated: true,
      transferred: downloaded.transferred,
      total: downloaded.total,
    };
  };

  estimator.start();
  // 安装阶段插件不报进度，只能自己按时间推进，让进度条动起来。
  const ticker = setInterval(() => report(installSnapshot()), Math.max(50, hooks.tickMs ?? INSTALL_ESTIMATE_TICK_MS));
  try {
    report(installSnapshot());
    await update.install();
  } finally {
    clearInterval(ticker);
    estimator.stop();
  }
  await relaunch();
}
