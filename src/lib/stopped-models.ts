import type { ModelOption } from "./model-memory";

interface Provider { id: string; models: { id: string; name: string }[] }
interface Credential { provider: string }
interface OfficialModel { id: string; name: string }

/** Model candidates while an RPC process is stopped, from the same configured catalogs as settings. */
export async function stoppedModels(
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>,
): Promise<ModelOption[]> {
  const [configured, status, selection] = await Promise.allSettled([
    invoke<Provider[]>("list_pi_providers"),
    invoke<{ credentials: Credential[] }>("pi_auth_status"),
    invoke<{ modelIds: string[] | null }>("pi_auth_model_selection"),
  ]);
  if (configured.status === "rejected" && status.status === "rejected") throw configured.reason;
  const credentials = status.status === "fulfilled" ? status.value.credentials : [];
  const selectionError = selection.status === "rejected" && credentials.some((item) => item.provider === "openai-codex")
    ? selection.reason : null;
  const models: ModelOption[] = configured.status === "fulfilled"
    ? configured.value.flatMap((provider) => provider.models.map((model) => ({
        provider: provider.id, id: model.id, name: model.name,
      })))
    : [];
  if (!models.length && status.status === "rejected") throw status.reason;
  if (!models.length && selectionError) throw selectionError;
  const selected = selection.status === "fulfilled" ? selection.value.modelIds : null;
  const official = await Promise.allSettled([...new Set(credentials.map((item) => item.provider))]
    .filter((provider) => provider !== "openai-codex" || !selectionError)
    .map(async (provider) => ({
      provider,
      models: await invoke<OfficialModel[]>("pi_auth_provider_models", { request: { providerId: provider } }),
    })));
  if (!models.length && official.length && official.every((item) => item.status === "rejected")) {
    const first = official[0];
    if (first.status === "rejected") throw first.reason;
  }
  for (const result of official) {
    if (result.status !== "fulfilled") continue;
    const { provider, models: catalog } = result.value;
    for (const model of catalog) {
      if (provider === "openai-codex" && selected !== null && !selected.includes(model.id)) continue;
      models.push({ provider, id: model.id, name: model.name });
    }
  }
  return [...new Map(models.map((model) => [`${model.provider}/${model.id}`, model])).values()];
}
