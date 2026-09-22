import { beforeEach, describe, expect, it, vi } from "vitest";

const check = vi.fn();
const relaunch = vi.fn(async () => {});
vi.mock("@tauri-apps/plugin-updater", () => ({ check: (...args: unknown[]) => check(...args) }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => relaunch() }));

const { checkAppUpdate, describeUpdateError, installAppUpdate } = await import("./app-update");

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
});
