import { describe, expect, it } from "vitest";
import {
  computeLineDiff,
  extractFileChange,
  trailingNewlineDiffers,
} from "./file-change-diff";

describe("extractFileChange", () => {
  it("extracts an edit change from path + oldText/newText", () => {
    expect(
      extractFileChange("edit", { path: "src/a.ts", oldText: "const a = 1;", newText: "const a = 2;" }),
    ).toEqual({ path: "src/a.ts", tool: "edit", before: "const a = 1;", after: "const a = 2;" });
  });

  it("falls back to file_path and tolerates case differences in tool name", () => {
    expect(
      extractFileChange("Edit", { file_path: "a.py", oldText: "x", newText: "y" }),
    ).toEqual({ path: "a.py", tool: "edit", before: "x", after: "y" });
  });

  it("extracts a write change with empty before", () => {
    expect(extractFileChange("write", { path: "b.md", content: "hello" })).toEqual({
      path: "b.md",
      tool: "write",
      before: "",
      after: "hello",
    });
  });

  it("returns null for non-file tools and incomplete arguments", () => {
    expect(extractFileChange("bash", { command: "rm -rf" })).toBeNull();
    expect(extractFileChange("edit", { path: "a.ts" })).toBeNull();
    expect(extractFileChange("edit", { path: "a.ts", oldText: "x" })).toBeNull();
    expect(extractFileChange("write", { path: "b.md" })).toBeNull();
    expect(extractFileChange("edit", "not-an-object")).toBeNull();
    expect(extractFileChange("edit", { path: "a.ts", oldText: 1, newText: 2 })).toBeNull();
  });
});

describe("computeLineDiff", () => {
  it("reports equality for identical content", () => {
    const diff = computeLineDiff("a\nb\nc", "a\nb\nc");
    expect(diff.equal).toBe(true);
    expect(diff.added).toBe(0);
    expect(diff.removed).toBe(0);
  });

  it("flags newline-style-only differences", () => {
    const diff = computeLineDiff("a\r\nb\r\n", "a\nb\n");
    expect(diff.newlineOnly).toBe(true);
    expect(diff.rows).toEqual([]);
  });

  it("computes a middle-line modification with context rows", () => {
    const diff = computeLineDiff("a\nb\nc", "a\nX\nc");
    expect(diff.added).toBe(1);
    expect(diff.removed).toBe(1);
    const kinds = diff.rows.map((row) => row.kind);
    expect(kinds).toContain("del");
    expect(kinds).toContain("add");
    expect(diff.rows[0]).toMatchObject({ kind: "ctx", text: "a", oldNo: 1, newNo: 1 });
  });

  it("handles pure additions and pure deletions", () => {
    const added = computeLineDiff("a", "a\nb\nc");
    expect(added.added).toBe(2);
    expect(added.removed).toBe(0);
    const removed = computeLineDiff("a\nb\nc", "a");
    expect(removed.added).toBe(0);
    expect(removed.removed).toBe(2);
  });

  it("treats a missing trailing newline as a removed empty line", () => {
    const diff = computeLineDiff("a\n", "a");
    expect(diff.removed).toBe(1);
    expect(diff.added).toBe(0);
    expect(trailingNewlineDiffers("a\n", "a")).toBe(true);
    expect(trailingNewlineDiffers("a", "a")).toBe(false);
  });

  it("collapses long unchanged context runs into gap rows", () => {
    const before = Array.from({ length: 40 }, (_, i) => `ctx ${i}`).join("\n");
    const after = before.replace("ctx 20", "CHANGED");
    const diff = computeLineDiff(before, after);
    expect(diff.added).toBe(1);
    expect(diff.removed).toBe(1);
    const gaps = diff.rows.filter((row) => row.kind === "gap");
    expect(gaps.length).toBeGreaterThan(0);
    // 41 行差异行（38 ctx + 1 del + 1 add），保留改动簇前后 3 行上下文
    //（8 行，其中 6 行 ctx），其余 33 行折叠。
    const collapsed = gaps.reduce((sum, row) => sum + (row.count ?? 0), 0);
    expect(collapsed).toBe(33);
  });

  it("marks oversized input as truncated instead of computing", () => {
    const big = `${Array.from({ length: 1501 }, (_, i) => `l${i}`).join("\n")}`;
    expect(computeLineDiff(big, "x").truncated).toBe(true);
    expect(computeLineDiff("x".repeat(200_001), "x").truncated).toBe(true);
  });

  it("handles repeated lines without corrupting counts", () => {
    const diff = computeLineDiff("x\nx\nx\nx", "x\nx");
    expect(diff.added).toBe(0);
    expect(diff.removed).toBe(2);
  });

  it("renders empty lines inside the diff body", () => {
    const diff = computeLineDiff("a\n\nb", "a\n\nc");
    expect(diff.added).toBe(1);
    expect(diff.removed).toBe(1);
  });
});
