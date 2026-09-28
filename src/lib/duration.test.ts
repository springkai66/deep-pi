import { describe, expect, it } from "vitest";
import { formatDuration } from "./duration";

/** 格式必须与 pi TUI（dist/core/tools/renderers/bash.js 的 formatDuration）逐字一致。 */
describe("formatDuration", () => {
  it("renders sub-minute durations with one decimal second", () => {
    expect(formatDuration(0)).toBe("0.0s");
    expect(formatDuration(250)).toBe("0.3s");
    expect(formatDuration(42_000)).toBe("42.0s");
    expect(formatDuration(59_900)).toBe("59.9s");
  });

  it("switches to minutes at 60s", () => {
    expect(formatDuration(60_000)).toBe("1m 0s");
    expect(formatDuration(61_000)).toBe("1m 1s");
    expect(formatDuration(125_400)).toBe("2m 5s");
  });

  it("switches to hours at 60m", () => {
    expect(formatDuration(3_600_000)).toBe("1h 0m 0s");
    expect(formatDuration(3_723_000)).toBe("1h 2m 3s");
  });
});
