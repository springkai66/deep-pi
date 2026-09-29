import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const service = readFileSync(new URL("./DshSettings.svelte", import.meta.url), "utf8");
const failure = readFileSync(new URL("./DshFailure.svelte", import.meta.url), "utf8");
const runtime = readFileSync(new URL("./RuntimeSettings.svelte", import.meta.url), "utf8");
const settings = readFileSync(new URL("./PiSettings.svelte", import.meta.url), "utf8");
const messages = readFileSync(new URL("./app-messages-runtime.ts", import.meta.url), "utf8");
const page = readFileSync(new URL("../routes/+page.svelte", import.meta.url), "utf8");

describe("DSH recovery controls", () => {
  it("keeps runtime components on their respective settings pages", () => {
    expect(settings).toContain('<RuntimeSettings {...runtime} view="pi" />');
    expect(settings).toContain('<RuntimeSettings {...runtime} view="dsh" />');
    expect(settings).toContain('<RuntimeSettings {...runtime} view="app" />');
    expect(runtime).toContain('runtime.id === "node" || runtime.id === "pi"');
    expect(runtime).toContain('runtime.id === "dsh" || runtime.id === "dshmarket"');
  });

  it("shows plugin status and removal only on the startup failure page", () => {
    expect(page).toContain("<DshFailure {dshRunning} {busyRuntime}");
    expect(failure).toContain('invoke<DshPlugin[]>("dsh_plugins")');
    expect(failure).toContain('invoke<DshDiagnosis>("dsh_diagnose")');
    expect(failure).toContain('diagnosis?.hasFailure && diagnosis.culprits.includes(plugin.name) && !plugin.official');
    expect(failure).toContain('suspected ? t("疑似不兼容") : t("未验证")');
    expect(failure).toContain("await confirm(");
    expect(failure).toContain('"dsh_repair", { request: { package: plugin.name } }');
    expect(service).not.toContain('t("移除插件")');
    expect(runtime).not.toContain('t("DSH 预发布版本：插件可能尚未适配，请确认兼容性后安装。")');
    expect(messages).toMatch(/"runtime\.dsh\.npm_only_warning": \{\s+"zh-CN": "仅 npm 已发布的 DSH 版本可安装/);
    expect(settings).not.toContain("<DshFailure");
    expect(service).not.toContain("onResetDsh");
    expect(page).not.toContain("resetAndReinstallDsh");
  });

  it("retains component install and uninstall with operation state", () => {
    expect(runtime).toContain('onUpdateRuntime(update)}><Download size={14} />{t("安装")}');
    expect(runtime).toContain("onUninstallRuntime(runtime)");
    expect(runtime).toContain("runtimeProgress.percent");
    expect(page).toContain('await invoke("uninstall_runtime", { componentId: runtime.id })');
  });
});
