import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "svelte/server";
import AppToasts from "./AppToasts.svelte";
import { NOTICE_LIMIT, notices } from "./notices.svelte";

afterEach(() => {
  notices.clear();
  vi.useRealTimers();
});

describe("应用浮动提示 store", () => {
  it("推送后可读；相同文案只续期不堆叠；空文案不入队", () => {
    const id = notices.push("no official login is in progress", "error", 0);
    expect(id).toBeGreaterThan(0);
    expect(notices.items).toHaveLength(1);
    expect(notices.push("no official login is in progress", "error", 0)).toBe(id);
    expect(notices.items).toHaveLength(1);
    expect(notices.push("   ")).toBe(0);
    expect(notices.items).toHaveLength(1);
  });

  it("同屏条数超过上限时丢弃最旧的", () => {
    for (let index = 0; index < NOTICE_LIMIT + 2; index++) {
      notices.push(`错误 ${index}`, "error", 0);
    }
    expect(notices.items).toHaveLength(NOTICE_LIMIT);
    expect(notices.items[0].message).toBe("错误 2");
    expect(notices.items.at(-1)?.message).toBe(`错误 ${NOTICE_LIMIT + 1}`);
  });

  it("dismiss 只移除指定提示，clear 清空全部", () => {
    const first = notices.push("第一条", "error", 0);
    const second = notices.push("第二条", "info", 0);
    notices.dismiss(first);
    expect(notices.items.map((item) => item.id)).toEqual([second]);
    notices.clear();
    expect(notices.items).toHaveLength(0);
  });

  it("到达停留时长后自动消失", () => {
    vi.useFakeTimers();
    notices.push("超时消失", "error", 1000);
    expect(notices.items).toHaveLength(1);
    vi.advanceTimersByTime(999);
    expect(notices.items).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(notices.items).toHaveLength(0);
  });
});

describe("浮动提示组件", () => {
  it("把提示渲染成可关闭的浮字（错误用 role=alert）", () => {
    notices.push("no official login is in progress", "error", 0);
    const { body } = render(AppToasts, {});
    expect(body).toContain("no official login is in progress");
    expect(body).toContain('role="alert"');
    expect(body).toContain('aria-label="关闭提示"');
  });

  it("没有提示时不渲染任何浮层", () => {
    const { body } = render(AppToasts, {});
    expect(body).not.toContain("toast-layer");
  });
});
