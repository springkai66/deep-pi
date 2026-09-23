/**
 * 桌宠任务环（纯几何 + 时序）。
 *
 * 气泡像花瓣一样围着桌宠本体排成一圈：浮层窗口（`pet-tasks`）是正方形，
 * 窗口中心与本体中心重合，气泡沿圆周均匀分布，圆周以内留给桌宠本体。
 * 本模块只做算术，不碰 DOM 也不碰 Tauri，便于直接覆盖边界情况：
 * 屏幕角落的半径收缩、单任务、满员、溢出角标的落位。
 */

import type { Task } from "./task";

/** 浮层窗口逻辑边长（与 pet.rs 的 TASKS_SIZE 必须一致）。 */
export const RING_WINDOW = 420;
/** 窗口中心：本体中心在窗口内的锚点（本体贴窗口底部居中）。 */
export const RING_CENTER = RING_WINDOW / 2;
/** 气泡卡片尺寸（逻辑像素）；宽度同时是布局宽度与 CSS 宽度，两者必须一致。 */
export const RING_BUBBLE_WIDTH = 168;
export const RING_BUBBLE_HEIGHT = 32;
/** 环半径范围：上限由「气泡最宽处不超出窗口」反推，下限保证最坏情况下仍不与本体（视觉半径约 75）重叠。 */
export const RING_RADIUS_MIN = 110;
export const RING_RADIUS_MAX = RING_WINDOW / 2 - RING_BUBBLE_WIDTH / 2;
/** 环上最多的任务气泡数；超出的任务汇总成角标。 */
export const RING_TASK_LIMIT = 6;

/* ------------------------------------------------------------------ *
 * 时序
 * ------------------------------------------------------------------ */

/** 相邻元素的显示间隔（入场）。 */
export const RING_ENTER_STAGGER_MS = 60;
/** 单个气泡的淡入时长。 */
export const RING_ENTER_MS = 180;
/** 相邻元素的消失间隔（退场，与入场同向，节奏更紧）。 */
export const RING_EXIT_STAGGER_MS = 40;
/** 单个气泡的淡出时长。 */
export const RING_EXIT_MS = 140;

/* ------------------------------------------------------------------ *
 * 排序与截断
 * ------------------------------------------------------------------ */

/** 角标内的任务数 = 总数 − 环上展示数。 */
export function ringOverflowCount(total: number): number {
  return Math.max(0, total - RING_TASK_LIMIT);
}

/**
 * 环上排序：运行中 → 等待输入 → 刚结束，同级取最近活跃的在前。
 * 超过上限时保留前 RING_TASK_LIMIT 个——先到先得的是「最需要你知道」的任务。
 */
export function orderRingTasks(tasks: Task[]): Task[] {
  const tiers = ["running", "waiting", "ended"] as const;
  const tierOf = (task: Task) =>
    task.status === "running" ? 0 : task.status === "waiting" ? 1 : 2;
  const recent = (task: Task) => task.completedAt ?? task.startedAt ?? task.createdAt ?? 0;
  return tasks
    .map((task, index) => ({ task, index }))
    .sort((left, right) => {
      const byTier = tierOf(left.task) - tierOf(right.task);
      if (byTier !== 0) return byTier;
      const byRecent = recent(right.task) - recent(left.task);
      // 时间戳相同（或都缺失）时保持后端给出的顺序，避免列表每次刷新都跳。
      return byRecent !== 0 ? byRecent : left.index - right.index;
    })
    .slice(0, RING_TASK_LIMIT)
    .map((entry) => entry.task);
}

/* ------------------------------------------------------------------ *
 * 几何
 * ------------------------------------------------------------------ */

/** 显示器物理区域 (x, y, width, height)。 */
export type Monitor = [number, number, number, number];

/**
 * 自适应半径：本体中心到屏幕四边的最小可用距离决定环能有多大。
 * 贴边时收缩（下限 RING_RADIUS_MIN），宽裕时不超过 RING_RADIUS_MAX。
 * 没有显示器信息（查询失败）时按上限处理——此时宁可允许出屏，也不无故缩小。
 */
export function ringRadius(center: { x: number; y: number }, monitors: Monitor[]): number {
  if (!monitors.length) return RING_RADIUS_MAX;
  const monitor = monitors.find(
    ([x, y, width, height]) =>
      center.x >= x && center.x < x + width && center.y >= y && center.y < y + height,
  );
  if (!monitor) return RING_RADIUS_MAX;
  const [x, y, width, height] = monitor;
  const available = Math.min(
    center.x - x,
    x + width - center.x,
    center.y - y,
    y + height - center.y,
  );
  return Math.max(
    RING_RADIUS_MIN,
    Math.min(RING_RADIUS_MAX, available - RING_BUBBLE_HEIGHT / 2),
  );
}

/**
 * 把 total 个元素均匀铺满整圈的居中角度（弧度，0 = 正右，顺时针为正）：
 * 第 0 个落在正上方，其余以正上方为轴左右对称铺开。单任务时正好在正上方。
 */
export function ringCenteredAngle(index: number, total: number): number {
  return -Math.PI / 2 + (index * 2 * Math.PI) / Math.max(1, total);
}

/** 环上一个元素的位置与时序（相对窗口中心的像素偏移 + 延迟毫秒）。 */
export interface RingSlot {
  x: number;
  y: number;
  delayMs: number;
}

/** 任务气泡在环上的落位。 */
export interface RingBubbleSlot extends RingSlot {
  task: Task;
}

export interface RingLayout {
  radius: number;
  bubbles: RingBubbleSlot[];
  /** 溢出角标：位置 + 剩余任务数；任务未超上限时为 null。 */
  badge: (RingSlot & { count: number }) | null;
}

/** 依次显示的节奏：第 index 个元素在入场/退场序列中的起始延迟。 */
export function ringStaggerMs(index: number, stepMs: number): number {
  return index * stepMs;
}

/**
 * 布局整圈：先定半径，再把任务和（可能的）溢出角标一起均匀铺在圆周上。
 * 有角标时它占正上方，任务从它两侧对称铺开——最后显示的是角标，
 * 「依次显示」讲的就是「一朵朵花开，最后告诉你还剩几朵」。
 */
export function ringLayout(
  tasks: Task[],
  center: { x: number; y: number },
  monitors: Monitor[],
): RingLayout {
  const radius = ringRadius(center, monitors);
  const visible = orderRingTasks(tasks);
  const overflow = ringOverflowCount(tasks.length);
  const slots = visible.length + (overflow > 0 ? 1 : 0);
  const at = (index: number): RingSlot => {
    const angle = ringCenteredAngle(index, slots);
    return {
      x: Math.round(Math.cos(angle) * radius),
      y: Math.round(Math.sin(angle) * radius),
      delayMs: ringStaggerMs(index, RING_ENTER_STAGGER_MS),
    };
  };
  const bubbles = visible.map((task, index) => ({ task, ...at(index) }));
  const badge = overflow > 0 ? { ...at(slots - 1), count: overflow } : null;
  return { radius, bubbles, badge };
}

/** 入场总时长：最后一个元素的起始延迟 + 它自己的淡入。 */
export function ringEnterMs(slots: number): number {
  return Math.max(0, slots - 1) * RING_ENTER_STAGGER_MS + RING_ENTER_MS;
}

/** 退场总时长；窗口要等它放完才能真正隐藏。 */
export function ringExitMs(slots: number): number {
  return Math.max(0, slots - 1) * RING_EXIT_STAGGER_MS + RING_EXIT_MS;
}

/** 一次布局里元素总数（任务气泡 + 角标）。 */
export function ringSlotCount(layout: RingLayout): number {
  return layout.bubbles.length + (layout.badge ? 1 : 0);
}
