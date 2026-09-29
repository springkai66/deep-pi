import { describe, expect, it } from "vitest";
import { stoppedModels } from "./stopped-models";

function catalog(responses: Record<string, unknown>, calls: string[] = []) {
  return <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    calls.push(`${command}:${String((args?.request as { providerId?: string } | undefined)?.providerId ?? "")}`);
    const key = command === "pi_auth_provider_models"
      ? `${command}:${(args?.request as { providerId: string }).providerId}` : command;
    if (!(key in responses)) return Promise.reject(new Error(`unavailable: ${key}`));
    return Promise.resolve(responses[key] as T);
  };
}

describe("stopped chat model catalog", () => {
  it("collects configured and signed-in models, applies subscription selection, and deduplicates", async () => {
    const calls: string[] = [];
    const models = await stoppedModels(catalog({
      list_pi_providers: [{ id: "custom", models: [{ id: "alpha", name: "Alpha" }] }],
      pi_auth_status: { credentials: [{ provider: "openai-codex" }, { provider: "openai-codex" }, { provider: "custom" }] },
      pi_auth_model_selection: { modelIds: ["allowed"] },
      "pi_auth_provider_models:openai-codex": [{ id: "allowed", name: "Allowed" }, { id: "hidden", name: "Hidden" }],
      "pi_auth_provider_models:custom": [{ id: "alpha", name: "Alpha" }],
    }, calls));
    expect(models).toEqual([
      { provider: "custom", id: "alpha", name: "Alpha" },
      { provider: "openai-codex", id: "allowed", name: "Allowed" },
    ]);
    expect(calls.filter((call) => call === "pi_auth_provider_models:openai-codex")).toHaveLength(1);
  });

  it("keeps the configured models when auth is unavailable", async () => {
    expect(await stoppedModels(catalog({
      list_pi_providers: [{ id: "custom", models: [{ id: "alpha", name: "Alpha" }] }],
    }))).toEqual([{ provider: "custom", id: "alpha", name: "Alpha" }]);
  });

  it("keeps custom choices when subscription selection cannot be read", async () => {
    expect(await stoppedModels(catalog({
      list_pi_providers: [{ id: "custom", models: [{ id: "alpha", name: "Alpha" }] }],
      pi_auth_status: { credentials: [{ provider: "openai-codex" }] },
    }))).toEqual([{ provider: "custom", id: "alpha", name: "Alpha" }]);
  });

  it("reports a failed catalog rather than presenting it as an empty selection", async () => {
    await expect(stoppedModels(catalog({}))).rejects.toThrow("unavailable: list_pi_providers");
    await expect(stoppedModels(catalog({
      list_pi_providers: [],
      pi_auth_status: { credentials: [{ provider: "openai-codex" }] },
    }))).rejects.toThrow("unavailable: pi_auth_model_selection");
    await expect(stoppedModels(catalog({
      list_pi_providers: [],
      pi_auth_status: { credentials: [{ provider: "openai-codex" }] },
      pi_auth_model_selection: { modelIds: null },
    }))).rejects.toThrow("unavailable: pi_auth_provider_models:openai-codex");
  });
});
