<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import {
    Check,
    ChevronDown,
    ChevronRight,
    KeyRound,
    Plus,
    RefreshCw,
    Save,
    Server,
    Trash2,
    X,
    Zap,
  } from "@lucide/svelte";
  import { onMount } from "svelte";
  import type {
    ConfiguredModel,
    PiModelProfile,
    PiModelSummary,
    ProviderConnectionResult,
    ProviderCredentialStatus,
    ProviderModelSummary,
    ProviderRecord,
  } from "$lib/provider";

  interface Props {
    confirm: (title: string, message: string, confirmLabel?: string) => Promise<boolean>;
    onClose: () => void;
    onError: (error: unknown) => void;
    embedded?: boolean;
    invokeCommand?: typeof nativeInvoke;
  }

  interface ProviderPreset {
    key: string;
    label: string;
    id: string;
    name: string;
    api: string;
    baseUrl: string;
    catalogProvider: string;
  }

  interface HeaderEntry {
    name: string;
    value: string;
  }

  interface ModelRow {
    id: string;
    name: string;
    sub: string;
    configured: boolean;
    summary: ProviderModelSummary | null;
  }

  interface ModelTestMessage {
    ok: boolean;
    text: string;
  }

  const PROVIDER_PRESETS: ProviderPreset[] = [
    {
      key: "deepseek",
      label: "DeepSeek",
      id: "deepseek",
      name: "DeepSeek",
      api: "openai-completions",
      baseUrl: "https://api.deepseek.com/v1",
      catalogProvider: "deepseek",
    },
    {
      key: "openai",
      label: "OpenAI",
      id: "openai",
      name: "OpenAI",
      api: "openai-responses",
      baseUrl: "https://api.openai.com/v1",
      catalogProvider: "openai",
    },
    {
      key: "gemini",
      label: "Gemini",
      id: "google",
      name: "Gemini",
      api: "google-generative-ai",
      baseUrl: "https://generativelanguage.googleapis.com/v1beta",
      catalogProvider: "google",
    },
    {
      key: "claude-code",
      label: "Claude Code",
      id: "anthropic",
      name: "Claude Code",
      api: "anthropic-messages",
      baseUrl: "https://api.anthropic.com",
      catalogProvider: "anthropic",
    },
    {
      key: "kimi",
      label: "Kimi For Coding",
      id: "kimi-coding",
      name: "Kimi For Coding",
      api: "openai-completions",
      baseUrl: "https://api.kimi.com/coding/v1",
      catalogProvider: "kimi-coding",
    },
    {
      key: "opencode",
      label: "OpenCode Zen",
      id: "opencode",
      name: "OpenCode Zen",
      api: "openai-completions",
      baseUrl: "https://opencode.ai/zen/v1",
      catalogProvider: "opencode",
    },
  ];

  const API_OPTIONS = [
    { value: "openai-completions", label: "OpenAI Chat Completions" },
    { value: "openai-responses", label: "OpenAI Responses" },
    { value: "anthropic-messages", label: "Anthropic Messages" },
    { value: "google-generative-ai", label: "Google Generative AI" },
  ];

  const KNOWN_LEVELS = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

  let { confirm, onClose, onError, embedded = false, invokeCommand = nativeInvoke }: Props = $props();
  const invoke = <T,>(command: string, args?: Parameters<typeof nativeInvoke>[1]) => invokeCommand<T>(command, args);
  let providers = $state<ProviderRecord[]>([]);
  let draft = $state<ProviderRecord>({
    id: "",
    name: "",
    api: "openai-completions",
    baseUrl: "",
    headers: {},
    proxy: null,
    models: [],
  });
  let providerPreset = $state("custom");
  let apiKey = $state("");
  let apiKeyConfigured = $state(false);
  let nativeAuthConfigured = $state(false);
  let authType = $state<string | null>(null);
  let credentialConfigured = $state(false);
  let statusMessage = $state("");
  let connectionMessage = $state("");
  let isLoading = $state(false);
  let isFetchingProviderModels = $state(false);
  let busyModel = $state<string | null>(null);
  let headerEntries = $state<HeaderEntry[]>([]);
  let expanded = $state<Record<string, boolean>>({});
  let showAllModels = $state<Record<string, boolean>>({});
  let fetchedModels = $state<Record<string, ProviderModelSummary[]>>({});
  let detailMode = $state<"provider" | "model">("provider");
  let selectedModelId = $state<string | null>(null);
  let modelTest = $state<Record<string, ModelTestMessage>>({});
  /// 保存成功后快照的草稿；草稿与快照一致时保存按钮置灰并提示已保存。
  let savedSnapshot = $state<string | null>(null);
  let autofillMessage = $state("");
  let autofillModelId = $state<string | null>(null);

  const isNewProvider = $derived(!providers.some((provider) => provider.id === draft.id));
  const isCustomProvider = $derived(providerPreset === "custom");
  const catalogProvider = $derived(
    PROVIDER_PRESETS.find((preset) => preset.key === providerPreset)?.catalogProvider ?? draft.id,
  );
  const editingModel = $derived(
    detailMode === "model" ? draft.models.find((model) => model.id === selectedModelId) ?? null : null,
  );
  const draftDirty = $derived(savedSnapshot !== null && draftSnapshot() !== savedSnapshot);
  const saveReady = $derived(!isLoading && (savedSnapshot === null || draftDirty));

  function draftSnapshot(): string {
    syncDraftHeaders();
    return JSON.stringify({
      draft,
      apiKey: apiKey.trim().length > 0,
      providerPreset,
    });
  }

  onMount(() => {
    void loadProviders();
  });

  function cloneProvider(provider: ProviderRecord): ProviderRecord {
    return {
      ...provider,
      headers: { ...provider.headers },
      models: provider.models.map((model) => ({
        ...model,
        input: [...model.input],
        thinkingLevels: [...model.thinkingLevels],
      })),
    };
  }

  function setHeaderEntries(headers: Record<string, string>) {
    headerEntries = Object.entries(headers).map(([name, value]) => ({ name, value }));
  }

  function syncDraftHeaders() {
    draft.headers = Object.fromEntries(
      headerEntries
        .map(({ name, value }) => [name.trim(), value] as const)
        .filter(([name]) => name.length > 0),
    );
  }

  function addHeader() {
    headerEntries = [...headerEntries, { name: "", value: "" }];
  }

  function removeHeader(index: number) {
    headerEntries = headerEntries.filter((_, candidate) => candidate !== index);
    syncDraftHeaders();
  }

  function presetFor(provider: ProviderRecord): ProviderPreset | undefined {
    return PROVIDER_PRESETS.find((preset) => preset.id === provider.id);
  }

  async function loadProviders() {
    isLoading = true;
    try {
      providers = await invoke<ProviderRecord[]>("list_pi_providers");
      if (providers.length > 0) selectProvider(providers[0]);
    } catch (error) {
      onError(error);
    } finally {
      isLoading = false;
    }
  }

  function selectProvider(provider: ProviderRecord) {
    draft = cloneProvider(provider);
    setHeaderEntries(provider.headers);
    providerPreset = presetFor(provider)?.key ?? "custom";
    apiKey = "";
    apiKeyConfigured = false;
    nativeAuthConfigured = false;
    authType = null;
    credentialConfigured = false;
    statusMessage = "";
    connectionMessage = "";
    savedSnapshot = null;
    void refreshCredentialStatus();
  }

  function newProvider() {
    draft = { id: "", name: "", api: "openai-completions", baseUrl: "", headers: {}, proxy: null, models: [] };
    providerPreset = "custom";
    headerEntries = [];
    apiKey = "";
    apiKeyConfigured = false;
    nativeAuthConfigured = false;
    authType = null;
    credentialConfigured = false;
    statusMessage = "";
    connectionMessage = "";
    detailMode = "provider";
    selectedModelId = null;
    savedSnapshot = null;
  }

  function applyProviderPreset(value: string) {
    providerPreset = value;
    const preset = PROVIDER_PRESETS.find((candidate) => candidate.key === value);
    if (!preset) {
      newProvider();
      return;
    }
    const existing = providers.find((provider) => provider.id === preset.id);
    if (existing) {
      selectProvider({
        ...existing,
        name: existing.name || preset.name,
        api: existing.api || preset.api,
        baseUrl: existing.baseUrl || preset.baseUrl,
        headers: { ...existing.headers },
        proxy: existing.proxy,
      });
      detailMode = "provider";
      selectedModelId = null;
      return;
    }
    draft = {
      id: preset.id,
      name: preset.name,
      api: preset.api,
      baseUrl: preset.baseUrl,
      headers: {},
      proxy: null,
      models: [],
    };
    headerEntries = [];
    apiKey = "";
    apiKeyConfigured = false;
    nativeAuthConfigured = false;
    authType = null;
    credentialConfigured = false;
    statusMessage = "";
    connectionMessage = "";
    detailMode = "provider";
    selectedModelId = null;
  }

  async function refreshCredentialStatus() {
    if (!draft.id) {
      credentialConfigured = false;
      return;
    }
    try {
      const status = await invoke<ProviderCredentialStatus>("provider_credential_status", {
        request: { providerId: draft.id },
      });
      apiKeyConfigured = status.apiKeyConfigured;
      nativeAuthConfigured = status.nativeAuthConfigured;
      authType = status.authType;
      credentialConfigured = status.configured;
    } catch (error) {
      onError(error);
    }
  }

  async function saveProvider(showStatus = true): Promise<ProviderRecord | null> {
    isLoading = true;
    if (showStatus) statusMessage = "";
    syncDraftHeaders();
    try {
      const saved = await invoke<ProviderRecord>("save_pi_provider", {
        request: {
          ...draft,
          baseUrl: draft.baseUrl?.trim() || null,
          proxy: draft.proxy?.trim() || null,
        },
      });
      providers = [saved, ...providers.filter((provider) => provider.id !== saved.id)];
      draft = cloneProvider(saved);
      setHeaderEntries(saved.headers);
      if (apiKey.trim()) {
        await saveCredential(false);
      } else {
        await refreshCredentialStatus();
      }
      if (showStatus) statusMessage = "Provider 已保存";
      savedSnapshot = draftSnapshot();
      return saved;
    } catch (error) {
      onError(error);
      return null;
    } finally {
      isLoading = false;
    }
  }

  async function saveCredential(showStatus = true) {
    if (!draft.id) {
      onError("请先保存 Provider");
      return;
    }
    if (!apiKey.trim()) {
      onError("API Key 不能为空");
      return;
    }
    try {
      const status = await invoke<ProviderCredentialStatus>("save_provider_credential", {
        request: { providerId: draft.id, apiKey },
      });
      apiKey = "";
      apiKeyConfigured = true;
      nativeAuthConfigured = false;
      authType = "api_key";
      credentialConfigured = status.configured;
      if (showStatus) statusMessage = "API Key 已保存到 Windows Credential Manager";
    } catch (error) {
      onError(error);
    }
  }

  async function deleteCredential() {
    if (!draft.id || !(await confirm("删除 API Key", `删除 ${draft.name || draft.id} 的 Windows 凭据吗？`, "删除"))) return;
    try {
      await invoke("delete_provider_credential", { request: { providerId: draft.id } });
      apiKeyConfigured = false;
      credentialConfigured = nativeAuthConfigured;
      if (!nativeAuthConfigured) authType = null;
      statusMessage = "API Key 已删除";
    } catch (error) {
      onError(error);
    }
  }

  async function testConnection() {
    if (!draft.id) {
      onError("请先保存 Provider");
      return;
    }
    connectionMessage = "正在测试…";
    try {
      const saved = await saveProvider(false);
      if (!saved) return;
      const result = await invoke<ProviderConnectionResult>("test_provider_connection", {
        request: { providerId: saved.id },
      });
      connectionMessage = result.reachable
        ? `可达 · HTTP ${result.status}`
        : `不可达 · HTTP ${result.status}（检查 Base URL 与 API Key）`;
    } catch (error) {
      connectionMessage = "连接失败";
      onError(error);
    }
  }

  async function removeProvider() {
    if (!draft.id || isNewProvider) return;
    if (!(await confirm("删除 Provider", `删除 ${draft.name || draft.id} 及其模型配置吗？`, "删除"))) return;
    try {
      await invoke("delete_pi_provider", { request: { id: draft.id } });
      await invoke("delete_provider_credential", { request: { providerId: draft.id } });
      providers = providers.filter((provider) => provider.id !== draft.id);
      newProvider();
      statusMessage = "Provider 已删除";
    } catch (error) {
      onError(error);
    }
  }

  function toggleExpanded(providerId: string) {
    expanded[providerId] = !expanded[providerId];
  }

  function ensureDraft(provider: ProviderRecord) {
    if (draft.id !== provider.id) selectProvider(provider);
  }

  function configuredModelsOf(provider: ProviderRecord): ConfiguredModel[] {
    return provider.id === draft.id ? draft.models : provider.models;
  }

  function modelRows(provider: ProviderRecord): ModelRow[] {
    const configured = configuredModelsOf(provider);
    const rows: ModelRow[] = configured.map((model) => ({
      id: model.id,
      name: model.name,
      sub: `${model.id} · ${formatContext(model.contextWindow)}${model.reasoning ? " · 推理" : ""}${model.cost ? ` · ${formatCost(model.cost.input)}/${formatCost(model.cost.output)}` : ""}`,
      configured: true,
      summary: null,
    }));
    if (!showAllModels[provider.id]) return rows;
    for (const summary of fetchedModels[provider.id] ?? []) {
      if (configured.some((model) => model.id === summary.id)) continue;
      rows.push({
        id: summary.id,
        name: summary.name,
        sub: `${summary.id} · ${formatContext(summary.contextWindow)}`,
        configured: false,
        summary,
      });
    }
    return rows;
  }

  async function refreshProviderModels(provider: ProviderRecord) {
    ensureDraft(provider);
    if (!draft.id || !draft.baseUrl) {
      onError("请先选择 Provider 或填写 Base URL");
      return;
    }
    isFetchingProviderModels = true;
    statusMessage = "";
    try {
      const saved = await saveProvider(false);
      if (!saved) return;
      const models = await invoke<ProviderModelSummary[]>("list_provider_models", {
        request: { providerId: saved.id },
      });
      fetchedModels = { ...fetchedModels, [saved.id]: models };
      showAllModels = { ...showAllModels, [saved.id]: true };
      expanded = { ...expanded, [saved.id]: true };
      statusMessage = `已拉取 ${models.length} 个模型`;
    } catch (error) {
      onError(error);
    } finally {
      isFetchingProviderModels = false;
    }
  }

  async function officialSummaryFor(modelId: string): Promise<PiModelSummary | undefined> {
    const results = await invoke<PiModelSummary[]>("search_pi_models", {
      request: { query: modelId, provider: catalogProvider.trim() || null },
    });
    return results.find((candidate) => candidate.id === modelId);
  }

  function summaryToConfigured(summary: ProviderModelSummary): ConfiguredModel {
    return {
      id: summary.id,
      name: summary.name,
      reasoning: summary.reasoning ?? false,
      input: summary.input.length > 0 ? summary.input : ["text"],
      contextWindow: summary.contextWindow ?? 128000,
      maxTokens: summary.maxTokens ?? 8192,
      thinkingLevels: summary.reasoning ? ["off", "minimal", "low", "medium", "high"] : [],
      cost: null,
      api: null,
    };
  }

  function upsertModel(model: ConfiguredModel) {
    draft.models = [model, ...draft.models.filter((candidate) => candidate.id !== model.id)];
  }

  function modelFromProfile(profile: PiModelProfile, fallback?: ProviderModelSummary): ConfiguredModel {
    return {
      id: profile.id,
      name: profile.name,
      reasoning: profile.reasoning,
      input: profile.input.length > 0 ? profile.input : fallback?.input.length ? fallback.input : ["text"],
      contextWindow: profile.contextWindow ?? fallback?.contextWindow ?? 128000,
      maxTokens: profile.maxTokens ?? fallback?.maxTokens ?? 8192,
      // 官方目录声明了 thinkingLevelMap 就用它；推理模型但目录没给级别时预填标准五档。
      thinkingLevels:
        profile.thinkingLevels.length > 0
          ? profile.thinkingLevels
          : profile.reasoning
            ? ["off", "minimal", "low", "medium", "high"]
            : [],
      cost: profile.cost,
      api: profile.api ?? null,
    };
  }

  async function addProviderModel(summary: ProviderModelSummary) {
    busyModel = summary.id;
    try {
      let official: PiModelSummary | undefined;
      let catalogUnavailable = false;
      try {
        official = await officialSummaryFor(summary.id);
      } catch {
        catalogUnavailable = true;
      }
      if (official) {
        const profile = await invoke<PiModelProfile>("pi_model_profile", {
          request: { path: official.path, provider: official.provider, modelId: official.id },
        });
        upsertModel(modelFromProfile(profile, summary));
        statusMessage = `${profile.name} 已从 pi.dev/models 补全并填入`;
      } else {
        upsertModel(summaryToConfigured(summary));
        statusMessage = catalogUnavailable
          ? `${summary.name} 已填入，官方模型目录暂不可用`
          : `${summary.name} 已填入，pi.dev/models 暂无对应详情`;
      }
    } catch (error) {
      onError(error);
    } finally {
      busyModel = null;
    }
  }

  async function toggleModel(provider: ProviderRecord, row: ModelRow) {
    ensureDraft(provider);
    if (row.configured) {
      draft.models = draft.models.filter((model) => model.id !== row.id);
      if (selectedModelId === row.id) {
        selectedModelId = null;
        detailMode = "provider";
      }
      statusMessage = `${row.name} 已取消选择`;
      return;
    }
    if (!row.summary) return;
    await addProviderModel(row.summary);
    selectedModelId = row.id;
    detailMode = "model";
  }

  function openModelDetail(provider: ProviderRecord, row: ModelRow) {
    ensureDraft(provider);
    if (!row.configured && row.summary) {
      upsertModel(summaryToConfigured(row.summary));
    }
    selectedModelId = row.id;
    detailMode = "model";
  }

  function setNumberField(model: ConfiguredModel, field: "contextWindow" | "maxTokens", raw: string) {
    const value = Number(raw);
    if (raw.trim() !== "" && Number.isFinite(value) && value >= 0) {
      model[field] = Math.floor(value);
    }
  }

  function toggleLevel(model: ConfiguredModel, level: string, checked: boolean) {
    model.thinkingLevels = checked
      ? [...model.thinkingLevels, level]
      : model.thinkingLevels.filter((candidate) => candidate !== level);
  }

  function costValue(model: ConfiguredModel, field: "input" | "output" | "cacheRead" | "cacheWrite"): number | "" {
    return model.cost?.[field] ?? "";
  }

  function setCostField(model: ConfiguredModel, field: "input" | "output" | "cacheRead" | "cacheWrite", raw: string) {
    const trimmed = raw.trim();
    const value = trimmed === "" ? null : Number(trimmed);
    if (value !== null && !Number.isFinite(value)) return;
    const cost = model.cost ?? { input: null, output: null, cacheRead: null, cacheWrite: null };
    cost[field] = value;
    model.cost = cost;
  }

  async function autofillModel() {
    const model = editingModel;
    if (!model) return;
    busyModel = model.id;
    autofillMessage = "";
    try {
      const official = await officialSummaryFor(model.id);
      if (!official) {
        autofillMessage = "目录暂无详情";
        statusMessage = "官方模型目录暂无该模型详情";
        return;
      }
      const profile = await invoke<PiModelProfile>("pi_model_profile", {
        request: { path: official.path, provider: official.provider, modelId: official.id },
      });
      const fallback = (fetchedModels[draft.id] ?? []).find((candidate) => candidate.id === model.id);
      const merged = modelFromProfile(profile, fallback);
      model.name = merged.name;
      model.reasoning = merged.reasoning;
      model.input = merged.input;
      model.contextWindow = merged.contextWindow;
      model.maxTokens = merged.maxTokens;
      model.thinkingLevels = merged.thinkingLevels;
      model.cost = merged.cost;
      model.api = merged.api;
      autofillModelId = model.id;
      autofillMessage = "已自动填入";
      statusMessage = `${model.id} 已自动填入官方参数`;
    } catch (error) {
      onError(error);
    } finally {
      busyModel = null;
    }
  }

  async function testModelConnection() {
    const model = editingModel;
    if (!model || !draft.id) return;
    const id = model.id;
    modelTest = { ...modelTest, [id]: { ok: false, text: "正在测试…" } };
    try {
      const saved = await saveProvider(false);
      if (!saved) {
        modelTest = { ...modelTest, [id]: { ok: false, text: "Provider 保存失败" } };
        return;
      }
      const result = await invoke<{ ok: boolean; status: number | null; latencyMs: number | null; error: string | null }>(
        "test_model_connection",
        { request: { providerId: saved.id, modelId: id } },
      );
      modelTest = {
        ...modelTest,
        [id]: result.ok
          ? { ok: true, text: `连通 · HTTP ${result.status} · ${result.latencyMs ?? 0}ms` }
          : { ok: false, text: result.error ?? "连接失败" },
      };
    } catch (error) {
      modelTest = { ...modelTest, [id]: { ok: false, text: String(error) } };
    }
  }

  function formatContext(value: number | null) {
    if (value === null) return "未知";
    return value >= 1_000_000 ? `${(value / 1_000_000).toFixed(1)}M` : `${Math.round(value / 1_000)}K`;
  }

  function formatCost(value: number | null | undefined) {
    if (value === null || value === undefined) return "-";
    return `$${value}/M`;
  }
</script>

<section class="provider-page" class:embedded aria-label="Pi Provider 和模型配置">
  <header class="provider-header">
    {#if !embedded}<div class="provider-title">
      <button class="icon-button" type="button" aria-label="返回工作区" title="返回" onclick={onClose}><X size={17} /></button>
      <Server size={18} />
      <h1>Provider 与模型</h1>
    </div>{/if}
    <div class="header-actions">
      <button type="button" class="quiet-button" onclick={newProvider}><Plus size={14} />新建 Provider</button>
      {#if savedSnapshot !== null && !draftDirty}
        <span class="saved-note" role="status">已保存</span>
      {/if}
      <button type="button" class="primary-button" disabled={!saveReady}
        title={saveReady ? "" : "没有未保存的修改"}
        onclick={() => void saveProvider()}><Save size={14} />保存</button>
    </div>
  </header>

  <div class="provider-layout">
    <aside class="provider-list" aria-label="Provider 列表">
      <div class="section-heading"><span>Provider</span><span>{providers.length}</span></div>
      {#if providers.length === 0}
        <p class="empty">还没有 Provider</p>
      {:else}
        {#each providers as provider (provider.id)}
          <div class="provider-entry">
            <div class="provider-row" class:selected={provider.id === draft.id && detailMode === "provider"}>
              <button class="provider-expand" type="button" aria-expanded={!!expanded[provider.id]}
                aria-label={`展开 ${provider.name || provider.id} 的模型`} title="展开模型列表"
                onclick={() => toggleExpanded(provider.id)}>
                {#if expanded[provider.id]}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
              </button>
              <button class="provider-open" type="button" onclick={() => { selectProvider(provider); detailMode = "provider"; }}>
                <span class="provider-copy"><strong>{provider.name || provider.id}</strong><small>{provider.id}</small></span>
              </button>
              <span class="model-count">{configuredModelsOf(provider).length}</span>
            </div>
            {#if expanded[provider.id]}
              <div class="provider-models">
                <div class="models-toolbar">
                  <button type="button" class="quiet-button compact"
                    disabled={isFetchingProviderModels || busyModel !== null || (provider.id === draft.id && (!draft.id || !draft.baseUrl))}
                    onclick={() => void refreshProviderModels(provider)}>
                    {#if isFetchingProviderModels && provider.id === draft.id}<span class="spin"><RefreshCw size={13} /></span>{:else}<RefreshCw size={13} />{/if}
                    拉取所有模型
                  </button>
                </div>
                {#if modelRows(provider).length === 0}
                  <p class="empty">暂无模型</p>
                {:else}
                  {#each modelRows(provider) as row (row.id)}
                    <div class="model-row"
                      class:selected={detailMode === "model" && selectedModelId === row.id && provider.id === draft.id}>
                      <input type="checkbox" checked={row.configured} disabled={busyModel === row.id}
                        aria-label={`选择 ${row.name}`} title={row.configured ? "取消选择" : "选择模型"}
                        onchange={() => void toggleModel(provider, row)} />
                      <button type="button" class="model-open" onclick={() => openModelDetail(provider, row)}>
                        <strong>{row.name}</strong>
                        <small>{row.sub}</small>
                      </button>
                    </div>
                  {/each}
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </aside>

    <div class="detail-panel">
      {#if detailMode === "model" && editingModel}
        <section class="editor-section model-detail-section">
          <div class="section-heading">
            <span>模型 · {editingModel.id}</span>
            <span class="section-actions">
              {#if autofillMessage && autofillModelId === editingModel.id}
                <span class="saved-note" role="status">{autofillMessage}</span>
              {/if}
              <button type="button" class="quiet-button compact" disabled={busyModel === editingModel.id}
                onclick={() => void autofillModel()}>
                {#if busyModel === editingModel.id}<span class="spin"><RefreshCw size={13} /></span>{:else}<Zap size={13} />{/if}
                自动填入
              </button>
              <button type="button" class="quiet-button compact" disabled={!draft.id || busyModel === editingModel.id}
                onclick={() => void testModelConnection()}>
                {#if busyModel === editingModel.id}<span class="spin"><RefreshCw size={13} /></span>{:else}<Check size={13} />{/if}
                测试连通
              </button>
              <button class="danger-icon" type="button" aria-label="移除模型" title="移除模型"
                onclick={() => { const id = editingModel.id; draft.models = draft.models.filter((candidate) => candidate.id !== id); selectedModelId = null; detailMode = "provider"; }}>
                <Trash2 size={14} />
              </button>
            </span>
          </div>
          <div class="form-grid">
            <label>名称<input value={editingModel.name}
              oninput={(event) => { editingModel.name = event.currentTarget.value; }} /></label>
            <label>模型 ID<input value={editingModel.id} disabled /></label>
            <label>API 类型（留空继承 Provider）
              <select
                value={editingModel.api ?? ""}
                onchange={(event) => { editingModel.api = event.currentTarget.value === "" ? null : event.currentTarget.value; }}>
                <option value="">继承 Provider（{draft.api}）</option>
                {#each API_OPTIONS as option (option.value)}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>
            <label>总上下文窗口（tokens）<input inputmode="numeric" value={editingModel.contextWindow}
              oninput={(event) => setNumberField(editingModel, "contextWindow", event.currentTarget.value)} /></label>
            <label>最大输出（tokens）<input inputmode="numeric" value={editingModel.maxTokens}
              oninput={(event) => setNumberField(editingModel, "maxTokens", event.currentTarget.value)} /></label>
          </div>
          <label class="check-line">
            <input type="checkbox" checked={editingModel.reasoning}
              onchange={(event) => {
                editingModel.reasoning = event.currentTarget.checked;
                if (event.currentTarget.checked && editingModel.thinkingLevels.length === 0) {
                  editingModel.thinkingLevels = ["off", "minimal", "low", "medium", "high"];
                }
                if (!event.currentTarget.checked) editingModel.thinkingLevels = [];
              }} />
            支持推理
          </label>
          {#if editingModel.reasoning}
            <div class="level-row">
              {#each KNOWN_LEVELS as level (level)}
                <label class="check-inline">
                  <input type="checkbox" checked={editingModel.thinkingLevels.includes(level)}
                    onchange={(event) => toggleLevel(editingModel, level, event.currentTarget.checked)} />
                  {level}
                </label>
              {/each}
            </div>
          {/if}
          {#if editingModel.cost}
            <div class="form-grid cost-grid">
              <label>输入价格（$/M）<input inputmode="decimal" value={costValue(editingModel, "input")}
                oninput={(event) => setCostField(editingModel, "input", event.currentTarget.value)} /></label>
              <label>输出价格（$/M）<input inputmode="decimal" value={costValue(editingModel, "output")}
                oninput={(event) => setCostField(editingModel, "output", event.currentTarget.value)} /></label>
              <label>缓存读（$/M）<input inputmode="decimal" value={costValue(editingModel, "cacheRead")}
                oninput={(event) => setCostField(editingModel, "cacheRead", event.currentTarget.value)} /></label>
              <label>缓存写（$/M）<input inputmode="decimal" value={costValue(editingModel, "cacheWrite")}
                oninput={(event) => setCostField(editingModel, "cacheWrite", event.currentTarget.value)} /></label>
            </div>
          {/if}
          {#if modelTest[editingModel.id]}
            <p class="status" class:ok={modelTest[editingModel.id].ok} role="status">{modelTest[editingModel.id].text}</p>
          {/if}
        </section>
      {:else}
        <section class="editor-section">
          <div class="section-heading"><span>Provider</span><span class="section-actions">
            {#if !isNewProvider}<button class="danger-icon" type="button" aria-label="删除 Provider" title="删除 Provider" onclick={() => void removeProvider()}><Trash2 size={14} /></button>{/if}
          </span></div>
          <div class="form-grid">
            <label>Provider 预设
              <select bind:value={providerPreset} onchange={(event) => applyProviderPreset(event.currentTarget.value)}>
                <option value="custom">Custom 自定义</option>
                {#each PROVIDER_PRESETS as preset (preset.key)}
                  <option value={preset.key}>{preset.label}</option>
                {/each}
              </select>
            </label>
            <label>标识<input bind:value={draft.id} disabled={!isCustomProvider || !isNewProvider} placeholder="my-provider" autocomplete="off" /></label>
            <label>名称<input bind:value={draft.name} disabled={!isCustomProvider} placeholder="自定义 Provider" autocomplete="off" /></label>
            <label>API 类型
              <select bind:value={draft.api} disabled={!isCustomProvider}>
                {#each API_OPTIONS as option (option.value)}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            </label>
            <label>Base URL<input bind:value={draft.baseUrl} disabled={!isCustomProvider} placeholder="https://api.example.com/v1" autocomplete="url" /></label>
          </div>
        </section>

        <section class="editor-section request-section">
          <div class="section-heading">
            <span>请求选项</span>
            <button type="button" class="quiet-button compact" onclick={addHeader}><Plus size={13} />添加 Header</button>
          </div>
          <label>代理<input bind:value={draft.proxy} placeholder="http://127.0.0.1:7890" autocomplete="url" /></label>
          {#if headerEntries.length === 0}
            <p class="empty">没有自定义 Header。</p>
          {:else}
            <div class="header-list">
              {#each headerEntries as header, index (index)}
                <div class="header-row">
                  <input bind:value={header.name} aria-label={`Header ${index + 1} 名称`} placeholder="Header 名称" oninput={syncDraftHeaders} />
                  <input bind:value={header.value} aria-label={`Header ${index + 1} 值`} placeholder="Header 值" oninput={syncDraftHeaders} />
                  <button class="danger-icon" type="button" aria-label={`移除 Header ${index + 1}`} title="移除 Header" onclick={() => removeHeader(index)}><Trash2 size={14} /></button>
                </div>
              {/each}
            </div>
          {/if}
        </section>

        <section class="editor-section credential-section">
          <div class="section-heading"><span>Credential Manager</span><span class:configured={credentialConfigured} class="credential-state">{authType === "oauth" ? "Pi OAuth" : nativeAuthConfigured && apiKeyConfigured ? "Pi 登录 + API Key" : credentialConfigured ? "API Key" : "未配置"}</span></div>
          <div class="credential-row">
            <KeyRound size={16} />
            <input type="password" bind:value={apiKey} placeholder={credentialConfigured ? "输入新 Key 以替换" : "API Key"} autocomplete="new-password" />
            <button type="button" class="quiet-button" disabled={!draft.id || !apiKey.trim()} onclick={() => void saveCredential()}><Save size={14} />保存 Key</button>
            <button type="button" class="danger-icon" aria-label="删除 API Key" title="删除 API Key" disabled={!credentialConfigured} onclick={() => void deleteCredential()}><Trash2 size={14} /></button>
            <button type="button" class="quiet-button" disabled={!draft.id || !draft.baseUrl} onclick={() => void testConnection()}><Check size={14} />测试连接</button>
          </div>
          {#if connectionMessage}<p class="status" role="status">{connectionMessage}</p>{/if}
        </section>
      {/if}
    </div>
  </div>
  {#if statusMessage}<p class="page-status" role="status">{statusMessage}</p>{/if}
</section>

<style>
  .provider-page.embedded { padding: 0; }
  .embedded .provider-header { justify-content: flex-end; }
  .embedded .detail-panel { border: 0; border-radius: 0; background: transparent; }
  .provider-page { height: 100%; padding: 14px 18px 18px; display: grid; grid-template-rows: 42px minmax(0, 1fr) 24px; gap: 10px; color: #d8ded9; overflow: hidden; font-family: var(--text-font); }
  .provider-header, .provider-title, .header-actions, .section-heading, .provider-row, .credential-row, .models-toolbar, .model-row { display: flex; align-items: center; }
  .provider-header, .section-heading { justify-content: space-between; }
  .provider-title { gap: 8px; }
  h1, p { margin: 0; }
  h1 { font-size: 16px; font-weight: 650; }
  .header-actions, .section-actions { gap: 6px; }
  .provider-layout { min-height: 0; display: grid; grid-template-columns: 280px minmax(0, 1fr); gap: 14px; overflow: hidden; }
  .provider-list, .detail-panel { min-height: 0; overflow: auto; }
  .provider-list { border-right: 1px solid #303832; padding-right: 12px; }
  .detail-panel { border: 1px solid #303832; border-radius: 5px; background: #161b18; padding: 14px; }
  .section-heading { min-height: 28px; color: #aeb8b0; font-size: 11px; font-weight: 700; letter-spacing: .04em; text-transform: uppercase; }
  .provider-entry { margin-bottom: 2px; }
  .provider-row { width: 100%; gap: 4px; padding: 2px; border: 1px solid transparent; border-radius: 4px; color: #bfc8c1; }
  .provider-row:hover, .provider-row.selected { border-color: #3b5646; background: #202922; color: #f4f7f5; }
  .provider-expand, .provider-open { display: flex; align-items: center; border: 0; background: transparent; color: inherit; cursor: pointer; }
  .provider-expand { width: 22px; height: 30px; flex-shrink: 0; justify-content: center; border-radius: 3px; }
  .provider-expand:hover { color: #f4f7f5; background: #2b352d; }
  .provider-open { flex: 1; min-width: 0; gap: 8px; padding: 4px; text-align: left; }
  .provider-copy { min-width: 0; display: grid; gap: 2px; }
  strong { overflow: hidden; color: #f4f7f5; font-size: 12px; font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  small { overflow: hidden; color: #7f8b82; font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .model-count { min-width: 18px; color: #8fd6ad; font-size: 11px; text-align: right; flex-shrink: 0; }
  .provider-models { margin: 2px 0 8px 26px; display: grid; gap: 2px; }
  .models-toolbar { justify-content: flex-end; min-height: 26px; }
  .model-row { display: flex; align-items: center; gap: 6px; padding: 2px; border: 1px solid transparent; border-radius: 4px; }
  .model-row:hover, .model-row.selected { border-color: #3b5646; background: #202922; }
  .model-row input[type="checkbox"] { width: 14px; height: 14px; flex-shrink: 0; accent-color: #8fd6ad; cursor: pointer; }
  .model-open { flex: 1; min-width: 0; display: grid; gap: 2px; padding: 4px; border: 0; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .editor-section + .editor-section { border-top: 1px solid #303832; margin-top: 14px; padding-top: 12px; }
  .model-detail-section { border-top: 0; margin-top: 0; padding-top: 0; }
  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-top: 10px; }
  label { display: grid; gap: 5px; color: #9aa59c; font-size: 11px; }
  input, select { min-width: 0; height: 30px; padding: 0 9px; border: 1px solid #39433c; border-radius: 4px; outline: none; color: #e7ece8; background: #101411; font: inherit; }
  input:focus, select:focus { border-color: #78bd96; box-shadow: 0 0 0 2px #78bd9622; }
  input:disabled, select:disabled { color: #6e786f; background: #1b201c; }
  input[type="checkbox"] { width: 14px; height: 14px; accent-color: #8fd6ad; cursor: pointer; }
  .check-line { display: flex; align-items: center; gap: 6px; margin-top: 10px; color: #c5cec7; font-size: 12px; }
  .level-row { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 8px; }
  .check-inline { display: inline-flex; align-items: center; gap: 4px; color: #c5cec7; font-size: 11px; }
  button { font: inherit; }
  .icon-button, .danger-icon, .quiet-button, .primary-button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; border-radius: 4px; cursor: pointer; }
  .icon-button, .danger-icon { width: 30px; height: 30px; border: 1px solid transparent; color: #aeb7b0; background: transparent; }
  .icon-button:hover, .danger-icon:hover:not(:disabled) { border-color: #465048; color: #f4f7f5; background: #252b27; }
  .danger-icon { color: #d88989; }
  .quiet-button, .primary-button { min-height: 30px; padding: 0 9px; border: 1px solid #39433c; color: #c5cec7; background: #202721; font-size: 11px; }
  .quiet-button:hover:not(:disabled) { border-color: #607467; color: #f4f7f5; }
  .primary-button { border-color: #75b894; color: #102017; background: #8fd6ad; font-weight: 700; }
  .primary-button:hover:not(:disabled) { background: #a6e6bd; }
  .quiet-button.compact { min-height: 26px; padding: 0 7px; }
  button:disabled { cursor: default; opacity: .45; }
  .request-section > label { margin-bottom: 8px; }
  .header-list { display: grid; gap: 6px; }
  .header-row { display: grid; grid-template-columns: minmax(100px, .8fr) minmax(120px, 1.2fr) 30px; gap: 6px; align-items: center; }
  .credential-row { gap: 7px; }
  .credential-row input { flex: 1; }
  .request-section + .editor-section { margin-top: 14px; }
  .empty { color: #778279; font-size: 11px; }
  .status { margin: 10px 0 0; color: #8fd6ad; font-size: 11px; overflow-wrap: anywhere; }
  .saved-note { color: #8fd6ad; font-size: 11px; white-space: nowrap; }
  .status:not(.ok) { color: #d88989; }
  .page-status { color: #8fd6ad; font-size: 12px; overflow-wrap: anywhere; }
  .spin { display: inline-flex; animation: provider-spin 1s linear infinite; }
  @keyframes provider-spin { to { transform: rotate(360deg); } }
</style>
