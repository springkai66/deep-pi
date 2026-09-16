import { describe, expect, it } from "vitest";
import { SETTINGS_CATEGORIES, nextSettingsCategory, parseTaskLimit, settingsGroups } from "./settings-navigation";

describe("settings navigation", () => {
  it("provides the planned categories with stable unique identifiers", () => {
    expect(SETTINGS_CATEGORIES.map((category) => category.id)).toEqual([
      "general", "appearance", "models", "extensions", "mcp", "skills", "dsh", "runtime", "advanced",
    ]);
    expect(new Set(SETTINGS_CATEGORIES.map((category) => category.id)).size).toBe(9);
  });

  it("separates Pi and DSH categories into their own navigation groups", () => {
    const groups = settingsGroups();
    expect(groups.map((group) => group.label)).toEqual(["应用", "Pi Coding Agent", "DSH (DeepSeek Harness)", "系统"]);
    const dshGroup = groups.find((group) => group.label === "DSH (DeepSeek Harness)");
    expect(dshGroup?.categories.map((category) => category.id)).toEqual(["dsh"]);
    const piGroup = groups.find((group) => group.label === "Pi Coding Agent");
    expect(piGroup?.categories.map((category) => category.id)).toEqual(["models", "extensions", "mcp", "skills"]);
  });

  it("supports arrows and boundary keys without capturing unrelated keys", () => {
    expect(nextSettingsCategory("general", "ArrowUp")).toBe("advanced");
    expect(nextSettingsCategory("advanced", "ArrowRight")).toBe("general");
    expect(nextSettingsCategory("models", "ArrowDown")).toBe("extensions");
    expect(nextSettingsCategory("skills", "ArrowDown")).toBe("dsh");
    expect(nextSettingsCategory("runtime", "Home")).toBe("general");
    expect(nextSettingsCategory("runtime", "End")).toBe("advanced");
    expect(nextSettingsCategory("general", "p")).toBeNull();
  });

  it("accepts only complete integer task limits matching backend bounds", () => {
    for (const value of ["", "0", "17", "1.5", "-1", "NaN", "1e1", " "]) expect(parseTaskLimit(value)).toBeNull();
    expect(parseTaskLimit("1")).toBe(1);
    expect(parseTaskLimit("16")).toBe(16);
  });
});
