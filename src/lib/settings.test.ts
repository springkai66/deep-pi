import { describe, expect, it } from "vitest";
import { DEFAULT_APP_SETTINGS } from "./settings";

describe("managed workspace defaults", () => {
  it("uses only the DeepPi environment for new tasks", () => {
    expect(DEFAULT_APP_SETTINGS.piEnvironment).toBe("managed");
  });
});
