import { describe, expect, it } from "vitest";
import { dshShortcutCommand } from "./dsh-shortcuts";
import { hostCommandEnabled, type HostShortcutContext } from "./shortcuts";

describe("DSH 原生快捷键路由", () => {
  it("只接受三个 host 作用域命令", () => {
    expect(dshShortcutCommand("tasks")).toBe("tasks");
    expect(dshShortcutCommand("files")).toBe("files");
    expect(dshShortcutCommand("settings")).toBe("settings");
    for (const value of [
      "sidebar", "composer", "save", "saveAs", "nextDiff", "",
      "TASKS", "files ", 42, null, undefined, {}, ["files"], { command: "files" },
    ]) {
      expect(dshShortcutCommand(value)).toBeNull();
    }
  });

  it("DSH 焦点下这三个命令仍然可用，其他命令按作用域禁用", () => {
    const dshContext: HostShortcutContext = {
      pi: false, workspace: false, rpc: false, fileOpen: false, fileReady: false,
      fileBusy: false, navigationLocked: false, modal: false, recovery: false, closing: false,
    };
    for (const command of ["tasks", "files", "settings"] as const) {
      expect(hostCommandEnabled(command, dshContext)).toBe(true);
    }
    expect(hostCommandEnabled("sidebar", dshContext)).toBe(false);
    expect(hostCommandEnabled("composer", dshContext)).toBe(false);
    expect(hostCommandEnabled("save", dshContext)).toBe(false);
  });

  it("模态、重命名或退出流程中保持禁用", () => {
    const base: HostShortcutContext = {
      pi: false, workspace: false, rpc: false, fileOpen: false, fileReady: false,
      fileBusy: false, navigationLocked: false, modal: true, recovery: false, closing: false,
    };
    expect(hostCommandEnabled("files", base)).toBe(false);
    expect(hostCommandEnabled("files", { ...base, modal: false, recovery: true })).toBe(false);
    expect(hostCommandEnabled("files", { ...base, modal: false, closing: true })).toBe(false);
  });
});
