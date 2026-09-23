import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { Task } from "./task";
import {
  RING_BUBBLE_HEIGHT,
  RING_BUBBLE_WIDTH,
  RING_CENTER,
  RING_ENTER_MS,
  RING_ENTER_STAGGER_MS,
  RING_EXIT_MS,
  RING_EXIT_STAGGER_MS,
  RING_RADIUS_MAX,
  RING_RADIUS_MIN,
  RING_TASK_LIMIT,
  RING_WINDOW,
  orderRingTasks,
  ringCenteredAngle,
  ringEnterMs,
  ringExitMs,
  ringLayout,
  ringOverflowCount,
  ringRadius,
  ringSlotCount,
  type Monitor,
} from "./pet-task-ring";

function task(overrides: Partial<Task> & { id: string }): Task {
  return {
    runId: "run-1",
    title: `任务 ${overrides.id}`,
    agent: "pi",
    status: "running",
    interactionMode: "rpc",
    projectId: "project",
    projectPath: "F:/project",
    sessionId: "session",
    sessionFile: null,
    executionTarget: "local",
    createdAt: 0,
    startedAt: null,
    completedAt: null,
    archivedAt: null,
    ...overrides,
  } as Task;
}

/** 1080p 主屏；窗口中心落在正中时四周都宽裕。 */
const SCREEN: Monitor[] = [[0, 0, 1920, 1080]];
const SCREEN_CENTER = { x: 960, y: 540 };
const ids = (tasks: Task[]) => tasks.map((value) => value.id);

describe("ring task ordering", () => {
  it("puts running first, then waiting, then recently ended", () => {
    const ordered = orderRingTasks([
      task({ id: "done", status: "completed", completedAt: 10 }),
      task({ id: "wait", status: "waiting" }),
      task({ id: "run", status: "running" }),
    ]);
    expect(ids(ordered)).toEqual(["run", "wait", "done"]);
  });

  it("keeps the most recently active task first inside a tier", () => {
    const ordered = orderRingTasks([
      task({ id: "old", status: "running", startedAt: 100 }),
      task({ id: "new", status: "running", startedAt: 900 }),
    ]);
    expect(ids(ordered)).toEqual(["new", "old"]);
  });

  it("falls back to the source order when timestamps are equal or missing", () => {
    // 后端顺序稳定意味着刷新不会让气泡在环上跳位置。
    const ordered = orderRingTasks([
      task({ id: "a", status: "running" }),
      task({ id: "b", status: "running" }),
      task({ id: "c", status: "running" }),
    ]);
    expect(ids(ordered)).toEqual(["a", "b", "c"]);
  });

  it("caps the ring and reports the remainder as overflow", () => {
    const many = Array.from({ length: 9 }, (_, index) => task({ id: `t${index}` }));
    expect(orderRingTasks(many)).toHaveLength(RING_TASK_LIMIT);
    expect(ringOverflowCount(9)).toBe(3);
    expect(ringOverflowCount(RING_TASK_LIMIT)).toBe(0);
    expect(ringOverflowCount(0)).toBe(0);
  });
});

describe("ring radius", () => {
  it("uses the maximum radius when the pet sits well inside the screen", () => {
    expect(ringRadius(SCREEN_CENTER, SCREEN)).toBe(RING_RADIUS_MAX);
  });

  it("shrinks to fit when the pet is close to a screen edge", () => {
    // 距右缘 180px：180 - 半个气泡高 = 164，仍高于上限，所以要按上限渲染。
    expect(ringRadius({ x: 1740, y: 540 }, SCREEN)).toBe(RING_RADIUS_MAX);
    // 距右缘 140px：140 - 16 = 124，落入收缩区间。
    expect(ringRadius({ x: 1780, y: 540 }, SCREEN)).toBe(140 - RING_BUBBLE_HEIGHT / 2);
    // 更贴边：收缩到下限后不再继续变小，避免圈缩到桌宠身上。
    expect(ringRadius({ x: 1900, y: 540 }, SCREEN)).toBe(RING_RADIUS_MIN);
    expect(ringRadius({ x: 1919, y: 1079 }, SCREEN)).toBe(RING_RADIUS_MIN);
  });

  it("keeps the maximum when no monitor contains the center", () => {
    expect(ringRadius({ x: 5000, y: 5000 }, SCREEN)).toBe(RING_RADIUS_MAX);
    expect(ringRadius(SCREEN_CENTER, [])).toBe(RING_RADIUS_MAX);
  });

  it("measures against the edges of the monitor that contains the center", () => {
    const two: Monitor[] = [[-1920, 0, 1920, 1080], [0, 0, 1920, 1080]];
    expect(ringRadius({ x: -960, y: 540 }, two)).toBe(RING_RADIUS_MAX);
    // 左侧副屏的最左缘：可用距离 60 → 落到下限。
    expect(ringRadius({ x: -1860, y: 540 }, two)).toBe(RING_RADIUS_MIN);
  });
});

describe("ring layout", () => {
  it("places a single task straight above the pet", () => {
    const layout = ringLayout([task({ id: "only" })], SCREEN_CENTER, SCREEN);
    expect(layout.bubbles).toHaveLength(1);
    expect(layout.bubbles[0].x).toBe(0);
    expect(layout.bubbles[0].y).toBe(-RING_RADIUS_MAX);
    expect(layout.badge).toBeNull();
  });

  it("lays the ring out symmetrically around the vertical axis", () => {
    const tasks = ["a", "b", "c", "d"].map((id) => task({ id }));
    const layout = ringLayout(tasks, SCREEN_CENTER, SCREEN);
    const xs = layout.bubbles.map((bubble) => bubble.x);
    // 4 个元素时落在正上方、正右、正下、正左。
    expect(layout.bubbles.map((bubble) => [bubble.x, bubble.y])).toEqual([
      [0, -RING_RADIUS_MAX],
      [RING_RADIUS_MAX, 0],
      [0, RING_RADIUS_MAX],
      [-RING_RADIUS_MAX, 0],
    ]);
    expect(xs.reduce((sum, value) => sum + value, 0)).toBe(0);
  });

  it("gives the overflow badge the last slot on the ring", () => {
    const tasks = Array.from({ length: 8 }, (_, index) => task({ id: `t${index}` }));
    const layout = ringLayout(tasks, SCREEN_CENTER, SCREEN);
    expect(layout.bubbles).toHaveLength(RING_TASK_LIMIT);
    expect(layout.badge?.count).toBe(2);
    expect(ringSlotCount(layout)).toBe(RING_TASK_LIMIT + 1);
    // 角标排在第 7 位 → 延迟最晚，正好是「依次显示」的最后一个。
    expect(layout.badge?.delayMs).toBe(RING_TASK_LIMIT * RING_ENTER_STAGGER_MS);
  });

  it("keeps every bubble inside the square window", () => {
    const tasks = Array.from({ length: RING_TASK_LIMIT }, (_, index) => task({ id: `t${index}` }));
    // 三种落点：屏幕正中、贴近右下角（半径收缩）、贴近左上角（半径收缩）。
    for (const center of [SCREEN_CENTER, { x: 1900, y: 1070 }, { x: 20, y: 20 }]) {
      const layout = ringLayout(tasks, center, SCREEN);
      for (const bubble of layout.bubbles) {
        expect(Math.abs(bubble.x) + RING_BUBBLE_WIDTH / 2).toBeLessThanOrEqual(RING_CENTER);
        expect(Math.abs(bubble.y) + RING_BUBBLE_HEIGHT / 2).toBeLessThanOrEqual(RING_CENTER);
        // 圈的内缘必须留在桌宠本体之外，否则气泡会压在本体上。
        expect(Math.hypot(bubble.x, bubble.y)).toBeGreaterThan(75);
      }
    }
  });

  it("starts each slot one stagger step after the previous one", () => {
    const tasks = ["a", "b", "c"].map((id) => task({ id }));
    const layout = ringLayout(tasks, SCREEN_CENTER, SCREEN);
    expect(layout.bubbles.map((bubble) => bubble.delayMs)).toEqual([
      0,
      RING_ENTER_STAGGER_MS,
      2 * RING_ENTER_STAGGER_MS,
    ]);
  });

  it("keeps the centered angle inside the circle for every slot count", () => {
    for (let index = 0; index < RING_TASK_LIMIT + 1; index += 1) {
      const angle = ringCenteredAngle(index, RING_TASK_LIMIT + 1);
      expect(Number.isFinite(angle)).toBe(true);
    }
    expect(ringCenteredAngle(0, 0)).toBe(-Math.PI / 2);
  });
});

describe("ring animation timing", () => {
  it("gives the first slot no delay and grows by one stagger step", () => {
    expect(ringEnterMs(1)).toBe(RING_ENTER_MS);
    expect(ringEnterMs(6)).toBe(5 * RING_ENTER_STAGGER_MS + RING_ENTER_MS);
    expect(ringEnterMs(0)).toBe(RING_ENTER_MS);
  });

  it("exits in the same direction but with a tighter rhythm", () => {
    // 退场与入场同向（Q7 定稿）：第 0 个先走，间隔比入场紧。
    expect(ringExitMs(6)).toBe(5 * RING_EXIT_STAGGER_MS + RING_EXIT_MS);
    expect(RING_EXIT_STAGGER_MS).toBeLessThan(RING_ENTER_STAGGER_MS);
    expect(RING_EXIT_MS).toBeLessThan(RING_ENTER_MS);
  });

  it("keeps the square window big enough for the widest ring", () => {
    expect(RING_WINDOW).toBe(2 * RING_CENTER);
    // 半径上限由气泡宽度反推：最宽处正好贴到窗口边缘（高度更小，不构成瓶颈）。
    expect(RING_RADIUS_MAX).toBe(RING_CENTER - RING_BUBBLE_WIDTH / 2);
    expect(RING_RADIUS_MAX + RING_BUBBLE_WIDTH / 2).toBeLessThanOrEqual(RING_CENTER);
    expect(RING_RADIUS_MAX + RING_BUBBLE_HEIGHT / 2).toBeLessThanOrEqual(RING_CENTER);
    // 下限仍要留在本体之外。
    expect(RING_RADIUS_MIN).toBeGreaterThan(75 + RING_BUBBLE_HEIGHT / 2);
  });
});

describe("ring stylesheet mirrors the ring constants", () => {
  // 样式表里的时长/宽度是写死的（Svelte 会把 <style> 里的花括号当表达式解析，
  // 没法插值）。这里守住它们与常量一致，避免改了常量而动画悄悄失配。
  const component = readFileSync("src/lib/PetTaskBubbles.svelte", "utf8");

  it("uses the same enter/exit durations and bubble width", () => {
    expect(component).toContain(`transition: opacity ${RING_ENTER_MS}ms ease-out, scale ${RING_ENTER_MS}ms ease-out;`);
    expect(component).toContain(`transition-duration: ${RING_EXIT_MS}ms;`);
    // 按钮与文字都按这个宽度排版：CSS 宽度必须等于布局宽度。
    expect(component).toContain("width: 168px;");
    expect(RING_BUBBLE_WIDTH).toBe(168);
  });
});
