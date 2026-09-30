import { describe, expect, it } from "vitest";
import { BUILTIN_SLASH_COMMANDS, filterSlashCommands, parseSlashCommand, runHostSlash, slashCommandRoute, slashSourceLabel, supportsProviderLogin, syncSessionRebound } from "./slash-commands";

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

  it("exposes managed Pi builtins in the GUI and reports source labels", () => {
    const quit = filterSlashCommands("quit", pi)[0];
    expect(quit?.available).toBe(true);
    expect(slashSourceLabel(quit!)).toBe("内置");
    const skill = filterSlashCommands("skill:pdf", pi)[0];
    expect(slashSourceLabel(skill!)).toBe("技能");
    const builtin = filterSlashCommands("compact", pi)[0];
    expect(slashSourceLabel(builtin!)).toBe("内置");
  });
});

describe("managed Pi slash dispatch", () => {
  it("routes every 0.99.1 builtin without sending it to the model", () => {
    const host = "new settings resume scoped-models import share changelog hotkeys trust login logout reload quit".split(" ");
    const rpc = "compact model thinking name copy session export tree fork clone".split(" ");
    expect(BUILTIN_SLASH_COMMANDS.map((command) => command.name).sort()).toEqual([...host, ...rpc, "bug"].sort());
    for (const name of host) expect(slashCommandRoute(name, [])).toBe("host");
    for (const name of rpc) expect(slashCommandRoute(name, [])).toBe("rpc");
    expect(BUILTIN_SLASH_COMMANDS.filter((command) => !command.available).map((command) => command.name)).toEqual(["bug"]);
    expect(slashCommandRoute("bug", [])).toBe("blocked");
    expect(filterSlashCommands("bug", []).map((command) => command.available)).toEqual([false]);
  });

  it("keeps extensions, skills and prompt templates on the Pi prompt path", () => {
    const commands = [
      { name: "deploy", source: "extension", description: "Deploy" },
      { name: "skill:pdf", source: "skill", description: "PDF" },
      { name: "review", source: "prompt", description: "Review" },
    ];
    for (const command of commands) expect(slashCommandRoute(command.name, commands)).toBe("prompt");
    expect(slashCommandRoute("missing", commands)).toBe("unknown");
    expect(slashCommandRoute("quit", commands)).toBe("host");
  });

  it("preserves a host draft on cancellation and failure, and handles success", async () => {
    const errors: unknown[] = [];
    expect(await runHostSlash(async () => true, (error) => errors.push(error))).toBe("handled");
    expect(await runHostSlash(async () => false, (error) => errors.push(error))).toBe("blocked");
    expect(await runHostSlash(async () => { throw new Error("offline"); }, (error) => errors.push(error))).toBe("blocked");
    expect(errors).toHaveLength(1);
    expect((errors[0] as Error).message).toBe("offline");
  });
  it("accepts OAuth or Pi API Key providers for /login, but rejects unsupported identifiers", () => {
    const providers = [{ id: "openai-codex", oauth: true, apiKey: false }, { id: "api-only", oauth: false, apiKey: true }, { id: "unsupported", oauth: false, apiKey: false }];
    expect(supportsProviderLogin("openai-codex", providers)).toBe(true);
    expect(supportsProviderLogin("api-only", providers)).toBe(true);
    expect(supportsProviderLogin("unsupported", providers)).toBe(false);
    expect(supportsProviderLogin("unknown", providers)).toBe(false);
  });

  it("reconnects after a session switch even if task mapping refresh fails", async () => {
    const events: string[] = [];
    expect(await syncSessionRebound(async () => { events.push("mapped"); }, () => events.push("reconnected"), () => events.push("error"))).toBe(true);
    expect(events).toEqual(["mapped", "reconnected"]);
    const failure = new Error("mapping unavailable");
    const errors: unknown[] = [];
    expect(await syncSessionRebound(async () => { throw failure; }, () => events.push("reconnected"), (error) => errors.push(error))).toBe(false);
    expect(errors).toEqual([failure]);
    expect(events).toEqual(["mapped", "reconnected", "reconnected"]);
  });
});
