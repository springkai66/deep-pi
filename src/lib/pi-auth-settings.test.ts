import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

// 无 jsdom 组件测试设施；沿用「源码不变式」锁定官方登录入口的可见性与兜底交互：
// 1) PiProviderSettings 在「官方供应商」分组里渲染 PiAuthSettings，并接上模型目录入口；
// 2) 官方分组只列已登录的官方供应商（默认空），不再平铺所有可用通道；
// 3) 登录流程组件（PiOfficialLoginFlow）提供手动粘贴与取消入口（浏览器授权为主 + 回贴兜底）；
// 4) 凭据状态带有效期；登录/退出后广播模型配置变更。
const providerSettings = readFileSync("src/lib/PiProviderSettings.svelte", "utf8");
const authSettings = readFileSync("src/lib/PiAuthSettings.svelte", "utf8");
const loginFlow = readFileSync("src/lib/PiOfficialLoginFlow.svelte", "utf8");

describe("pi auth settings wiring", () => {
  it("renders the official-provider group inside the model settings detail panel", () => {
    expect(providerSettings).toContain('import PiAuthSettings from "./PiAuthSettings.svelte";');
    expect(providerSettings).toMatch(/<PiAuthSettings onError=\{onError\} refreshToken=\{authRefreshToken\}/);
    // 渲染位置在 detail-panel 内（模型设置详情列）。
    const panel = providerSettings.indexOf('<div class="detail-panel">');
    const component = providerSettings.indexOf("<PiAuthSettings");
    expect(panel).toBeGreaterThanOrEqual(0);
    expect(component).toBeGreaterThan(panel);
  });

  it("only lists signed-in official providers, keyed by oauth credentials", () => {
    expect(authSettings).toMatch(/providers = providerList\.filter\(\(provider\) => provider\.oauth\)/);
    expect(authSettings).toContain("const signedInProviders = $derived(");
    expect(authSettings).toContain('credentials[provider.id]?.authType === "oauth"');
    expect(authSettings).toMatch(/\{#each signedInProviders as provider \(provider\.id\)\}/);
  });

  it("offers manual paste fallback and cancellation during login", () => {
    expect(loginFlow).toContain("normalizePastedCode(manualValue)");
    expect(loginFlow).toContain('"取消登录"');
    // 浏览器授权为主：授权链接展示 + 重新打开入口。
    expect(loginFlow).toContain('"打开授权页"');
    expect(loginFlow).toContain("plugin-opener");
    // 设备码通道同样可用（github-copilot / kimi-coding / radius / xai 的默认流程）。
    expect(loginFlow).toContain('"复制设备码"');
  });

  it("reports credential status with expiry and triggers model reload", () => {
    // 状态标签带有效期；登录/退出完成后广播模型配置变更。
    expect(authSettings).toContain("formatValidityText(validityParts(");
    expect(authSettings).toContain("notifyModelsChanged()");
    expect(loginFlow).toContain("notifyModelsChanged()");
  });
});
