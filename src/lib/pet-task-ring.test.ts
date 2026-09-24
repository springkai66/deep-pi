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
  ringBubbleWidth,
  ringCenteredAngle,
  ringEnterMs,
  ringExitMs,
  ringLayout,
  ringNeededRadius,
  ringOverflowCount,
  ringRadius,
  ringSlotCount,
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

describe("ring task ordering", () => {
  it("puts running first, then waiting, then recently ended", () => {
    const ordered = orderRingTasks([
      task({ id: "done", status: "completed", completedAt: 10 }),
      task({ id: "wait", status: "waiting" }),
      task({ id: "run", status: "running" }),
    ]);
    expect(ordered.map((value) => value.id)).toEqual(["run", "wait", "done"]);
  });

  it("keeps the most recently active task first inside a tier", () => {
    const ordered = orderRingTasks([
      task({ id: "old", status: "running", startedAt: 100 }),
      task({ id: "new", status: "running", startedAt: 900 }),
    ]);
    expect(ordered.map((value) => value.id)).toEqual(["new", "old"]);
  });

  it("falls back to the source order when timestamps are equal or missing", () => {
    // 后端顺序稳定意味着刷新不会让气泡在环上跳位置。
    const ordered = orderRingTasks([
      task({ id: "a", status: "running" }),
      task({ id: "b", status: "running" }),
      task({ id: "c", status: "running" }),
    ]);
    expect(ordered.map((value) => value.id)).toEqual(["a", "b", "c"]);
  });

  it("caps the ring at 20 and reports the remainder as overflow", () => {
    const many = Array.from({ length: 25 }, (_, index) => task({ id: `t${index}` }));
    expect(orderRingTasks(many)).toHaveLength(RING_TASK_LIMIT);
    expect(RING_TASK_LIMIT).toBe(20);
    expect(ringOverflowCount(25)).toBe(5);
    expect(ringOverflowCount(RING_TASK_LIMIT)).toBe(0);
    expect(ringOverflowCount(0)).toBe(0);
  });
});

describe("ring radius follows the clock geometry", () => {
  it("grows the radius so adjacent bubbles never overlap", () => {
    // 圆心距 = 2r·sin(π/slots) 必须容下气泡宽度 + 间隙。
    for (let slots = 2; slots <= RING_TASK_LIMIT + 1; slots += 1) {
      const radius = ringRadius(slots);
      const chord = 2 * radius * Math.sin(Math.PI / slots);
      expect(chord).toBeGreaterThanOrEqual(ringBubbleWidth(slots));
    }
  });

  it("narrows the bubble width as the count grows so the ring stays close", () => {
    expect(ringBubbleWidth(1)).toBe(168);
    expect(ringBubbleWidth(4)).toBe(168);
    // 刻度变多后气泡收窄，环半径才不必被推远。
    expect(ringBubbleWidth(9)).toBeLessThan(168);
    expect(ringBubbleWidth(9)).toBeGreaterThanOrEqual(96);
    expect(ringBubbleWidth(20)).toBeLessThanOrEqual(ringBubbleWidth(9));
    // 下限兜底：再窄也读不出标题，此时改由半径让步。
    expect(ringBubbleWidth(60)).toBe(96);
  });

  it("keeps the radius close to the target whenever the width floor allows", () => {
    // 目标半径 175：气泡宽度没触底时，环半径不超过目标（刻度少时更近，
    // 那是想要的效果），不会像「一味加宽半径」的旧几何那样越推越远。
    for (let slots = 3; slots <= RING_TASK_LIMIT + 1; slots += 1) {
      const width = ringBubbleWidth(slots);
      const radius = ringRadius(slots);
      if (width > 96) expect(radius).toBeLessThanOrEqual(181);
      else expect(radius).toBeGreaterThan(175); // 触底后只能靠半径让步
    }
    // 典型场景：9 个任务时环明显比旧几何（半径 217）更贴近本体。
    expect(ringRadius(9)).toBeLessThanOrEqual(180);
  });

  it("keeps the needed radius inside the window bounds", () => {
    for (let slots = 1; slots <= RING_TASK_LIMIT + 1; slots += 1) {
      const radius = ringRadius(slots);
      expect(radius).toBeGreaterThanOrEqual(RING_RADIUS_MIN);
      expect(radius).toBeLessThanOrEqual(RING_RADIUS_MAX);
      // 最宽的气泡贴窗口边缘再留余量。
      expect(radius + ringBubbleWidth(slots) / 2).toBeLessThanOrEqual(RING_CENTER);
    }
  });

  it("keeps the needed radius monotonic inside each bubble-width tier", () => {
    // 跨档时气泡变窄，所需半径可以回落（168→140 那一档就是如此）；
    // 同一档内刻度越多，需要越大的半径。
    let previous = 0;
    for (let slots = 1; slots <= RING_TASK_LIMIT + 1; slots += 1) {
      const needed = ringNeededRadius(slots);
      if (slots === 1 || ringBubbleWidth(slots) === ringBubbleWidth(slots - 1)) {
        expect(needed).toBeGreaterThanOrEqual(previous);
      }
      previous = needed;
    }
  });

  it("shrinks only when the monitor is too small for the needed radius", () => {
    // 常规显示器装得下所需半径：按需取值，不无故收缩。
    expect(ringRadius(RING_TASK_LIMIT, [[0, 0, 1920, 1080]])).toBe(ringNeededRadius(RING_TASK_LIMIT));
    // 显示器装不下（中心到边不足所需半径）：收缩到可用距离，但不低于下限。
    const tiny = [[0, 0, 500, 500]] as [number, number, number, number][];
    expect(ringRadius(RING_TASK_LIMIT, tiny)).toBe(RING_RADIUS_MIN);
    // 没有显示器信息：按所需半径渲染（宁可出屏，也不无故缩小）。
    expect(ringRadius(RING_TASK_LIMIT, [])).toBe(ringNeededRadius(RING_TASK_LIMIT));
  });
});

describe("ring layout", () => {
  it("places a single task straight above the pet", () => {
    const layout = ringLayout([task({ id: "only" })]);
    expect(layout.bubbles).toHaveLength(1);
    expect(layout.bubbles[0].x).toBe(0);
    expect(layout.bubbles[0].y).toBe(-layout.radius);
    expect(layout.badge).toBeNull();
    expect(layout.bubbleWidth).toBe(168);
  });

  it("spreads slots evenly around the full circle like clock ticks", () => {
    const tasks = ["a", "b", "c", "d"].map((id) => task({ id }));
    const layout = ringLayout(tasks);
    // 4 个元素时落在正上、正右、正下、正左——每个方向各占一个刻度。
    expect(layout.bubbles.map((bubble) => [bubble.x, bubble.y])).toEqual([
      [0, -layout.radius],
      [layout.radius, 0],
      [0, layout.radius],
      [-layout.radius, 0],
    ]);
  });

  it("gives the overflow badge the last slot on the ring", () => {
    const tasks = Array.from({ length: 22 }, (_, index) => task({ id: `t${index}` }));
    const layout = ringLayout(tasks);
    expect(layout.bubbles).toHaveLength(RING_TASK_LIMIT);
    expect(layout.badge?.count).toBe(2);
    expect(ringSlotCount(layout)).toBe(RING_TASK_LIMIT + 1);
    // 角标排在最后一个刻度 → 延迟最晚，正好是「依次显示」的最后一个。
    expect(layout.badge?.delayMs).toBe(RING_TASK_LIMIT * RING_ENTER_STAGGER_MS);
  });

  it("keeps every bubble inside the square window at every count", () => {
    for (let count = 1; count <= RING_TASK_LIMIT + 1; count += 1) {
      const tasks = Array.from({ length: count }, (_, index) => task({ id: `t${index}` }));
      const layout = ringLayout(tasks);
      for (const bubble of layout.bubbles) {
        expect(Math.abs(bubble.x) + layout.bubbleWidth / 2).toBeLessThanOrEqual(RING_CENTER);
        expect(Math.abs(bubble.y) + RING_BUBBLE_HEIGHT / 2).toBeLessThanOrEqual(RING_CENTER);
        // 圈的内缘必须留在桌宠本体之外，否则气泡会压在本体上。
        expect(Math.hypot(bubble.x, bubble.y)).toBeGreaterThan(75);
      }
      if (layout.badge) {
        expect(Math.abs(layout.badge.x) + layout.bubbleWidth / 2).toBeLessThanOrEqual(RING_CENTER);
      }
    }
  });

  it("starts each slot one stagger step after the previous one", () => {
    const tasks = ["a", "b", "c"].map((id) => task({ id }));
    const layout = ringLayout(tasks);
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
    // 上限之下，任何刻度数量的最宽气泡都留在窗口内。
    for (let slots = 1; slots <= RING_TASK_LIMIT + 1; slots += 1) {
      const radius = ringRadius(slots);
      expect(radius + ringBubbleWidth(slots) / 2).toBeLessThanOrEqual(RING_CENTER);
      expect(radius + RING_BUBBLE_HEIGHT / 2).toBeLessThanOrEqual(RING_CENTER);
    }
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
    // CSS 里的 168px 是最宽档兜底值；实际宽度由布局按数量内联给出。
    expect(component).toContain("width: 168px;");
    expect(RING_BUBBLE_WIDTH).toBe(168);
  });
});
