import { readFileSync } from "node:fs";
import { render } from "svelte/server";
import { beforeEach, describe, expect, it } from "vitest";
import AppUpdateProgressBar from "./AppUpdateProgressBar.svelte";
import type { AppUpdateProgress } from "./app-update";
import { setLocale } from "./i18n.svelte";

const page = readFileSync(new URL("../routes/+page.svelte", import.meta.url), "utf8");
const settings = readFileSync(new URL("./RuntimeSettings.svelte", import.meta.url), "utf8");

const piSettings = readFileSync(new URL("./PiSettings.svelte", import.meta.url), "utf8");
const navigation = readFileSync(new URL("./settings-navigation.ts", import.meta.url), "utf8");
/// 45% 下载态：12.3 MB / 27.4 MB 是契约里点名的对照文案。
function downloadProgress(overrides: Partial<AppUpdateProgress> = {}): AppUpdateProgress {
  return {
    phase: "downloading",
    percent: 45,
    estimated: false,
    transferred: Math.round(12.3 * 1024 * 1024),
    total: Math.round(27.4 * 1024 * 1024),
    ...overrides,
  };
}

beforeEach(() => {
  setLocale("zh-CN");
});

describe("应用更新进度条", () => {
  it("下载阶段显示真实百分比与已下载/总大小，进度条宽度就是百分比", () => {
    const { body } = render(AppUpdateProgressBar, { props: { progress: downloadProgress() } });
    expect(body).toContain("下载中 45% · 12.3 MB / 27.4 MB");
    expect(body).toContain("width: 45%");
    expect(body).not.toContain("indeterminate");
  });

  it("总量未知时退化成不确定动画条，不编造百分比", () => {
    const { body } = render(AppUpdateProgressBar, {
      props: { progress: downloadProgress({ percent: null, total: null }) },
    });
    expect(body).toContain("indeterminate");
    expect(body).toContain("已下载 12.3 MB");
    expect(body).not.toMatch(/width: \d+%/);
  });

  it("安装阶段显示按时间估算的百分比，并明确标注为估算", () => {
    const { body } = render(AppUpdateProgressBar, {
      props: { progress: downloadProgress({ phase: "installing", percent: 60, estimated: true }) },
    });
    expect(body).toContain("安装中 约 60%");
    expect(body).toContain("估算");
    // 安装阶段也要是确定宽度，而不是不确定动画条。
    expect(body).toContain("width: 60%");
    expect(body).not.toContain("indeterminate");
  });
});

describe("英文界面下的进度文案", () => {
  it("下载与安装的百分比、字节数和估算标注都取到译文", () => {
    setLocale("en");
    const installing = render(AppUpdateProgressBar, {
      props: { progress: downloadProgress({ phase: "installing", percent: 60, estimated: true }) },
    }).body;
    expect(installing).toContain("Installing ~60%");
    expect(installing).toContain("estimated");

    const downloading = render(AppUpdateProgressBar, { props: { progress: downloadProgress() } }).body;
    expect(downloading).toContain("Downloading 45% · 12.3 MB / 27.4 MB");
  });
});

describe("应用更新进度的接线", () => {
  it("主界面右下角浮层是唯一的更新界面，进度与安装动作都在这里", () => {
    expect(page).toContain('<AppUpdateProgressBar progress={appUpdateProgress} />');
    expect(page).toContain('class="app-update-float"');
    expect(page).toContain("onProgress: (progress) => { appUpdateProgress = progress; }");
    // 没有进度时浮层给的是「安装更新」入口，而不是只显示一行状态。
    expect(page).toContain('{#if appUpdateProgress || (appUpdate.status === "available" && !appUpdateDismissed)}');
    expect(page).toContain("onclick={() => void installDeepPiUpdate()}");
    expect(page).toContain('{t("安装更新")}');
    // 可以手动关掉，但下载/安装中的进度条不能被关掉。
    expect(page).toContain("appUpdateDismissed = true");
    expect(page).toContain("{#if !appUpdateProgress}");
    // 组件（运行时）更新的提示仍留在设置里，不新增主工具栏提示。
    expect(page).not.toContain("<RuntimeUpdateNotice");
    expect(page).not.toContain("runtime-updates-visible");
  });

  it("设置里不再有应用更新入口（「运行时与更新」分类已移除）", () => {
    expect(settings).not.toContain("AppUpdateProgressBar");
    expect(settings).not.toContain('t("检查应用更新")');
    expect(settings).not.toContain('t("安装更新")');
    expect(piSettings).not.toContain('view="app"');
    expect(navigation).not.toContain('id: "runtime"');
  });
});
