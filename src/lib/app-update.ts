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

export async function checkAppUpdate(): Promise<Update | null> {
  // A signed stable channel may intentionally point to an older version during rollback.
  return check({ timeout: 15_000, allowDowngrades: true });
}

export async function installAppUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}
