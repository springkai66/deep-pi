import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

// 无 jsdom 组件测试设施；沿用 provider-save-state.test.ts 的「源码不变式」：
// 用源码结构锁定官方登录入口的可见性规则——
// 1) PiProviderSettings 必须在模型设置里渲染 PiAuthSettings；
// 2) PiAuthSettings 只对 oauth=true 的 provider 渲染登录/退出按钮（纯 API Key provider 不出现）；
// 3) 登录面板必须提供手动粘贴输入与取消入口（浏览器授权为主 + 回贴兜底）。

const providerSettings = readFileSync("src/lib/PiProviderSettings.svelte", "utf8");
const authSettings = readFileSync("src/lib/PiAuthSettings.svelte", "utf8");

describe("pi auth settings wiring", () => {
  it("renders the official-login section inside the model settings detail panel", () => {
    expect(providerSettings).toContain('import PiAuthSettings from "./PiAuthSettings.svelte";');
    expect(providerSettings).toMatch(/<PiAuthSettings onError=\{onError\} \/>/);
    // 渲染位置在 detail-panel 内（模型设置详情列）。
    const panel = providerSettings.indexOf('<div class="detail-panel">');
    const component = providerSettings.indexOf("<PiAuthSettings");
    expect(panel).toBeGreaterThanOrEqual(0);
    expect(component).toBeGreaterThan(panel);
  });

  it("only shows sign-in actions for oauth-capable providers", () => {
    // 列表来自 pi_auth_providers，组件先按 oauth 过滤，再按 isSignedIn 分支按钮。
    expect(authSettings).toMatch(/providers = providerList\.filter\(\(provider\) => provider\.oauth\)/);
    expect(authSettings).toMatch(/\{#if isSignedIn\(provider\.id\)\}/);
    expect(authSettings).toMatch(/\{:else\}\s*\n\s*<button class="auth-button primary"/);
    // 登录按钮只在未登录分支出现，退出按钮只在已登录分支。
    const signIn = authSettings.indexOf('"官方登录"');
    const branch = authSettings.lastIndexOf("{#if isSignedIn(provider.id)}", signIn);
    expect(branch).toBeGreaterThan(0);
  });

  it("offers manual paste fallback and cancellation during login", () => {
    expect(authSettings).toContain('normalizePastedCode(manualValue)');
    expect(authSettings).toContain('"取消登录"');
    // 浏览器授权为主：授权链接展示 + 重新打开入口。
    expect(authSettings).toContain('"打开授权页"');
    expect(authSettings).toContain("plugin-opener");
  });

  it("reports credential status with expiry and triggers model reload", () => {
    // 状态标签带有效期；登录/退出完成后广播模型配置变更。
    expect(authSettings).toContain("formatValidityText(validityParts(");
    expect(authSettings).toContain("notifyModelsChanged()");
  });
});
