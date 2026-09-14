import { describe, expect, it } from "vitest";
import { SETTINGS_CATEGORIES, nextSettingsCategory, parseTaskLimit } from "./settings-navigation";

describe("settings navigation", () => {
  it("provides the planned categories with stable unique identifiers", () => {
    expect(SETTINGS_CATEGORIES.map((category) => category.id)).toEqual(["general", "appearance", "models", "runtime", "extensions", "mcp", "skills", "advanced"]);
    expect(new Set(SETTINGS_CATEGORIES.map((category) => category.id)).size).toBe(8);
  });
  it("supports arrows and boundary keys without capturing unrelated keys", () => {
    expect(nextSettingsCategory("general", "ArrowUp")).toBe("advanced");
    expect(nextSettingsCategory("advanced", "ArrowRight")).toBe("general");
    expect(nextSettingsCategory("models", "ArrowDown")).toBe("runtime");
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
