import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const settingsSource = readFileSync("src/lib/PiProviderSettings.svelte", "utf8");
const providerSource = readFileSync("src-tauri/src/provider.rs", "utf8");

describe("provider proxy configuration", () => {
  it("exposes a visible model proxy setting with an enabled state", () => {
    expect(settingsSource).toMatch(/\{t\("模型代理"\)\}/);
    expect(settingsSource).toMatch(/bind:value=\{draft\.proxy\}/);
    expect(settingsSource).toMatch(/class="proxy-state"/);
    expect(settingsSource).toMatch(/留空则直连/);
  });

  it("validates, persists, and applies the proxy to provider requests", () => {
    expect(providerSource).toMatch(/validate_proxy\(request\.proxy\.as_deref\(\)\)/);
    expect(providerSource).toMatch(/provider\.insert\("proxy"\.into\(\), Value::String\(proxy\.clone\(\)\)\)/);
    expect(providerSource).toMatch(/config = config\.proxy\(Some\(proxy\)\)/);
    expect(providerSource).toMatch(/fn chat_agent\(/);
  });
});
