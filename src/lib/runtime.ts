export interface RuntimeComponent {
  id: "deeppi" | "node" | "pi" | "dsh" | "dshmarket";
  name: string;
  currentVersion: string | null;
  source: "managed" | "development" | "profile";
  available: boolean;
}

export interface RuntimeUpdate {
  id: string;
  name: string;
  currentVersion: string | null;
  latestVersion: string | null;
  updateAvailable: boolean;
  installable: boolean;
  canRollback: boolean;
  stale: boolean;
  error: string | null;
  /** 版本说明，例如上游有更新但尚未通过兼容验证。 */
  note: string | null;
}

/** 「稍后提醒」的有效期：24 小时。 */
export const SNOOZE_DURATION_MS = 24 * 60 * 60 * 1000;

/**
 * 某个组件当前是否被「跳过」或「稍后提醒」抑制。
 *
 * - 跳过：针对**具体版本**（`skippedUpdates[id] === latestVersion`），所以上游发了
 *   更新版本后会自动重新提示，不会永久静音。
 * - 稍后提醒：带过期时间，过期后自动重新提示。
 * - 两者都依赖 `latestVersion`：没有已知新版本时不抑制任何东西。
 */
export function updateSuppressed(
  update: Pick<RuntimeUpdate, "latestVersion"> | undefined,
  componentId: string,
  skippedUpdates: Record<string, string>,
  snoozedUpdates: Record<string, number>,
  now: number = Date.now(),
): { skipped: boolean; snoozed: boolean; suppressed: boolean } {
  const latest = update?.latestVersion ?? null;
  const skipped = Boolean(latest && skippedUpdates[componentId] === latest);
  const snoozed = Boolean(latest && (snoozedUpdates[componentId] ?? 0) > now);
  return { skipped, snoozed, suppressed: skipped || snoozed };
}

/**
 * 该组件是否应展示「更新 / 安装 / 修复」入口（被跳过或稍后提醒时隐藏）。
 *
 * 用类型谓词保住 `update` 的非空收窄，使调用方在不做额外判断的情况下仍能
 * 直接使用 `update`（否则模板里每次访问都要再判一次 undefined）。
 */
export function updateActionsVisible(
  update: RuntimeUpdate | undefined,
  skipped: boolean,
  snoozed: boolean,
): update is RuntimeUpdate {
  return Boolean(update?.latestVersion && update.installable && !skipped && !snoozed);
}