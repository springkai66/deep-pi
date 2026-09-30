import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { render } from "svelte/server";

import RuntimeSettings from "./RuntimeSettings.svelte";
import type { RuntimeComponent } from "./runtime";
import { DEFAULT_APP_SETTINGS } from "./settings";

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
    // 「运行时与更新」分类已移除：设置里不再有 DeepPi 应用更新入口。
    expect(settings).not.toContain('view="app"');
    expect(runtime).toContain('runtime.id === "node" || runtime.id === "pi"');
    expect(runtime).toContain('runtime.id === "dsh" || runtime.id === "dshmarket"');
  });

  it("leaves process start/stop out of settings", () => {
    // 启动/重启 DSH 与重启 Pi 任务都属于工作区操作，设置里不再提供按钮。
    expect(service).not.toContain('t("启动")');
    expect(service).not.toContain('t("重启")');
    expect(service).not.toContain('t("停止")');
    expect(service).not.toContain("stop_dsh");
    expect(runtime).not.toContain('t("运行服务")');
    expect(runtime).not.toContain('t("重启全部任务")');
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

  it("renders the trimmed component panel for both views", () => {
    const props = {
      settings: { ...DEFAULT_APP_SETTINGS },
      runtimes: [
        { id: "pi", name: "Pi Coding Agent", currentVersion: "1.0.0", source: "managed", available: true, installed: true },
        { id: "dsh", name: "DeepSeek Harness", currentVersion: "1.0.0", source: "managed", available: true, installed: true },
      ] satisfies RuntimeComponent[],
      updates: [],
      isCheckingUpdates: false,
      busyRuntime: null,
      runtimeOperation: null,
      runtimeProgress: null,
      onCancelRuntime: () => {},
      onCheckUpdates: () => {},
      onUpdateRuntime: () => {},
      onUninstallRuntime: () => {},
      runningPiCount: 0,
      dshRunning: false,
    };
    for (const view of ["pi", "dsh"] as const) {
      const body = render(RuntimeSettings, { props: { ...props, view } }).body;
      expect(body).toContain("组件");
      expect(body).toContain("检查组件更新");
      // 启动/停止与应用更新都不再出现在设置里。
      expect(body).not.toContain("重启全部任务");
      expect(body).not.toContain("运行服务");
      expect(body).not.toContain("应用更新");
    }
  });
});
