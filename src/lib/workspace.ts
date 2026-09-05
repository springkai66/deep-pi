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
