import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { t } from "$lib/i18n.svelte";

export type { Update } from "@tauri-apps/plugin-updater";

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

export async function installAppUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}
