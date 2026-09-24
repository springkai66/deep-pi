/**
 * 桌宠任务环（纯几何 + 时序）。
 *
 * 气泡像时钟刻度一样围着桌宠本体均匀排成一圈：浮层窗口（`pet-tasks`）是
 * 正方形，窗口中心与本体中心重合，气泡沿圆周均匀分布，圆周以内留给本体。
 * 相邻气泡的圆心距随半径增大，因此半径按「刻度数量」反推——任务越多，
 * 气泡越窄、环越大，保证任何数量下气泡互不重叠、各自占住一个方向。
 * 本模块只做算术，不碰 DOM 也不碰 Tauri。
 */

import type { Task } from "./task";

/** 浮层窗口逻辑边长（与 pet.rs 的 TASKS_SIZE 必须一致）。 */
export const RING_WINDOW = 960;
/** 窗口中心：本体中心与窗口中心重合（贴合窗口）。 */
export const RING_CENTER = RING_WINDOW / 2;
/** 最宽档的气泡宽度（≤7 个任务时）；宽度同时是布局宽度与 CSS 兜底宽度。 */
export const RING_BUBBLE_WIDTH = 168;
export const RING_BUBBLE_HEIGHT = 32;
/**
 * 环半径下限：贴着本体外缘。贴合窗口的本体半高约 60px，气泡半高 16px，
 * 因此 78 就是「气泡内缘刚好不压到小熊」的半径——单个任务时气泡就悬在
 * 头顶正上方一点，中间不留大块空白。
 */
export const RING_RADIUS_MIN = 78;
/** 环半径上限：最宽气泡贴窗口边缘再留 8px 余量。 */
export const RING_RADIUS_MAX = 400;
/** 相邻气泡圆心距至少比气泡宽度多出的间隙。 */
const RING_GAP = 8;
/**
 * 目标半径默认值（与 Rust 侧 settings 的 pet_task_ring_radius 一致）：
 * 气泡宁可窄一点，也要让环贴近本体——刻度多时按这个半径反推气泡宽度，
 * 而不是把环越推越远。用户可在设置里改（RING_RADIUS_RANGE）。
 */
export const RING_RADIUS_TARGET_DEFAULT = 100;
/** 用户可设置的半径范围；下限即 RING_RADIUS_MIN（再近就压到本体上了）。 */
export const RING_RADIUS_RANGE = { min: RING_RADIUS_MIN, max: 240, step: 5 } as const;
/** 气泡宽度下限：再窄就读不出标题了，此时改由半径让步（环变大）。 */
export const RING_BUBBLE_WIDTH_MIN = 80;
/** 环上最多的任务气泡数；超出的任务汇总成角标。 */
export const RING_TASK_LIMIT = 20;

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

/**
 * 气泡宽度按刻度数量反推：以「目标半径」下相邻气泡恰好不相邻为目标，
 * 刻度越多气泡越窄（下限 RING_BUBBLE_WIDTH_MIN），从而让环始终贴着本体
 * ——而不是靠把环越推越远来避免重叠。刻度很少时回到最宽档 168px。
 * target 由用户设置（设置 → 通知与桌宠 → 任务环半径）传入。
 */
export function ringBubbleWidth(slots: number, target = RING_RADIUS_TARGET_DEFAULT): number {
  if (slots <= 1) return RING_BUBBLE_WIDTH;
  const fit = 2 * normalizeTarget(target) * Math.sin(Math.PI / slots) - RING_GAP;
  return Math.round(Math.max(RING_BUBBLE_WIDTH_MIN, Math.min(RING_BUBBLE_WIDTH, fit)));
}

/** 用户设置值归一化：非有限值回落默认，并钳进可设置范围。 */
function normalizeTarget(target: number): number {
  if (!Number.isFinite(target)) return RING_RADIUS_TARGET_DEFAULT;
  return Math.min(RING_RADIUS_RANGE.max, Math.max(RING_RADIUS_RANGE.min, target));
}

/**
 * 环半径：取「用户设定的半径」与「不重叠所需半径」中较大的一个，再钳进
 * [下限, 上限]。用户把半径调近时气泡会自动收窄；窄到下限仍放不下时，
 * 半径只能变大（保证永不重叠）。
 */
export function ringRadius(slots: number, target = RING_RADIUS_TARGET_DEFAULT): number {
  const wanted = normalizeTarget(target);
  if (slots <= 1) return Math.min(wanted, RING_RADIUS_MAX);
  const width = ringBubbleWidth(slots, wanted);
  const needed = Math.ceil((width + RING_GAP) / (2 * Math.sin(Math.PI / slots)));
  return Math.max(RING_RADIUS_MIN, Math.min(Math.max(wanted, needed), RING_RADIUS_MAX));
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
  /** 本圈气泡的宽度（按刻度数量分档，见 ringBubbleWidth）。 */
  bubbleWidth: number;
  bubbles: RingBubbleSlot[];
  /** 溢出角标：位置 + 剩余任务数；任务未超上限时为 null。 */
  badge: (RingSlot & { count: number }) | null;
}

/** 依次显示的节奏：第 index 个元素在入场/退场序列中的起始延迟。 */
export function ringStaggerMs(index: number, stepMs: number): number {
  return index * stepMs;
}

/**
 * 布局整圈：先按刻度数量定半径与气泡宽度，再把任务和（可能的）溢出角标
 * 一起均匀铺在圆周上。有角标时它占最后一个位置——「依次显示」讲的就是
 * 「一朵朵花开，最后告诉你还剩几朵」。
 */
export function ringLayout(
  tasks: Task[],
  target = RING_RADIUS_TARGET_DEFAULT,
): RingLayout {
  const visible = orderRingTasks(tasks);
  const overflow = ringOverflowCount(tasks.length);
  const slots = visible.length + (overflow > 0 ? 1 : 0);
  const radius = ringRadius(slots, target);
  const bubbleWidth = ringBubbleWidth(slots, target);
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
  return { radius, bubbleWidth, bubbles, badge };
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
