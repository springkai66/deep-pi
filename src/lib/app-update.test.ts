import { beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
const relaunch = vi.fn(async () => {});
vi.mock("@tauri-apps/plugin-updater", () => ({ check: (...args: unknown[]) => check(...args) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => relaunch() }));

const {
  DownloadProgressTracker,
  INSTALL_ESTIMATE_CEILING,
  InstallProgressEstimator,
  checkAppUpdate,
  describeUpdateError,
  formatBytes,
  installAppUpdate,
} = await import("./app-update");
type AppUpdateProgress = import("./app-update").AppUpdateProgress;
type DownloadEvent = import("./app-update").DownloadEvent;

beforeEach(() => { check.mockReset(); relaunch.mockClear(); });

describe("DeepPi self update", () => {
  it("checks with a bounded timeout and allows downgrades", async () => {
    check.mockResolvedValue(null);
    await checkAppUpdate();
    // 回滚场景下 stable 通道会指向更旧的版本，必须允许降级，否则回滚后无法再更新。
    expect(check).toHaveBeenCalledWith({ timeout: 15_000, allowDowngrades: true });
  });

  it("returns the available update without installing it", async () => {
    const update = { version: "1.0.1", downloadAndInstall: vi.fn() };
    check.mockResolvedValue(update);
    await expect(checkAppUpdate()).resolves.toBe(update);
    expect(update.downloadAndInstall).not.toHaveBeenCalled();
  });

  it("propagates check failures instead of reporting a silent 'up to date'", async () => {
    check.mockRejectedValue(new Error("Updater does not have any endpoints set."));
    await expect(checkAppUpdate()).rejects.toThrow("endpoints");
  });

  it("retries transport-level failures like 'error sending request'", async () => {
    // 连接被重置时 reqwest 只报「error sending request」，旊配不到旧重试正则；
    // 直连 GitHub 被间歇阻断或代理链路抖动时这类错误最常见，必须重试。
    check.mockRejectedValue(new Error(
      "error sending request for url (https://github.com/springkai66/deep-pi/releases/download/stable/latest.json)",
    ));
    await expect(checkAppUpdate()).rejects.toThrow("error sending request");
    expect(check).toHaveBeenCalledTimes(3);
  });

  it("downloads before installing, and does not relaunch when the install fails", async () => {
    const order: string[] = [];
    const update = {
      version: "1.0.1",
      download: vi.fn(async () => { order.push("download"); }),
      install: vi.fn(async () => { order.push("install"); }),
    };
    relaunch.mockImplementation(async () => { order.push("relaunch"); });
    await installAppUpdate(update as never);
    expect(order).toEqual(["download", "install", "relaunch"]);

    order.length = 0;
    const failing = {
      version: "1.0.1",
      download: vi.fn(async () => { order.push("download"); }),
      install: vi.fn(async () => { order.push("install"); throw new Error("disk full"); }),
    };
    await expect(installAppUpdate(failing as never)).rejects.toThrow("disk full");
    expect(order).toEqual(["download", "install"]);
  });

  describe("describeUpdateError", () => {
    it("appends proxy guidance to transport-level failures, keeping the original detail", () => {
      const message = describeUpdateError(
        new Error("error sending request for url (https://github.com/springkai66/deep-pi/releases/download/stable/latest.json)"),
      );
      expect(message).toContain("无法连接更新服务器");
      expect(message).toContain("error sending request for url");
      expect(message).toContain("手动");
    });

    it("passes non-network errors through unchanged (signature/manifest issues)", () => {
      expect(describeUpdateError("signature verification failed"))
        .toBe("signature verification failed");
      expect(describeUpdateError("Updater does not have any endpoints set."))
        .toBe("Updater does not have any endpoints set.");
    });
  });

  describe("下载进度", () => {
    it("按 contentLength 与 chunkLength 累加出真实百分比", () => {
      const tracker = new DownloadProgressTracker();
      expect(tracker.apply({ event: "Started", data: { contentLength: 1000 } }).percent).toBe(0);
      expect(tracker.apply({ event: "Progress", data: { chunkLength: 250 } }).percent).toBe(25);
      const half = tracker.apply({ event: "Progress", data: { chunkLength: 250 } });
      expect(half.percent).toBe(50);
      expect(half.transferred).toBe(500);
      expect(half.total).toBe(1000);
      expect(half.estimated).toBe(false);
    });

    it("收尾时直接落到 100%，不会因为取整停在 99%", () => {
      const tracker = new DownloadProgressTracker();
      tracker.apply({ event: "Started", data: { contentLength: 1000 } });
      expect(tracker.apply({ event: "Progress", data: { chunkLength: 999 } }).percent).toBe(99);
      expect(tracker.apply({ event: "Finished" }).percent).toBe(100);
    });

    it("服务端没给总量时保持不确定态，但已下载字节照实上报", () => {
      const tracker = new DownloadProgressTracker();
      expect(tracker.apply({ event: "Started", data: {} }).percent).toBeNull();
      expect(tracker.apply({ event: "Progress", data: { chunkLength: 100 } }).percent).toBeNull();
      const finished = tracker.apply({ event: "Finished" });
      expect(finished.percent).toBeNull();
      expect(finished.total).toBeNull();
      expect(finished.transferred).toBe(100);
    });

    it("安装流程里先报下载进度，下载结束后才进入安装阶段", async () => {
      const seen: AppUpdateProgress[] = [];
      const update = {
        version: "1.0.1",
        download: vi.fn(async (onEvent?: (event: DownloadEvent) => void) => {
          onEvent?.({ event: "Started", data: { contentLength: 1000 } });
          onEvent?.({ event: "Progress", data: { chunkLength: 250 } });
          onEvent?.({ event: "Progress", data: { chunkLength: 250 } });
        }),
        install: vi.fn(async () => {}),
      };
      await installAppUpdate(update as never, { onProgress: (progress) => seen.push(progress) });

      const downloading = seen.filter((progress) => progress.phase === "downloading");
      expect(seen[0].phase).toBe("downloading");
      expect(downloading.at(-1)?.percent).toBe(50);
      expect(downloading.at(-1)?.transferred).toBe(500);
      const installing = seen.find((progress) => progress.phase === "installing");
      expect(installing).toBeDefined();
      expect(seen.indexOf(installing!) ).toBeGreaterThan(seen.indexOf(downloading.at(-1)!));
      expect(update.install).toHaveBeenCalledTimes(1);
    });
  });

  describe("安装阶段的时间估算", () => {
    it("按已过去的时间线性推进，单调不减且封顶 99%", () => {
      const estimator = new InstallProgressEstimator(750);
      expect(estimator.percent(0)).toBe(0);
      estimator.start(1_000);
      expect(estimator.percent(1_000)).toBe(0);
      expect(estimator.percent(1_750)).toBe(1);
      expect(estimator.percent(1_000_000)).toBe(INSTALL_ESTIMATE_CEILING);
      estimator.stop();
      expect(estimator.percent(1_000_000)).toBe(0);
    });

    it("系统时钟倒退时已显示的进度不倒退", () => {
      const estimator = new InstallProgressEstimator(750);
      estimator.start(1_000);
      expect(estimator.percent(9_000)).toBe(10);
      // 对时或休眠恢复可能把 now 拉回过去，此时不能倒退。
      expect(estimator.percent(500)).toBe(10);
      expect(estimator.percent(0)).toBe(10);
    });

    it("安装阶段进度带 estimated 标记，且随时间只增不减、不超过 99%", async () => {
      vi.useFakeTimers();
      try {
        const seen: AppUpdateProgress[] = [];
        const finishInstall: Array<() => void> = [];
        const update = {
          version: "1.0.1",
          download: vi.fn(async () => {}),
          install: vi.fn(() => new Promise<void>((resolve) => { finishInstall.push(resolve); })),
        };
        const pending = installAppUpdate(update as never, {
          onProgress: (progress) => seen.push(progress),
          tickMs: 100,
        });
        await vi.advanceTimersByTimeAsync(1);
        await vi.advanceTimersByTimeAsync(10 * 60_000);
        for (const resolve of finishInstall) resolve();
        await pending;

        const installing = seen.filter((progress) => progress.phase === "installing");
        expect(installing.length).toBeGreaterThan(1);
        expect(installing.every((progress) => progress.estimated)).toBe(true);
        const percents = installing.map((progress) => progress.percent ?? -1);
        expect(percents).toEqual([...percents].sort((a, b) => a - b));
        expect(percents[0]).toBe(0);
        expect(percents.at(-1)).toBe(INSTALL_ESTIMATE_CEILING);
      } finally {
        vi.useRealTimers();
      }
    });

    it("安装结束后不再上报进度（定时器已清理）", async () => {
      vi.useFakeTimers();
      try {
        const seen: AppUpdateProgress[] = [];
        const update = { version: "1.0.1", download: vi.fn(async () => {}), install: vi.fn(async () => {}) };
        await installAppUpdate(update as never, {
          onProgress: (progress) => seen.push(progress),
          tickMs: 100,
        });
        const reported = seen.length;
        await vi.advanceTimersByTimeAsync(5_000);
        expect(seen.length).toBe(reported);
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe("formatBytes", () => {
    it("按 1024 进制给出可对照的字节文案", () => {
      expect(formatBytes(0)).toBe("0 B");
      expect(formatBytes(512)).toBe("512 B");
      expect(formatBytes(1536)).toBe("1.5 KB");
      expect(formatBytes(Math.round(12.3 * 1024 * 1024))).toBe("12.3 MB");
      expect(formatBytes(Math.round(27.4 * 1024 * 1024))).toBe("27.4 MB");
      expect(formatBytes(null)).toBe("0 B");
    });
  });
});
