import { describe, expect, it } from "vitest";
import { createLatestSearch, groupSearchMatches, sourceOffset } from "./search";

describe("content result grouping", () => {
  it("groups paths without losing matches or changing their order", () => {
    const matches = [
      { path: "src/模型.ts", line: 2, column: 1, text: "first" },
      { path: "a.ts", line: 1, column: 2, text: "second" },
      { path: "src/模型.ts", line: 3, column: 4, text: "third" },
    ];
    const groups = groupSearchMatches(matches);
    expect(groups.map((group) => group.path)).toEqual(["src/模型.ts", "a.ts"]);
    expect(groups[0].matches.map((match) => match.text)).toEqual(["first", "third"]);
    expect(groupSearchMatches([])).toEqual([]);
  });
});

describe("source location", () => {
  it("maps Unicode columns and CRLF lines to UTF-16 offsets", () => {
    expect(sourceOffset("one\r\n世界😀match", 2, 4)).toBe(9);
    expect(sourceOffset("one\n", 99, 99)).toBe(4);
    expect(sourceOffset("abc\nnext", 1, 99)).toBe(3);
  });
});

describe("latest content search", () => {
  it("ignores a response after the query is cleared", async () => {
    let finish!: (value: string) => void;
    const results: string[] = [];
    const controller = createLatestSearch<string>(
      () => new Promise((resolve) => { finish = resolve; }),
      async () => {},
    );
    const done = controller.run("one", (result) => results.push(result), () => {});
    await Promise.resolve();
    controller.cancel();
    finish("old");
    await done;
    expect(results).toEqual([]);
  });

  it("serializes searches and publishes only the newest result", async () => {
    let finish!: (value: string) => void;
    const calls: string[] = [];
    const results: string[] = [];
    const controller = createLatestSearch<string>(
      async (id) => {
        calls.push(id);
        if (id === "one") return new Promise((resolve) => { finish = resolve; });
        return "new";
      },
      async () => {},
    );
    const first = controller.run("one", (result) => results.push(result), () => {});
    await Promise.resolve();
    const second = controller.run("two", (result) => results.push(result), () => {});
    expect(calls).toEqual(["one"]);
    finish("old");
    await Promise.all([first, second]);
    expect(calls).toEqual(["one", "two"]);
    expect(results).toEqual(["new"]);
  });
});
