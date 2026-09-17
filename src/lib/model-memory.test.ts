import { describe, expect, it } from "vitest";
import {
  findSavedModel,
  parseModelKey,
  shouldApplySavedChoice,
  validSavedThinkingLevel,
  type SavedModelChoice,
} from "./model-memory";

const MODELS = [
  { id: "deepseek-chat", provider: "deepseek", name: "DeepSeek Chat" },
  { id: "deepseek-reasoner", provider: "deepseek", name: "DeepSeek Reasoner" },
  { id: "vendor/model", provider: "openrouter", name: "Vendor Model" },
];

function choice(provider: string, modelId: string, thinkingLevel: string | null = null): SavedModelChoice {
  return { provider, modelId, thinkingLevel };
}

describe("shouldApplySavedChoice", () => {
  it("applies to an empty idle session", () => {
    expect(shouldApplySavedChoice(0, false)).toBe(true);
  });

  it("never overrides a restored session that already has history", () => {
    expect(shouldApplySavedChoice(1, false)).toBe(false);
    expect(shouldApplySavedChoice(42, false)).toBe(false);
  });

  it("does not touch a session that is already streaming", () => {
    expect(shouldApplySavedChoice(0, true)).toBe(false);
  });
});

describe("findSavedModel", () => {
  it("matches provider and model id", () => {
    expect(findSavedModel(choice("deepseek", "deepseek-chat"), MODELS)).toEqual(MODELS[0]);
  });

  it("returns null when the saved model is no longer available", () => {
    expect(findSavedModel(choice("deepseek", "deleted-model"), MODELS)).toBeNull();
    expect(findSavedModel(choice("unknown-provider", "deepseek-chat"), MODELS)).toBeNull();
    expect(findSavedModel(choice("deepseek", "deepseek-chat"), [])).toBeNull();
  });

  it("keeps model ids that contain a slash distinct", () => {
    expect(findSavedModel(choice("openrouter", "vendor/model"), MODELS)).toEqual(MODELS[2]);
  });
});

describe("validSavedThinkingLevel", () => {
  it("keeps a level that the current model still offers", () => {
    expect(validSavedThinkingLevel("high", ["off", "medium", "high"])).toBe("high");
  });

  it("skips when the level is unknown to the current model", () => {
    expect(validSavedThinkingLevel("max", ["off", "medium", "high"])).toBeNull();
  });

  it("skips when the project never recorded a level", () => {
    expect(validSavedThinkingLevel(null, ["off", "high"])).toBeNull();
    expect(validSavedThinkingLevel("", ["off", "high"])).toBeNull();
  });
});

describe("parseModelKey", () => {
  it("splits provider from the rest of the key", () => {
    expect(parseModelKey("deepseek/deepseek-chat")).toEqual({ provider: "deepseek", modelId: "deepseek-chat" });
    expect(parseModelKey("openrouter/vendor/model")).toEqual({ provider: "openrouter", modelId: "vendor/model" });
  });

  it("rejects malformed keys", () => {
    expect(parseModelKey("no-slash")).toBeNull();
    expect(parseModelKey("/leading")).toBeNull();
    expect(parseModelKey("trailing/")).toBeNull();
    expect(parseModelKey("")).toBeNull();
  });
});
