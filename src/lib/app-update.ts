import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type { Update } from "@tauri-apps/plugin-updater";

type AppUpdateStatus = "idle" | "checking" | "available" | "current" | "installing" | "error";

export interface AppUpdateState {
  status: AppUpdateStatus;
  version: string | null;
  notes: string | null;
  error: string | null;
}

/// 应用更新检查的网络重试：瞬断自动重试一次；非网络类错误（签名/清单
/// 解析等）直接抛出。null 表示"无更新"（成功），不触发重试。
const UPDATE_CHECK_RETRIES = 1;
const UPDATE_CHECK_RETRY_DELAY_MS = 1_500;

function isRetryableUpdateError(error: unknown): boolean {
  return /network|fetch|timed? ?out|timeout|connection|temporary|5\d\d/i.test(String(error ?? ""));
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

export async function installAppUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}
