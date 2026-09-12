import { beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
const relaunch = vi.fn(async () => {});
vi.mock("@tauri-apps/plugin-updater", () => ({ check: (...args: unknown[]) => check(...args) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => relaunch() }));

const { checkAppUpdate, installAppUpdate } = await import("./app-update");

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

  it("installs before relaunching, and does not relaunch when the install fails", async () => {
    const order: string[] = [];
    const update = { version: "1.0.1", downloadAndInstall: vi.fn(async () => { order.push("install"); }) };
    relaunch.mockImplementation(async () => { order.push("relaunch"); });
    await installAppUpdate(update as never);
    expect(order).toEqual(["install", "relaunch"]);

    order.length = 0;
    const failing = { version: "1.0.1", downloadAndInstall: vi.fn(async () => { order.push("install"); throw new Error("disk full"); }) };
    await expect(installAppUpdate(failing as never)).rejects.toThrow("disk full");
    expect(order).toEqual(["install"]);
  });
});
