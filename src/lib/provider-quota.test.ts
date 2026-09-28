import { describe, expect, it } from "vitest";
import { createQuotaTracker, quotaTarget, quotaWindowRows, type QuotaResponse, type QuotaState } from "./provider-quota";

describe("quotaTarget", () => {
  it.each([
    ["anthropic/claude-sonnet", "Claude Code"],
    ["openai-codex/gpt-codex", "Codex"],
    ["google/gemini-2.5-pro", "Gemini"],
    ["google-vertex/gemini-2.5-pro", "Gemini (Vertex)"],
    ["deepseek/deepseek-chat", "DeepSeek"],
    ["z-ai/glm", "Z.ai"],
    ["zai/glm", "Z.ai"],
    ["kimi-coding/kimi", "Kimi Coding"],
    ["moonshotai/kimi", "Kimi API"],
    ["moonshot/kimi", "Kimi API"],
    ["qwen/qwen3", "Qwen"],
    ["opencode-go/deepseek", "OpenCode Go"],
  ])("identifies %s as %s", (modelKey, label) => {
    expect(quotaTarget(modelKey)?.label).toBe(label);
  });

  it("does not mistake OpenCode Zen or OpenAI API for subscription accounts", () => {
    expect(quotaTarget("opencode/deepseek")).toBeNull();
    expect(quotaTarget("openai/gpt-codex")).toBeNull();
    expect(quotaTarget("/unknown")).toBeNull();
    expect(quotaTarget("")).toBeNull();
  });

  it("keeps Gemini API and Vertex quota identities separate", () => {
    expect(quotaTarget("google/gemini-2.5-pro")?.providerId).toBe("google");
    expect(quotaTarget("google-vertex/gemini-2.5-pro")?.providerId).toBe("google-vertex");
  });
});

describe("quotaWindowRows", () => {
  it("shows verified 5h and weekly windows in separate ordered rows", () => {
    expect(quotaWindowRows({ status: "available", windows: { fiveHour: { remainingPercent: 62.5 }, weekly: { remainingPercent: 30 } } })).toEqual([
      { label: "5h", remainingPercent: 62.5 },
      { label: "weekly", remainingPercent: 30 },
    ]);
    expect(quotaWindowRows({ status: "exhausted", windows: { weekly: { remainingPercent: 0 } } })).toEqual([
      { label: "weekly", remainingPercent: 0 },
    ]);
  });

  it("does not display invalid or fabricated windows", () => {
    expect(quotaWindowRows({ status: "unavailable" })).toEqual([]);
    expect(quotaWindowRows({ status: "available", amount: 10, currency: "USD" })).toEqual([]);
    expect(quotaWindowRows({ status: "available", windows: { fiveHour: { remainingPercent: Number.NaN }, weekly: { remainingPercent: 101 } } })).toEqual([]);
  });
});

describe("createQuotaTracker", () => {
  const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

  it("drops old provider responses on model switch and session reset", async () => {
    const pending: { providerId: string; resolve: (value: QuotaResponse) => void }[] = [];
    const states: (QuotaState | null)[] = [];
    const tracker = createQuotaTracker(
      (providerId) => new Promise((resolve) => { pending.push({ providerId, resolve }); }),
      (state) => { states.push(state); },
    );
    tracker.load(quotaTarget("deepseek/chat"));
    tracker.load(quotaTarget("opencode-go/chat"));
    expect(pending.map((item) => item.providerId)).toEqual(["deepseek", "opencode-go"]);
    pending[0].resolve({ status: "available", amount: 99, currency: "CNY" });
    await flush();
    expect(states.at(-1)).toEqual({ status: "loading" });
    pending[1].resolve({ status: "unavailable" });
    await flush();
    expect(states.at(-1)).toEqual({ status: "unavailable" });
    tracker.load(quotaTarget("deepseek/chat"));
    tracker.stop();
    pending[2].resolve({ status: "available", amount: 2, currency: "CNY" });
    await flush();
    expect(states.at(-1)).toBeNull();
  });

  it("distinguishes missing, exhausted and errors without exposing rejection text", async () => {
    const states: (QuotaState | null)[] = [];
    let response: QuotaResponse | Error = { status: "missing" };
    const tracker = createQuotaTracker(
      async () => { if (response instanceof Error) throw response; return response; },
      (state) => { states.push(state); },
    );
    tracker.load(quotaTarget("anthropic/claude"));
    await flush();
    expect(states.at(-1)).toEqual({ status: "missing" });
    response = { status: "exhausted", amount: 0, currency: "USD" };
    tracker.load(quotaTarget("deepseek/chat"));
    await flush();
    expect(states.at(-1)).toEqual(response);
    response = new Error("secret_token_from_server");
    tracker.load(quotaTarget("deepseek/chat"));
    await flush();
    expect(states.at(-1)).toEqual({ status: "error" });
    expect(JSON.stringify(states)).not.toContain("secret_token_from_server");
  });
});
