import { describe, expect, it } from "vitest";
import { retainEmptyDirectories, visibleFiles, type FileEntry } from "./files";

const entries: FileEntry[] = [
  { path: "src", name: "src", isDirectory: true, size: 0 },
  { path: "src/lib", name: "lib", isDirectory: true, size: 0 },
  { path: "src/lib/模型.ts", name: "模型.ts", isDirectory: false, size: 10 },
  { path: "src/main.ts", name: "main.ts", isDirectory: false, size: 10 },
  { path: "README.md", name: "README.md", isDirectory: false, size: 1 },
];

describe("file tree filtering", () => {
  it("shows only expanded children in normal mode", () => {
    expect(visibleFiles(entries, "", new Set()).map((entry) => entry.path)).toEqual(["src", "README.md"]);
    expect(visibleFiles(entries, "", new Set(["src"])).map((entry) => entry.path))
      .toEqual(["src", "src/lib", "src/main.ts", "README.md"]);
  });
  it("keeps and opens ancestors of a match without mutating expansion", () => {
    const expanded = new Set<string>();
    expect(visibleFiles(entries, "模型", expanded).map((entry) => entry.path))
      .toEqual(["src", "src/lib", "src/lib/模型.ts"]);
    expect(expanded.size).toBe(0);
    expect(visibleFiles(entries, "", expanded).length).toBe(2);
  });
  it("matches paths, ignores case and distinguishes no matches", () => {
    expect(visibleFiles(entries, "SL模", new Set()).length).toBe(3);
    expect(visibleFiles(entries, "zzz", new Set())).toEqual([]);
    expect(visibleFiles(entries, "README", new Set()).map((entry) => entry.path)).toEqual(["README.md"]);
  });
});

describe("file tree refresh", () => {
  it("keeps expanded folders and filename search when entries change", () => {
    const expanded = new Set(["src"]);
    const query = "main";
    const updated = [...entries, { path: "src/main.test.ts", name: "main.test.ts", isDirectory: false, size: 5 }];
    expect(visibleFiles(updated, query, expanded).map((entry) => entry.path))
      .toEqual(["src", "src/main.test.ts", "src/main.ts"]);
    expect(expanded.has("src")).toBe(true);
    expect(query).toBe("main");
  });

  it("keeps empty-folder markers until the refreshed index has children or removes the folder", () => {
    const old: FileEntry[] = [{ path: "empty", name: "empty", isDirectory: true, size: 0 }];
    expect(retainEmptyDirectories(["empty"], old)).toEqual(["empty"]);
    expect(retainEmptyDirectories(["empty"], [
      ...old, { path: "empty/new.txt", name: "new.txt", isDirectory: false, size: 1 },
    ])).toEqual([]);
    expect(retainEmptyDirectories(["empty"], [])).toEqual([]);
  });
});
