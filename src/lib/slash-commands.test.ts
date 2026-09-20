import { describe, expect, it } from "vitest";
import { filterSlashCommands, parseSlashCommand, slashSourceLabel } from "./slash-commands";

describe("slash command parsing", () => {
  it("parses name and args from a command-looking draft", () => {
    expect(parseSlashCommand("/compact")).toEqual({ name: "compact", args: "" });
    expect(parseSlashCommand("  /name My session  ")).toEqual({ name: "name", args: "My session" });
    expect(parseSlashCommand("/skill:pdf extract pages")).toEqual({ name: "skill:pdf", args: "extract pages" });
  });

  it("rejects plain text, bare slashes and paths", () => {
    expect(parseSlashCommand("hello /compact")).toBeNull();
    expect(parseSlashCommand("/")).toBeNull();
    expect(parseSlashCommand("")).toBeNull();
    expect(parseSlashCommand("/usr/bin/env")).toBeNull();
  });
});

describe("slash command filtering", () => {
  const pi = [
    { name: "deploy", description: "Deploy the app", source: "extension" },
    { name: "skill:pdf", description: "PDF tools", source: "skill" },
    { name: "compact", description: "should be deduped", source: "extension" },
    { name: "", description: "invalid", source: "extension" },
  ];

  it("merges builtins first, dedupes pi commands and normalizes sources", () => {
    const items = filterSlashCommands("", pi, 100);
    expect(items[0]?.name).toBe("compact");
    expect(items.filter((item) => item.name === "compact")).toHaveLength(1);
    expect(items.find((item) => item.name === "deploy")?.source).toBe("extension");
    expect(items.find((item) => item.name === "skill:pdf")?.source).toBe("skill");
    expect(items.some((item) => item.name === "")).toBe(false);
  });

  it("ranks prefix matches first and respects the limit", () => {
    const items = filterSlashCommands("co", pi);
    expect(items[0]?.name).toBe("compact");
    expect(items.every((item) => item.name.toLowerCase().includes("co")
      || item.description.toLowerCase().includes("co"))).toBe(true);
    expect(filterSlashCommands("", pi, 3)).toHaveLength(3);
  });

  it("marks terminal-only commands and reports source labels", () => {
    const terminal = filterSlashCommands("quit", pi)[0];
    expect(terminal?.available).toBe(false);
    expect(slashSourceLabel(terminal!)).toBe("终端");
    const skill = filterSlashCommands("skill:pdf", pi)[0];
    expect(slashSourceLabel(skill!)).toBe("技能");
    const builtin = filterSlashCommands("compact", pi)[0];
    expect(slashSourceLabel(builtin!)).toBe("内置");
  });
});
