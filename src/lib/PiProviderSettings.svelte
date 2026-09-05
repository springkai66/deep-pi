<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Check, KeyRound, Plus, RefreshCw, Save, Search, Server, Trash2, X } from "@lucide/svelte";
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

  const CATALOG_PROVIDERS = [
    ["", "全部 Provider"],
    ["deepseek", "DeepSeek"],
    ["openai", "OpenAI"],
    ["google", "Gemini"],
    ["anthropic", "Anthropic / Claude"],
    ["kimi-coding", "Kimi For Coding"],
    ["opencode", "OpenCode"],
  ];

  let { confirm, onClose, onError }: Props = $props();
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
  let modelQuery = $state("");
  let modelProvider = $state("");
  let modelResults = $state<PiModelSummary[]>([]);
  let providerModels = $state<ProviderModelSummary[]>([]);
  let apiKey = $state("");
  let apiKeyConfigured = $state(false);
  let nativeAuthConfigured = $state(false);
  let authType = $state<string | null>(null);
  let credentialConfigured = $state(false);
  let statusMessage = $state("");
  let connectionMessage = $state("");
  let isLoading = $state(false);
  let isSearching = $state(false);
  let isFetchingProviderModels = $state(false);
  let busyModel = $state<string | null>(null);
  let headerEntries = $state<HeaderEntry[]>([]);

  const isNewProvider = $derived(!providers.some((provider) => provider.id === draft.id));
  const isCustomProvider = $derived(providerPreset === "custom");
  const catalogProvider = $derived(
    PROVIDER_PRESETS.find((preset) => preset.key === providerPreset)?.catalogProvider ?? draft.id,
  );

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
    providerModels = [];
    apiKey = "";
    apiKeyConfigured = false;
    nativeAuthConfigured = false;
    authType = null;
    credentialConfigured = false;
    statusMessage = "";
    connectionMessage = "";
    void refreshCredentialStatus();
  }

  function newProvider() {
    draft = { id: "", name: "", api: "openai-completions", baseUrl: "", headers: {}, proxy: null, models: [] };
    providerPreset = "custom";
    providerModels = [];
    headerEntries = [];
    apiKey = "";
    apiKeyConfigured = false;
    nativeAuthConfigured = false;
    authType = null;
    credentialConfigured = false;
    statusMessage = "";
    connectionMessage = "";
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
      modelProvider = preset.catalogProvider;
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
    providerModels = [];
    headerEntries = [];
    modelProvider = preset.catalogProvider;
    apiKey = "";
    apiKeyConfigured = false;
    nativeAuthConfigured = false;
    authType = null;
    credentialConfigured = false;
    statusMessage = "";
    connectionMessage = "";
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
      connectionMessage = result.reachable ? `可达 · HTTP ${result.status}` : "不可达";
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

  async function refreshProviderModels() {
    if (!draft.id || !draft.baseUrl) {
      onError("请先选择 Provider 或填写 Base URL");
      return;
    }
    isFetchingProviderModels = true;
    statusMessage = "";
    try {
      const saved = await saveProvider(false);
      if (!saved) return;
      providerModels = await invoke<ProviderModelSummary[]>("list_provider_models", {
        request: { providerId: saved.id },
      });
      statusMessage = `已拉取 ${providerModels.length} 个 Provider 模型`;
    } catch (error) {
      onError(error);
    } finally {
      isFetchingProviderModels = false;
    }
  }

  async function searchModels() {
    isSearching = true;
    try {
      modelResults = await invoke<PiModelSummary[]>("search_pi_models", {
        request: { query: modelQuery.trim(), provider: modelProvider || null },
      });
    } catch (error) {
      onError(error);
    } finally {
      isSearching = false;
    }
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
      thinkingLevels: profile.thinkingLevels,
      cost: profile.cost,
    };
  }

  async function officialSummaryFor(modelId: string): Promise<PiModelSummary | undefined> {
    const results = await invoke<PiModelSummary[]>("search_pi_models", {
      request: { query: modelId, provider: catalogProvider.trim() || null },
    });
    return results.find((candidate) => candidate.id === modelId);
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
        upsertModel({
          id: summary.id,
          name: summary.name,
          reasoning: summary.reasoning ?? false,
          input: summary.input.length > 0 ? summary.input : ["text"],
          contextWindow: summary.contextWindow ?? 128000,
          maxTokens: summary.maxTokens ?? 8192,
          thinkingLevels: summary.reasoning ? ["off", "minimal", "low", "medium", "high"] : [],
          cost: null,
        });
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

  async function addModel(summary: PiModelSummary) {
    busyModel = summary.id;
    try {
      const profile = await invoke<PiModelProfile>("pi_model_profile", {
        request: { path: summary.path, provider: summary.provider, modelId: summary.id },
      });
      upsertModel(modelFromProfile(profile));
      statusMessage = `${profile.name} 已填入模型列表`;
    } catch (error) {
      onError(error);
    } finally {
      busyModel = null;
    }
  }

  function removeModel(modelId: string) {
    draft.models = draft.models.filter((model) => model.id !== modelId);
  }

  function formatContext(value: number | null) {
    if (value === null) return "未知";
    return value >= 1_000_000 ? `${(value / 1_000_000).toFixed(1)}M` : `${Math.round(value / 1_000)}K`;
  }

  function formatCost(value: number | null) {
    return value === null ? "-" : `$${value}/M`;
  }
</script>

<section class="provider-page" aria-label="Pi Provider 和模型配置">
  <header class="provider-header">
    <div class="provider-title">
      <button class="icon-button" type="button" aria-label="返回工作区" title="返回" onclick={onClose}><X size={17} /></button>
      <Server size={18} />
      <h1>Provider 与模型</h1>
    </div>
    <div class="header-actions">
      <button type="button" class="quiet-button" onclick={newProvider}><Plus size={14} />新建 Provider</button>
      <button type="button" class="primary-button" disabled={isLoading} onclick={() => void saveProvider}><Save size={14} />保存</button>
    </div>
  </header>

  <div class="provider-layout">
    <aside class="provider-list" aria-label="Provider 列表">
      <div class="section-heading"><span>已配置 Provider</span><span>{providers.length}</span></div>
      {#if providers.length === 0}
        <p class="empty">还没有 Provider</p>
      {:else}
        {#each providers as provider (provider.id)}
          <button class:selected={provider.id === draft.id} class="provider-row" type="button" onclick={() => selectProvider(provider)}>
            <span><strong>{provider.name || provider.id}</strong><small>{provider.id}</small></span>
            <span class="model-count">{provider.models.length}</span>
          </button>
        {/each}
      {/if}
    </aside>

    <div class="provider-editor">
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
        <p class="credential-note">API Key 保存在 Windows Credential Manager；Claude Code、ChatGPT 等订阅/OAuth 登录请在 Pi 会话中使用 /login。</p>
        {#if connectionMessage}<p class="status" role="status">{connectionMessage}</p>{/if}
      </section>

      <section class="editor-section provider-model-section">
        <div class="section-heading">
          <span>Provider 模型</span>
          <button
            type="button"
            class="quiet-button compact"
            disabled={!draft.id || !draft.baseUrl || isFetchingProviderModels || isLoading}
            onclick={() => void refreshProviderModels()}
          >
            {#if isFetchingProviderModels}<span class="spin"><RefreshCw size={13} /></span>{:else}<RefreshCw size={13} />{/if}
            拉取模型
          </button>
        </div>
        {#if isFetchingProviderModels}
          <p class="empty">正在请求 {draft.name || draft.id} 的模型列表…</p>
        {:else if !draft.id}
          <p class="empty">先选择或保存一个 Provider。</p>
        {:else if providerModels.length === 0}
          <p class="empty">点击“拉取模型”读取 Provider 的 /models。</p>
        {:else}
          <div class="provider-models">
            {#each providerModels as model (model.id)}
              <article class="provider-model">
                <div class="model-main"><strong>{model.name}</strong><small>{model.id}{model.input.length > 0 ? ` · ${model.input.join(" / ")}` : ""}</small></div>
                <div class="model-meta">
                  <span>{formatContext(model.contextWindow)}</span>
                  <span>{model.reasoning === true ? "推理" : model.reasoning === false ? "普通" : "待官方补全"}</span>
                </div>
                <button type="button" class="primary-button compact" disabled={busyModel === model.id} onclick={() => void addProviderModel(model)}>
                  {#if busyModel === model.id}<span class="spin"><RefreshCw size={13} /></span>{:else}<Plus size={13} />{/if}填入
                </button>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      <section class="editor-section model-section">
        <div class="section-heading"><span>已填入模型</span><span>{draft.models.length}</span></div>
        {#if draft.models.length === 0}
          <p class="empty">从官方目录选择模型后，一键填入这里。</p>
        {:else}
          <div class="configured-models">
            {#each draft.models as model (model.id)}
              <article class="configured-model">
                <div class="model-main"><strong>{model.name}</strong><small>{model.id} · {model.input.join(" / ")}</small></div>
                <div class="model-meta"><span>{formatContext(model.contextWindow)}</span><span>{model.reasoning ? `推理：${model.thinkingLevels.join(" / ") || "默认"}` : "普通"}</span></div>
                <button class="danger-icon" type="button" aria-label={`移除 ${model.name}`} title="移除模型" onclick={() => removeModel(model.id)}><Trash2 size={14} /></button>
              </article>
            {/each}
          </div>
        {/if}
      </section>
    </div>

    <section class="catalog-panel" aria-label="Pi 官方模型目录">
      <div class="section-heading"><span>官方模型目录</span><button class="icon-button" type="button" aria-label="刷新模型目录" title="刷新" onclick={() => void searchModels()}><RefreshCw size={14} /></button></div>
      <form class="catalog-search" onsubmit={(event) => { event.preventDefault(); void searchModels(); }}>
        <Search size={15} />
        <input bind:value={modelQuery} aria-label="搜索模型" placeholder="搜索模型名称或 ID" />
        <select bind:value={modelProvider} aria-label="筛选 Provider">
          {#each CATALOG_PROVIDERS as [id, label]}
            <option value={id}>{label}</option>
          {/each}
        </select>
        <button type="submit" aria-label="搜索" title="搜索" disabled={isSearching}><Search size={14} /></button>
      </form>
      {#if isSearching}
        <p class="empty">正在加载模型目录…</p>
      {:else if modelResults.length === 0}
        <p class="empty">输入关键词搜索 pi.dev/models。</p>
      {:else}
        <div class="catalog-list">
          {#each modelResults as model (model.path)}
            <article class="catalog-model">
              <div class="catalog-model-main"><strong>{model.name}</strong><small>{model.provider} · {model.id}</small></div>
              <div class="catalog-meta"><span>{formatContext(model.contextWindow)}</span><span>{formatCost(model.inputCost)} / {formatCost(model.outputCost)}</span></div>
              <button type="button" class="primary-button compact" disabled={busyModel === model.id} onclick={() => void addModel(model)}>
                {#if busyModel === model.id}<span class="spin"><RefreshCw size={13} /></span>{:else}<Plus size={13} />{/if}填入
              </button>
            </article>
          {/each}
        </div>
      {/if}
    </section>
  </div>
  {#if statusMessage}<p class="page-status" role="status">{statusMessage}</p>{/if}
</section>

<style>
  .provider-page { height: 100%; padding: 14px 18px 18px; display: grid; grid-template-rows: 42px minmax(0, 1fr) 24px; gap: 10px; color: #d8ded9; overflow: hidden; font-family: var(--text-font); }
  .provider-header, .provider-title, .header-actions, .section-heading, .provider-row, .credential-row, .configured-model, .provider-model, .catalog-model, .catalog-search { display: flex; align-items: center; }
  .provider-header, .section-heading, .configured-model, .provider-model, .catalog-model { justify-content: space-between; }
  .provider-title { gap: 8px; }
  h1, p { margin: 0; }
  h1 { font-size: 16px; font-weight: 650; }
  .header-actions, .section-actions { gap: 6px; }
  .provider-layout { min-height: 0; display: grid; grid-template-columns: 190px minmax(320px, 1fr) minmax(300px, 0.9fr); gap: 14px; overflow: hidden; }
  .provider-list, .provider-editor, .catalog-panel { min-height: 0; overflow: auto; }
  .provider-list { border-right: 1px solid #303832; padding-right: 12px; }
  .provider-editor, .catalog-panel { border: 1px solid #303832; border-radius: 5px; background: #161b18; }
  .provider-editor { padding: 14px; }
  .catalog-panel { padding: 12px; }
  .section-heading { min-height: 28px; color: #aeb8b0; font-size: 11px; font-weight: 700; letter-spacing: .04em; text-transform: uppercase; }
  .provider-row { width: 100%; justify-content: space-between; gap: 8px; padding: 9px 7px; border: 1px solid transparent; border-radius: 4px; color: #bfc8c1; background: transparent; text-align: left; cursor: pointer; }
  .provider-row:hover, .provider-row.selected { border-color: #3b5646; background: #202922; color: #f4f7f5; }
  .provider-row span:first-child, .catalog-model-main, .model-main { min-width: 0; display: grid; gap: 3px; }
  strong { overflow: hidden; color: #f4f7f5; font-size: 12px; font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  small { overflow: hidden; color: #7f8b82; font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .model-count { min-width: 18px; color: #8fd6ad; font-size: 11px; text-align: right; }
  .editor-section + .editor-section { border-top: 1px solid #303832; margin-top: 14px; padding-top: 12px; }
  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  label { display: grid; gap: 5px; color: #9aa59c; font-size: 11px; }
  input, select { min-width: 0; height: 30px; padding: 0 9px; border: 1px solid #39433c; border-radius: 4px; outline: none; color: #e7ece8; background: #101411; font: inherit; }
  input:focus, select:focus { border-color: #78bd96; box-shadow: 0 0 0 2px #78bd9622; }
  input:disabled, select:disabled { color: #6e786f; background: #1b201c; }
  button { font: inherit; }
  .icon-button, .danger-icon, .catalog-search button, .quiet-button, .primary-button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; border-radius: 4px; cursor: pointer; }
  .icon-button, .danger-icon { width: 30px; height: 30px; border: 1px solid transparent; color: #aeb7b0; background: transparent; }
  .icon-button:hover, .danger-icon:hover:not(:disabled) { border-color: #465048; color: #f4f7f5; background: #252b27; }
  .danger-icon { color: #d88989; }
  .quiet-button, .primary-button { min-height: 30px; padding: 0 9px; border: 1px solid #39433c; color: #c5cec7; background: #202721; font-size: 11px; }
  .quiet-button:hover:not(:disabled) { border-color: #607467; color: #f4f7f5; }
  .primary-button { border-color: #75b894; color: #102017; background: #8fd6ad; font-weight: 700; }
  .primary-button:hover:not(:disabled) { background: #a6e6bd; }
  .primary-button.compact, .quiet-button.compact { min-height: 26px; padding: 0 7px; }
  button:disabled { cursor: default; opacity: .45; }
  .request-section > label { margin-bottom: 8px; }
  .header-list { display: grid; gap: 6px; }
  .header-row { display: grid; grid-template-columns: minmax(100px, .8fr) minmax(120px, 1.2fr) 30px; gap: 6px; align-items: center; }
  .credential-row { gap: 7px; }
  .credential-row input { flex: 1; }
  .credential-note { margin: 8px 0 0 23px; color: #778279; font-size: 10px; line-height: 1.45; }
  .request-section + .editor-section { margin-top: 14px; }

  .credential-state { color: #7f8b82; font-size: 10px; }
  .credential-state.configured { color: #8fd6ad; }
  .status, .page-status { color: #8fd6ad; font-size: 11px; }
  .status { margin: 8px 0 0 23px; }
  .empty { padding: 18px 4px; color: #778279; font-size: 11px; }
  .configured-models, .provider-models, .catalog-list { display: grid; gap: 6px; }
  .configured-model, .provider-model, .catalog-model { gap: 8px; padding: 8px 0; border-bottom: 1px solid #283029; }
  .configured-model:last-child, .provider-model:last-child, .catalog-model:last-child { border-bottom: 0; }
  .model-main, .catalog-model-main { flex: 1; }
  .model-meta, .catalog-meta { display: grid; gap: 3px; color: #94a097; font-size: 10px; text-align: right; white-space: nowrap; }
  .catalog-search { gap: 5px; padding: 7px 0 10px; }
  .catalog-search input:first-of-type { flex: 1; }
  .catalog-search input, .catalog-search select { width: 120px; height: 28px; font-size: 11px; }
  .catalog-search button { width: 28px; height: 28px; border: 1px solid #39433c; color: #bfc8c1; background: #202721; cursor: pointer; }
  .provider-model, .catalog-model { align-items: center; }
  .catalog-model-main { min-width: 100px; }
  .page-status { align-self: center; }
  .spin { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1000px) { .provider-layout { grid-template-columns: 160px minmax(280px, 1fr); } .catalog-panel { grid-column: 1 / -1; max-height: 260px; } }
  @media (max-width: 680px) { .provider-page { padding: 10px; } .provider-layout { display: flex; flex-direction: column; } .provider-list { max-height: 150px; border-right: 0; border-bottom: 1px solid #303832; padding: 0 0 8px; } .provider-editor, .catalog-panel { flex: 1; } .form-grid { grid-template-columns: 1fr; } .credential-row { flex-wrap: wrap; } .credential-row input { flex-basis: 100%; } .header-row { grid-template-columns: 1fr 1fr 30px; } }
</style>
