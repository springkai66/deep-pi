import { describe, expect, it, vi } from "vitest";
import { DEFAULT_APP_SETTINGS, type AppSettings } from "./settings";
import { createSettingsSaver } from "./settings-save";
import { createWindowCloseHandler } from "./window-close";

describe("settings persistence", () => {
  it("serializes writes and coalesces pending full snapshots", async () => {
    let finish!: () => void;
    const save = vi.fn().mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; })).mockResolvedValue(undefined);
    const saver = createSettingsSaver(save, vi.fn(), vi.fn());
    saver.enqueue({ ...DEFAULT_APP_SETTINGS, colorMode: "light" });
    saver.enqueue({ ...DEFAULT_APP_SETTINGS, colorMode: "dark" });
    saver.enqueue({ ...DEFAULT_APP_SETTINGS, colorMode: "dark", appFontName: "Microsoft YaHei UI" });
    expect(save).toHaveBeenCalledTimes(1);
    finish();
    await saver.flush();
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][0]).toMatchObject({ colorMode: "dark", appFontName: "Microsoft YaHei UI" });
  });
  it("copies mutable nested update preferences before asynchronous saving", async () => {
    const save = vi.fn(async (_settings: AppSettings) => {});
    const saver = createSettingsSaver(save, vi.fn(), vi.fn());
    const settings = { ...DEFAULT_APP_SETTINGS, skippedUpdates: { pi: "1.0" } };
    saver.enqueue(settings);
    settings.skippedUpdates.pi = "2.0";
    await saver.flush();
    expect(save.mock.calls[0][0]).toMatchObject({ skippedUpdates: { pi: "1.0" } });
  });
  it("reports failure and retries only on an explicit flush", async () => {
    const save = vi.fn().mockRejectedValueOnce(new Error("disk unavailable")).mockResolvedValue(undefined);
    const error = vi.fn();
    const saver = createSettingsSaver(save, error, vi.fn());
    saver.enqueue(DEFAULT_APP_SETTINGS);
    await Promise.resolve();
    await Promise.resolve();
    expect(save).toHaveBeenCalledTimes(1);
    expect(error).toHaveBeenCalledTimes(1);
    await saver.flush();
    expect(save).toHaveBeenCalledTimes(2);
  });
  it("refuses to finish a flush while the current value is still unsaved", async () => {
    const save = vi.fn(async () => { throw new Error("read only"); });
    const saver = createSettingsSaver(save, vi.fn(), vi.fn());
    saver.enqueue(DEFAULT_APP_SETTINGS);
    await expect(saver.flush()).rejects.toThrow("read only");
  });
  it("drains a new value queued while the preceding batch becomes idle", async () => {
    const save = vi.fn(async (_settings: AppSettings) => {});
    let added = false;
    const saver = createSettingsSaver(save, vi.fn(), (busy) => {
      if (!busy && !added) { added = true; saver.enqueue({ ...DEFAULT_APP_SETTINGS, colorMode: "dark" }); }
    });
    saver.enqueue({ ...DEFAULT_APP_SETTINGS, colorMode: "light" });
    await saver.flush();
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][0].colorMode).toBe("dark");
  });
  it("waits for persistence before stopping tasks or destroying the window", async () => {
    let finish!: () => void;
    const saver = createSettingsSaver(() => new Promise<void>((resolve) => { finish = resolve; }), vi.fn(), vi.fn());
    const destroy = vi.fn();
    const stopPi = vi.fn(async () => {});
    const stopDsh = vi.fn(async () => {});
    const close = createWindowCloseHandler({
      behavior: () => "exit", blocked: () => false, hasActiveTasks: () => false,
      choose: async () => null, confirm: async () => true, remember: () => {},
      minimize: async () => {}, stopPi, stopDsh,
      prepareExit: async () => { await saver.flush(); return true; },
      destroy: async () => { destroy(); }, error: vi.fn(),
    });
    saver.enqueue(DEFAULT_APP_SETTINGS);
    const closing = close({ preventDefault: vi.fn() });
    await Promise.resolve();
    expect(stopPi).not.toHaveBeenCalled();
    expect(stopDsh).not.toHaveBeenCalled();
    expect(destroy).not.toHaveBeenCalled();
    finish();
    await closing;
    expect(stopPi).toHaveBeenCalledOnce();
    expect(stopDsh).toHaveBeenCalledOnce();
    expect(destroy).toHaveBeenCalledTimes(1);
  });
});
