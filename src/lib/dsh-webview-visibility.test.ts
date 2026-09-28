import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

// 原生 DSH 子 WebView 永远盖在主窗口的 HTML 之上：只要 DSH 处于激活状态，
// 打开设置时必须隐藏原生视图，让 HTML 覆盖层可以操作；关闭设置后由显隐 effect 恢复。
const page = readFileSync("src/routes/+page.svelte", "utf8");

describe("DSH 原生子 WebView 的可见性", () => {
  it("设置覆盖层打开时必须隐藏原生 WebView", () => {
    const match = page.match(/const visible = activeAgent[^;]*;/);
    expect(match, "应能找到 DSH 可见性条件").not.toBeNull();
    const condition = match![0];
    expect(condition).toContain('activeAgent === "dsh"');
    // 关键：设置覆盖层打开时隐藏，避免原生视图盖住 HTML。
    expect(condition).toContain('view === "workspace"');
    expect(condition).toContain("!settingsOpen");
    expect(condition).toContain("!dialogRequest");
    expect(condition).toContain("!closingWindow");
    expect(condition).toContain("!recoveryProject");
  });

  it("异步创建完成后的首次显隐判断同样考虑设置覆盖层", () => {
    const match = page.match(/if \(activeAgent !== "dsh"[^)]*\) await dshWebview\.hide\(\);/);
    expect(match, "应能找到创建 WebView 后的显隐判断").not.toBeNull();
    expect(match![0]).toContain("settingsOpen");
  });

  it("设置作为覆盖层显示，底层 DSH 工作区保持挂载", () => {
    const match = page.match(/\{#if ([^}]+)\}\s*<div class="dsh-placeholder"/);
    expect(match, "应能找到 DSH 工作区渲染分支").not.toBeNull();
    expect(match![1]).toBe('activeAgent === "dsh" && view === "workspace"');
    expect(page).toContain("{#if settingsOpen}");
    expect(page).toContain('<div class="settings-modal"');
    expect(page).toContain("inert={closingWindow || startupPending || !!startupFailure || settingsOpen}");
  });
});
