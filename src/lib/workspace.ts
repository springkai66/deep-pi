export interface ProjectTaskState {
  projectPath: string;
  status: string;
}

const ACTIVE_STATUSES = new Set(["queued", "running", "waiting"]);

function normalizeProjectPath(path: string): string {
  return path.trim().replaceAll("\\", "/").replace(/\/+$/, "").toLocaleLowerCase();
}

export function hasProjectConflict(projectPath: string, tasks: ProjectTaskState[]): boolean {
  const target = normalizeProjectPath(projectPath);
  return tasks.some(
    (task) =>
      ACTIVE_STATUSES.has(task.status) && normalizeProjectPath(task.projectPath) === target,
  );
}

export interface PaneRequest {
  current: string[];
  active: string | null;
  available: string[];
  capacity: number;
}

export interface OpenPaneRequest extends PaneRequest {
  requested: string;
}

export interface PaneSelection {
  panes: string[];
  active: string | null;
}

function boundedCapacity(capacity: number): number {
  return Math.min(4, Math.max(1, Math.trunc(capacity)));
}

function validIds(ids: string[], available: string[]): string[] {
  return [...new Set(ids)].filter((id) => available.includes(id));
}

export function openPane({
  current,
  active,
  requested,
  available,
  capacity,
}: OpenPaneRequest): PaneSelection {
  if (!available.includes(requested)) {
    return changePaneCapacity({ current, active, available, capacity });
  }
  const limit = boundedCapacity(capacity);
  let panes = validIds(current, available).slice(0, limit);
  if (panes.includes(requested)) return { panes, active: requested };
  if (limit === 1) return { panes: [requested], active: requested };
  if (panes.length < limit) return { panes: [...panes, requested], active: requested };

  const replaceAt = Math.max(0, panes.indexOf(active ?? ""));
  panes = panes.map((id, index) => (index === replaceAt ? requested : id));
  return { panes, active: requested };
}

export interface SplitPaneRequest extends PaneRequest {
  /** 分割目标：右键菜单选中的任务。 */
  requested: string;
  /** 分割方向：right 在活动窗格右侧插入，down 在其下方插入。 */
  direction: "right" | "down";
}

/**
 * 右键分割：把当前窗格切成两半，新任务占一半。
 * - right：布局进入双列（或四格），新窗格插到活动窗格之后（水平方向）。
 * - down：同上，但新窗格语义上排在下方；grid 布局按行优先填充。
 * 都没有空位时替换活动窗格旁的第一个非活动窗格。
 */
export function splitPane({
  current,
  active,
  requested,
  available,
  capacity,
  direction,
}: SplitPaneRequest): PaneSelection {
  if (!available.includes(requested)) {
    return changePaneCapacity({ current, active, available, capacity });
  }
  const limit = boundedCapacity(capacity);
  // 至少需要两个窗格才有分割意义；single 模式先升到 split。
  const effectiveLimit = Math.max(2, limit);
  let panes = validIds(current, available).slice(0, effectiveLimit);
  const anchor = active !== null && panes.includes(active) ? panes.indexOf(active) : 0;
  if (panes.includes(requested)) {
    // 已经打开：把它挪到锚点旁边（分割语义）。
    panes = panes.filter((id) => id !== requested);
    panes.splice(direction === "down" ? Math.min(anchor + effectiveLimit, panes.length) : anchor + 1, 0, requested);
    return { panes: panes.slice(0, effectiveLimit), active: requested };
  }
  if (panes.length < effectiveLimit) {
    panes.splice(anchor + 1, 0, requested);
    return { panes, active: requested };
  }
  // 满员：替换锚点后第一个非活动窗格。
  const replaceAt = panes.findIndex((id, index) => index > anchor && id !== active);
  const target = replaceAt === -1 ? (anchor + 1) % panes.length : replaceAt;
  panes[target] = requested;
  return { panes, active: requested };
}

export function changePaneCapacity({
  current,
  active,
  available,
  capacity,
}: PaneRequest): PaneSelection {
  const limit = boundedCapacity(capacity);
  const valid = validIds(current, available);
  const currentActive = active && available.includes(active) ? active : valid[0] ?? available[0] ?? null;

  let panes = valid;
  if (panes.length > limit) {
    panes = currentActive
      ? [currentActive, ...panes.filter((id) => id !== currentActive)].slice(0, limit)
      : panes.slice(0, limit);
  }
  for (const id of available) {
    if (panes.length >= limit) break;
    if (!panes.includes(id)) panes.push(id);
  }
  return {
    panes,
    active: currentActive && panes.includes(currentActive) ? currentActive : panes[0] ?? null,
  };
}
