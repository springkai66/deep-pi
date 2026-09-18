import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

// 原生 DSH 子 WebView 永远盖在主窗口的 HTML 之上：只要 DSH 处于激活状态，
// 进入「设置」时就必须把它隐藏，否则设置界面被原生视图挡住（用户报告：
// 「切换到 dsh 打不开 deeppi 的设置」）。这里把显隐条件钉成源码不变式。
const page = readFileSync("src/routes/+page.svelte", "utf8");

describe("DSH 原生子 WebView 的可见性", () => {
  it("设置视图下必须隐藏原生 WebView", () => {
    const match = page.match(/const visible = activeAgent[^;]*;/);
    expect(match, "应能找到 DSH 可见性条件").not.toBeNull();
    const condition = match![0];
    expect(condition).toContain('activeAgent === "dsh"');
    // 关键：非工作区视图（设置）时隐藏。
    expect(condition).toContain('view === "workspace"');
    expect(condition).toContain("!dialogRequest");
    expect(condition).toContain("!closingWindow");
    expect(condition).toContain("!recoveryProject");
  });

  it("异步创建完成后的首次显隐判断同样考虑设置视图", () => {
    const match = page.match(/if \(activeAgent !== "dsh"[^)]*\) await dshWebview\.hide\(\);/);
    expect(match, "应能找到创建 WebView 后的显隐判断").not.toBeNull();
    expect(match![0]).toContain('view !== "workspace"');
  });

  it("DSH 页面分支仅在工作区渲染，不拦截设置组件", () => {
    const match = page.match(/\{#if ([^}]+)\}\s*<div class="dsh-placeholder"/);
    expect(match, "应能找到 DSH 页面渲染分支").not.toBeNull();
    expect(match![1]).toBe('activeAgent === "dsh" && view === "workspace"');
    expect(page).toMatch(/\{:else if view === "settings"\}\s*<PiSettings/);
  });
});
