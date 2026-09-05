import { describe, expect, it } from "vitest";
import { changePaneCapacity, hasProjectConflict, openPane } from "./workspace";

const available = ["task-1", "task-2", "task-3", "task-4"];

describe("hasProjectConflict", () => {
  it("matches Windows paths case-insensitively", () => {
    expect(
      hasProjectConflict("F:/Repo/", [
        { projectPath: "f:\\repo", status: "waiting" },
        { projectPath: "F:\\other", status: "running" },
      ]),
    ).toBe(true);
  });

  it("ignores terminal task statuses", () => {
    expect(hasProjectConflict("F:/repo", [{ projectPath: "F:/repo", status: "completed" }])).toBe(false);
  });
});

describe("openPane", () => {
  it("replaces the active pane in single mode", () => {
    expect(openPane({ current: ["task-1"], active: "task-1", requested: "task-2", available, capacity: 1 })).toEqual({
      panes: ["task-2"],
      active: "task-2",
    });
  });

  it("adds a second pane without discarding the first", () => {
    expect(openPane({ current: ["task-1"], active: "task-1", requested: "task-2", available, capacity: 2 })).toEqual({
      panes: ["task-1", "task-2"],
      active: "task-2",
    });
  });
});

describe("changePaneCapacity", () => {
  it("keeps the active pane when reducing capacity", () => {
    expect(changePaneCapacity({ current: ["task-1", "task-2"], active: "task-2", available, capacity: 1 })).toEqual({
      panes: ["task-2"],
      active: "task-2",
    });
  });

  it("fills newly available panes in stable task order", () => {
    expect(changePaneCapacity({ current: ["task-2"], active: "task-2", available, capacity: 4 })).toEqual({
      panes: ["task-2", "task-1", "task-3", "task-4"],
      active: "task-2",
    });
  });
});
