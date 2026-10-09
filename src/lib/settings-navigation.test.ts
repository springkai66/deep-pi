import { describe, expect, it } from "vitest";
import { SETTINGS_CATEGORIES, filterSettingsCategories, nextSettingsCategory, parseTaskLimit, settingsGroups } from "./settings-navigation";

describe("settings navigation", () => {
  it("provides the planned categories with stable unique identifiers", () => {
    expect(SETTINGS_CATEGORIES.map((category) => category.id)).toEqual([
      "general", "appearance", "pi", "models", "extensions", "mcp", "skills", "workflows", "dsh", "network", "advanced",
    ]);
    expect(new Set(SETTINGS_CATEGORIES.map((category) => category.id)).size).toBe(11);
  });

  it("separates Pi and DSH categories into their own navigation groups", () => {
    const groups = settingsGroups();
    expect(groups.map((group) => group.label)).toEqual(["应用", "Pi Coding Agent", "DSH (DeepSeek Harness)", "系统"]);
    const dshGroup = groups.find((group) => group.label === "DSH (DeepSeek Harness)");
    expect(dshGroup?.categories.map((category) => category.id)).toEqual(["dsh"]);
    const piGroup = groups.find((group) => group.label === "Pi Coding Agent");
    expect(piGroup?.categories.map((category) => category.id)).toEqual(["pi", "models", "extensions", "mcp", "skills", "workflows"]);
  });
  it("filters setting labels and indexed controls without changing category order", () => {
    expect(filterSettingsCategories("  代理地址 ")).toEqual(["network"]);
    expect(filterSettingsCategories("任务环半径")).toEqual(["general"]);
    expect(filterSettingsCategories("会话字体大小")).toEqual(["appearance"]);
    expect(filterSettingsCategories("磨玻璃透明程度")).toEqual(["appearance"]);
    expect(filterSettingsCategories("Provider")).toEqual(["models"]);
    expect(filterSettingsCategories("Pi 资源包")).toEqual(["extensions"]);
    expect(filterSettingsCategories("提示模板")).toEqual(["extensions"]);
    expect(filterSettingsCategories("Codemode")).toEqual(["pi"]);
    expect(filterSettingsCategories("默认启动")).toEqual(["pi"]);
    expect(filterSettingsCategories("内联预算")).toEqual(["pi"]);
    expect(filterSettingsCategories("外部编辑器")).toEqual(["general"]);
    expect(filterSettingsCategories("Pi RPC")).toEqual([]);
    expect(filterSettingsCategories("没有对应项")).toEqual([]);
    expect(filterSettingsCategories("关于")).toEqual(["advanced"]);
    expect(filterSettingsCategories("about", (text) => text === "关于" ? "About" : text)).toEqual(["advanced"]);
    expect(filterSettingsCategories("pi coding agent")).toEqual(["pi", "models", "extensions", "mcp", "skills", "workflows"]);
    expect(nextSettingsCategory("models", "ArrowDown", ["models", "advanced"])).toBe("advanced");
    expect(nextSettingsCategory("models", "Home", [])).toBeNull();
  });

  it("supports arrows and boundary keys without capturing unrelated keys", () => {
    expect(nextSettingsCategory("general", "ArrowUp")).toBe("advanced");
    expect(nextSettingsCategory("advanced", "ArrowRight")).toBe("general");
    expect(nextSettingsCategory("models", "ArrowDown")).toBe("extensions");
    expect(nextSettingsCategory("skills", "ArrowDown")).toBe("workflows");
    expect(nextSettingsCategory("workflows", "ArrowDown")).toBe("dsh");
    expect(nextSettingsCategory("network", "Home")).toBe("general");
    expect(nextSettingsCategory("network", "End")).toBe("advanced");
    expect(nextSettingsCategory("general", "p")).toBeNull();
  });

  it("accepts only complete integer task limits matching backend bounds", () => {
    for (const value of ["", "0", "17", "1.5", "-1", "NaN", "1e1", " "]) expect(parseTaskLimit(value)).toBeNull();
    expect(parseTaskLimit("1")).toBe(1);
    expect(parseTaskLimit("16")).toBe(16);
  });
});
