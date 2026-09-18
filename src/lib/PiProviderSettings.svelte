<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import {
    Check,
    KeyRound,
    Pencil,
    Plus,
    RefreshCw,
    Save,
    Search,
    Server,
    Trash2,
    X,
    Zap,
  } from "@lucide/svelte";
  import { notifyModelsChanged } from "./model-config-sync";
  import PiAuthSettings from "./PiAuthSettings.svelte";
  import PiOfficialLoginFlow from "./PiOfficialLoginFlow.svelte";
  import { type PiAuthProviderInfo, type PiAuthStatusResponse } from "./pi-auth";
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
  import { t, tm } from "$lib/i18n.svelte";

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
  let fetchedModels = $state<Record<string, ProviderModelSummary[]>>({});
  let detailMode = $state<"provider" | "model">("provider");
  let selectedModelId = $state<string | null>(null);
  let modelTest = $state<Record<string, ModelTestMessage>>({});
  /// 保存成功后快照的草稿；草稿与快照一致时保存按钮置灰并提示已保存。
  let savedSnapshot = $state<string | null>(null);
  let autofillMessage = $state("");
  let autofillModelId = $state<string | null>(null);
  let manualAddVisible = $state(false);
  let manualModelId = $state("");
  let manualModelName = $state("");
  /// 编辑弹窗：编辑中的供应商与高级设置（Provider 字段）面板的显隐。
  let editorOpen = $state(false);
  let showAdvanced = $state(false);
  /// 弹窗左侧模型列表的搜索词。
  let modelSearch = $state("");
  const isNewProvider = $derived(!providers.some((provider) => provider.id === draft.id));
  /// 官方供应商可用模型（pi 运行时自带目录）；只读展示，不写回配置。
  interface PiOfficialModel {
    id: string;
    name: string;
    contextWindow: number | null;
    maxTokens: number | null;
    reasoning: boolean;
    input: string[];
    inputCost: number | null;
    outputCost: number | null;
  }

  interface OfficialLoginMethod {
    id: string;
    label: string;
  }

  /// 各官方供应商实际支持的登录方式（对照 pi 运行时实现）：
  /// openai-codex 支持网页与设备码；anthropic / openrouter 为浏览器授权 + 粘贴回调；
  /// github-copilot / kimi-coding / radius / xai 走设备码；未知供应商交给 pi 默认流程。
  const OFFICIAL_LOGIN_METHODS: Record<string, OfficialLoginMethod[]> = {
    "openai-codex": [
      { id: "browser", label: "网页登录（本机回调授权）" },
      { id: "device_code", label: "设备码登录（无浏览器）" },
    ],
    anthropic: [{ id: "browser", label: "网页登录（浏览器授权 + 粘贴回调）" }],
    openrouter: [{ id: "browser", label: "网页登录（浏览器授权 + 粘贴回调）" }],
    "github-copilot": [{ id: "device_code", label: "设备码登录（浏览器输入设备码）" }],
    "kimi-coding": [{ id: "device_code", label: "设备码登录（浏览器输入设备码）" }],
    radius: [{ id: "device_code", label: "设备码登录（浏览器输入设备码）" }],
    xai: [{ id: "device_code", label: "设备码登录（浏览器输入设备码）" }],
  };
  const DEFAULT_OFFICIAL_METHODS: OfficialLoginMethod[] = [
    { id: "default", label: "由 pi 决定（默认登录流程）" },
  ];

  /// 新建/查看官方模型弹窗：类型选择、登录步骤与只读模型目录。
  let createOpen = $state(false);
  let createStep = $state<"form" | "login" | "models">("form");
  let createKind = $state<"official" | "custom">("official");
  let createBusy = $state(false);
  let createMessage = $state("");
  let officialProviders = $state<PiAuthProviderInfo[]>([]);
  let signedInIds = $state<Set<string>>(new Set());
  let officialProviderId = $state("");
  let officialProviderName = $state("");
  let officialLoginMethod = $state<string | null>(null);
  let officialModels = $state<PiOfficialModel[]>([]);
  let officialModelsBusy = $state(false);
  let officialModelsError = $state("");
  /// 官方分组刷新令牌：登录/退出后递增，驱动 PiAuthSettings 重新拉取状态。
  let authRefreshToken = $state(0);
  const officialMethods = $derived(OFFICIAL_LOGIN_METHODS[officialProviderId] ?? DEFAULT_OFFICIAL_METHODS);
  const createReady = $derived(
    createKind === "official"
      ? officialProviderId !== ""
      : draft.id.trim() !== "" && (draft.baseUrl ?? "").trim() !== "",
  );
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
    // Traverse nested model proxies without writing state from a derived.
    return JSON.stringify({
      draft: { ...draft, headers: draftHeaders() },
      apiKey,
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
        cost: model.cost ? { ...model.cost } : null,
      })),
    };
  }

  function setHeaderEntries(headers: Record<string, string>) {
    headerEntries = Object.entries(headers).map(([name, value]) => ({ name, value }));
  }

  function draftHeaders() {
    return Object.fromEntries(
      headerEntries
        .map(({ name, value }) => [name.trim(), value] as const)
        .filter(([name]) => name.length > 0),
    );
  }

  function syncDraftHeaders() {
    draft.headers = draftHeaders();
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
    savedSnapshot = draftSnapshot();
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

  /// 打开某个供应商的编辑弹窗：草稿未指向该供应商时先载入，弹窗内共享同一草稿。
  function openProviderEditor(provider: ProviderRecord) {
    if (draft.id !== provider.id) selectProvider(provider);
    detailMode = "provider";
    selectedModelId = null;
    modelSearch = "";
    manualAddVisible = false;
    showAdvanced = false;
    editorOpen = true;
  }

  /// 新建 Provider 弹窗：先选类型（官方 / 自定义）。官方走登录流程，自定义
  /// 保存后切换到模型拉取弹窗；官方模型目录为只读展示。
  async function openCreateModal() {
    newProvider();
    manualAddVisible = false;
    modelSearch = "";
    createOpen = true;
    createStep = "form";
    createKind = "official";
    createMessage = "";
    officialProviderId = "";
    officialProviderName = "";
    officialLoginMethod = null;
    officialModels = [];
    officialModelsError = "";
    officialProviders = [];
    signedInIds = new Set();
    apiKey = "";
    createBusy = true;
    try {
      const [providerList, status] = await Promise.all([
        invoke<PiAuthProviderInfo[]>("pi_auth_providers"),
        invoke<PiAuthStatusResponse>("pi_auth_status"),
      ]);
      officialProviders = providerList.filter((provider) => provider.oauth);
      signedInIds = new Set(status.credentials.map((credential) => credential.provider));
    } catch (error) {
      onError(error);
    } finally {
      createBusy = false;
    }
  }

  function closeCreate() {
    createOpen = false;
    createStep = "form";
    createBusy = false;
    // 放弃的新草稿不留在「未保存」状态，避免保存按钮指向一个空 Provider。
    if (isNewProvider) savedSnapshot = draftSnapshot();
  }

  /// 切换官方供应商：登录方式默认取该供应商支持的第一种（未命中则交给 pi 默认流程）。
  function selectOfficialProvider(providerId: string) {
    officialProviderId = providerId;
    const methods = OFFICIAL_LOGIN_METHODS[providerId] ?? DEFAULT_OFFICIAL_METHODS;
    officialLoginMethod = methods[0]?.id ?? null;
  }

  /// 官方：直接进入登录流程；自定义：保存后切到模型拉取弹窗（编辑弹窗本体）。
  async function submitCreate() {
    if (!createReady || createBusy) return;
    if (createKind === "official") {
      officialProviderName =
        officialProviders.find((provider) => provider.id === officialProviderId)?.name ?? officialProviderId;
      if (!officialLoginMethod) officialLoginMethod = officialMethods[0]?.id ?? null;
      createMessage = "";
      createStep = "login";
      return;
    }
    createBusy = true;
    try {
      const saved = await saveProvider(false);
      if (!saved) return;
      createOpen = false;
      createStep = "form";
      manualAddVisible = false;
      detailMode = "provider";
      selectedModelId = null;
      showAdvanced = false;
      editorOpen = true;
      statusMessage = t("已创建 {name}，请选择要启用的模型", { name: saved.name || saved.id });
    } finally {
      createBusy = false;
    }
  }

  /// 官方登录成功：刷新官方分组，并切到只读模型目录步骤。
  async function onOfficialLoggedIn(providerId: string) {
    authRefreshToken += 1;
    officialProviderId = providerId;
    officialProviderName =
      officialProviders.find((provider) => provider.id === providerId)?.name ?? providerId;
    createStep = "models";
    await loadOfficialModels();
  }

  /// 从官方分组直接查看某个已登录供应商的模型目录。
  async function openOfficialModels(providerId: string, providerName: string) {
    officialProviderId = providerId;
    officialProviderName = providerName;
    officialModels = [];
    officialModelsError = "";
    createKind = "official";
    createStep = "models";
    createOpen = true;
    await loadOfficialModels();
  }

  async function loadOfficialModels() {
    if (!officialProviderId) return;
    officialModelsBusy = true;
    officialModelsError = "";
    try {
      officialModels = await invoke<PiOfficialModel[]>("pi_auth_provider_models", {
        request: { providerId: officialProviderId },
      });
    } catch (error) {
      officialModelsError = tm(String(error));
      onError(error);
    } finally {
      officialModelsBusy = false;
    }
  }

  function closeProviderEditor() {
    editorOpen = false;
  }

  function closeOnBackdrop(event: MouseEvent) {
    if (event.target === event.currentTarget) closeProviderEditor();
  }

  function handleEditorKeydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (event.key !== "Escape") return;
    if (createOpen) {
      closeCreate();
      return;
    }
    if (editorOpen) closeProviderEditor();
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
    if (isLoading) return null;
    const submittedDraft = draft;
    const submittedSnapshot = draftSnapshot();
    // 是否有真实变更：决定是否广播（无变更的保存不值得让所有对话重载）。
    const hadChanges = savedSnapshot === null || submittedSnapshot !== savedSnapshot;
    const submitted = JSON.parse(submittedSnapshot) as {
      draft: ProviderRecord; apiKey: string; providerPreset: string;
    };
    if (showStatus) statusMessage = "";
    try {
      const saved = await invoke<ProviderRecord>("save_pi_provider", {
        request: {
          ...submitted.draft,
          baseUrl: submitted.draft.baseUrl?.trim() || null,
          proxy: submitted.draft.proxy?.trim() || null,
        },
      });
      if (submitted.apiKey.trim()) {
        // Let credential failures reach the same failure path; keep the draft retryable.
        await invoke<ProviderCredentialStatus>("save_provider_credential", {
          request: { providerId: saved.id, apiKey: submitted.apiKey },
        });
      }
      providers = [saved, ...providers.filter((provider) => provider.id !== saved.id)];
      if (draft === submittedDraft) {
        const unchanged = draftSnapshot() === submittedSnapshot;
        if (unchanged) {
          draft = cloneProvider(saved);
          setHeaderEntries(saved.headers);
        }
        if (apiKey === submitted.apiKey) apiKey = "";
        savedSnapshot = JSON.stringify({ draft: saved, apiKey: "", providerPreset: submitted.providerPreset });
        await refreshCredentialStatus();
        if (showStatus && draft === (unchanged ? draft : submittedDraft)) {
          statusMessage = t("Provider 已保存；模型变更将在重启任务后出现在对话窗口");
        }
      }
      // 真实变更落盘后广播：空闲的对话面板会自动重载并读到新的模型/推理强度。
      if (hadChanges) notifyModelsChanged();
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
      onError(t("请先保存 Provider"));
      return;
    }
    if (!apiKey.trim()) {
      onError(t("API Key 不能为空"));
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
      if (showStatus) statusMessage = t("API Key 已保存到 Windows Credential Manager");
      // 凭据变化会改变「已配置」的 provider 集合，同样值得让对话面板知道。
      notifyModelsChanged();
    } catch (error) {
      onError(error);
    }
  }

  async function deleteCredential() {
    if (!draft.id || !(await confirm(t("删除 API Key"), t("删除 {name} 的 Windows 凭据吗？", { name: draft.name || draft.id }), t("删除")))) return;
    try {
      await invoke("delete_provider_credential", { request: { providerId: draft.id } });
      apiKeyConfigured = false;
      credentialConfigured = nativeAuthConfigured;
      if (!nativeAuthConfigured) authType = null;
      statusMessage = t("API Key 已删除");
      notifyModelsChanged();
    } catch (error) {
      onError(error);
    }
  }

  async function testConnection() {
    if (!draft.id) {
      onError(t("请先保存 Provider"));
      return;
    }
    connectionMessage = t("正在测试…");
    try {
      const saved = await saveProvider(false);
      if (!saved) return;
      const result = await invoke<ProviderConnectionResult>("test_provider_connection", {
        request: { providerId: saved.id },
      });
      connectionMessage = result.reachable
        ? t("可达 · HTTP {status}", { status: result.status })
        : t("不可达 · HTTP {status}（检查 Base URL 与 API Key）", { status: result.status });
    } catch (error) {
      connectionMessage = t("连接失败");
      onError(error);
    }
  }

  async function removeProvider(provider: ProviderRecord) {
    if (!provider.id) return;
    if (!(await confirm(t("删除 Provider"), t("删除 {name} 及其模型配置吗？", { name: provider.name || provider.id }), t("删除")))) return;
    try {
      await invoke("delete_pi_provider", { request: { id: provider.id } });
      await invoke("delete_provider_credential", { request: { providerId: provider.id } });
      providers = providers.filter((candidate) => candidate.id !== provider.id);
      fetchedModels = Object.fromEntries(
        Object.entries(fetchedModels).filter(([key]) => key !== provider.id),
      );
      if (draft.id === provider.id) {
        newProvider();
        closeProviderEditor();
      }
      statusMessage = t("Provider 已删除");
    } catch (error) {
      onError(error);
    }
  }

  function configuredModelsOf(provider: ProviderRecord): ConfiguredModel[] {
    return provider.id === draft.id ? draft.models : provider.models;
  }

  /// 弹窗左侧的模型行：草稿中已配置的模型在前，拉取到但未配置的在后，再按搜索词过滤。
  function modalModelRows(): ModelRow[] {
    const rows: ModelRow[] = draft.models.map((model) => ({
      id: model.id,
      name: model.name,
      sub: `${model.id} · ${formatContext(model.contextWindow)}${model.reasoning ? ` · ${t("推理")}` : ""}${model.cost ? ` · ${formatCost(model.cost.input)}/${formatCost(model.cost.output)}` : ""}`,
      configured: true,
      summary: null,
    }));
    for (const summary of fetchedModels[draft.id] ?? []) {
      if (draft.models.some((model) => model.id === summary.id)) continue;
      rows.push({
        id: summary.id,
        name: summary.name,
        sub: `${summary.id} · ${formatContext(summary.contextWindow)}`,
        configured: false,
        summary,
      });
    }
    const query = modelSearch.trim().toLowerCase();
    if (!query) return rows;
    return rows.filter((row) =>
      row.id.toLowerCase().includes(query) ||
      row.name.toLowerCase().includes(query) ||
      row.sub.toLowerCase().includes(query));
  }

  async function refreshProviderModels() {
    if (!draft.id || !draft.baseUrl) {
      onError(t("请先选择 Provider 或填写 Base URL"));
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
      statusMessage = t("已拉取 {count} 个模型", { count: models.length });
    } catch (error) {
      onError(error);
    } finally {
      isFetchingProviderModels = false;
    }
  }

  /// 目录候选排序：完全相等 → 补 provider 前缀后相等 → 末段相等 → 前缀命中；同一优先级
  /// 取最短 id（最接近用户输入，避免落到 `-20250929` 这类长日期变体），长度相同按字典序，
  /// 保证结果稳定且大小写不敏感。
  function pickCatalogMatch(
    candidates: PiModelSummary[],
    modelId: string,
    provider: string,
  ): PiModelSummary | undefined {
    const wanted = modelId.trim().toLowerCase();
    if (wanted === "") return undefined;
    const providerKey = provider.trim().toLowerCase();
    const prefixed = providerKey === "" ? "" : `${providerKey}/${wanted}`;
    const ranked: { priority: number; candidate: PiModelSummary }[] = [];
    for (const candidate of candidates) {
      const id = candidate.id.trim().toLowerCase();
      const name = candidate.name.trim().toLowerCase();
      const tail = id.slice(id.lastIndexOf("/") + 1);
      let priority: number | null = null;
      if (id === wanted) priority = 0;
      else if (prefixed !== "" && id === prefixed) priority = 1;
      else if (tail === wanted) priority = 2;
      else if (id.startsWith(wanted) || name.startsWith(wanted)) priority = 3;
      if (priority !== null) ranked.push({ priority, candidate });
    }
    ranked.sort(
      (left, right) =>
        left.priority - right.priority ||
        left.candidate.id.length - right.candidate.id.length ||
        left.candidate.id.toLowerCase().localeCompare(right.candidate.id.toLowerCase()),
    );
    return ranked[0]?.candidate;
  }

  /// 目录查询：先带 provider 过滤；models.dev 的 provider key 与预设/手填的 id 并不总是一致
  /// （如 `kimi-coding` 实际是 `kimi-for-coding`，`moonshot` 实际是 `moonshotai`），过滤后命中
  /// 不到就再不带 provider 过滤查一次回退，避免「自动填入」永远失败。
  async function officialSummaryFor(modelId: string): Promise<PiModelSummary | undefined> {
    const provider = catalogProvider.trim();
    const filtered = await invoke<PiModelSummary[]>("search_pi_models", {
      request: { query: modelId, provider: provider || null },
    });
    const matched = pickCatalogMatch(filtered, modelId, provider);
    if (matched || !provider) return matched;
    const fallback = await invoke<PiModelSummary[]>("search_pi_models", {
      request: { query: modelId, provider: null },
    });
    return pickCatalogMatch(fallback, modelId, provider);
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

  /// 手动添加模型：供应商 /models 接口不可用时，用户可以直接录入模型并保存。
  function addManualModel() {
    const id = manualModelId.trim();
    if (!id) {
      onError(t("请输入模型 ID"));
      return;
    }
    if (draft.models.some((candidate) => candidate.id === id)) {
      onError(t("模型 {id} 已存在", { id }));
      return;
    }
    const model: ConfiguredModel = {
      id,
      name: manualModelName.trim() || id,
      reasoning: false,
      input: ["text"],
      contextWindow: 128000,
      maxTokens: 8192,
      thinkingLevels: [],
      cost: null,
      api: null,
    };
    upsertModel(model);
    manualModelId = "";
    manualModelName = "";
    statusMessage = t("模型 {name} 已添加，记得保存 Provider", { name: model.name });
    selectedModelId = id;
    detailMode = "model";
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
        statusMessage = t("{name} 已从 pi.dev/models 补全并填入", { name: profile.name });
      } else {
        upsertModel(summaryToConfigured(summary));
        statusMessage = catalogUnavailable
          ? t("{name} 已填入，官方模型目录暂不可用", { name: summary.name })
          : t("{name} 已填入，pi.dev/models 暂无对应详情", { name: summary.name });
      }
    } catch (error) {
      onError(error);
    } finally {
      busyModel = null;
    }
  }

  async function toggleModel(row: ModelRow) {
    if (row.configured) {
      draft.models = draft.models.filter((model) => model.id !== row.id);
      if (selectedModelId === row.id) {
        selectedModelId = null;
        detailMode = "provider";
      }
      statusMessage = t("{name} 已取消选择", { name: row.name });
      return;
    }
    if (!row.summary) return;
    await addProviderModel(row.summary);
    selectedModelId = row.id;
    detailMode = "model";
  }

  function openModelDetail(row: ModelRow) {
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

  /// 价格允许留空（`null`）；空字符串写回 `null`，非法（非有限数/负数，与 Rust 端
  /// `validate_cost` 一致）时忽略输入。`cost` 为 null 时按四项全空兜底建对象。
  function setCostField(model: ConfiguredModel, field: "input" | "output" | "cacheRead" | "cacheWrite", raw: string) {
    const trimmed = raw.trim();
    const value = trimmed === "" ? null : Number(trimmed);
    if (value !== null && (!Number.isFinite(value) || value < 0)) return;
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
        autofillMessage = t("目录暂无详情");
        statusMessage = t("官方模型目录暂无该模型详情");
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
      autofillMessage = t("已自动填入");
      statusMessage = t("{id} 已自动填入官方参数", { id: model.id });
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
    modelTest = { ...modelTest, [id]: { ok: false, text: t("正在测试…") } };
    try {
      const saved = await saveProvider(false);
      if (!saved) {
        modelTest = { ...modelTest, [id]: { ok: false, text: t("Provider 保存失败") } };
        return;
      }
      const result = await invoke<{ ok: boolean; status: number | null; latencyMs: number | null; error: string | null }>(
        "test_model_connection",
        { request: { providerId: saved.id, modelId: id } },
      );
      modelTest = {
        ...modelTest,
        [id]: result.ok
          ? { ok: true, text: t("连通 · HTTP {status} · {latency}ms", { status: result.status ?? "", latency: result.latencyMs ?? 0 }) }
          : { ok: false, text: result.error ?? t("连接失败") },
      };
    } catch (error) {
      modelTest = { ...modelTest, [id]: { ok: false, text: tm(String(error)) } };
    }
  }

  function formatContext(value: number | null) {
    if (value === null) return t("未知");
    return value >= 1_000_000 ? `${(value / 1_000_000).toFixed(1)}M` : `${Math.round(value / 1_000)}K`;
  }

  function formatCost(value: number | null | undefined) {
    if (value === null || value === undefined) return "-";
    return `$${value}/M`;
  }
</script>

<svelte:window onkeydown={handleEditorKeydown} />

<section class="provider-page" class:embedded aria-label={t("Pi Provider 和模型配置")}>
  <header class="provider-header">
    {#if !embedded}<div class="provider-title">
      <button class="icon-button" type="button" aria-label={t("返回工作区")} title={t("返回")} onclick={onClose}><X size={17} /></button>
      <Server size={18} />
      <h1>{t("Provider 与模型")}</h1>
    </div>{/if}
    <div class="header-actions">
      {#if savedSnapshot !== null && !draftDirty}
        <span class="saved-note" role="status">{t("已保存")}</span>
      {/if}
      <button type="button" class="quiet-button" onclick={() => void openCreateModal()}>
        <Plus size={13} />{t("新建 Provider")}
      </button>
      <button type="button" class="primary-button" disabled={!saveReady}
        title={saveReady ? "" : t("没有未保存的修改")}
        onclick={() => void saveProvider()}><Save size={14} />{t("保存")}</button>
    </div>
  </header>

  <!-- 官方供应商（OAuth 账户）分组卡片；PiAuthSettings 自带分组标题 / 计数 / 刷新。 -->
  <div class="provider-groups">
    <div class="detail-panel">
      <PiAuthSettings onError={onError} refreshToken={authRefreshToken}
        onViewModels={(providerId, providerName) => void openOfficialModels(providerId, providerName)} />
    </div>

    <section class="provider-group" aria-label={t("自定义供应商")}>
      <div class="section-heading">
        <span class="group-title"><span>{t("自定义供应商")}</span><span class="group-count">{providers.length}</span></span>
      </div>
      {#if providers.length === 0}
        <p class="empty">{t("还没有自定义供应商，点击右上角「新建 Provider」添加")}</p>
      {:else}
        <div class="provider-rows">
          {#each providers as provider (provider.id)}
            <div class="provider-row" class:selected={provider.id === draft.id}>
              <button class="provider-open" type="button" onclick={() => openProviderEditor(provider)}>
                <span class="provider-copy">
                  <strong>{provider.name || provider.id}</strong>
                  <small>{provider.id} · {t("{count} 个模型", { count: configuredModelsOf(provider).length })}</small>
                </span>
              </button>
              <button type="button" class="quiet-button compact"
                title={t("编辑 {name}", { name: provider.name || provider.id })}
                onclick={() => openProviderEditor(provider)}>
                <Pencil size={13} />{t("编辑")}
              </button>
              <button class="danger-icon" type="button" aria-label={t("删除 Provider")} title={t("删除 Provider")}
                onclick={() => void removeProvider(provider)}><Trash2 size={14} /></button>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </div>

  {#if statusMessage}<p class="page-status" role="status">{statusMessage}</p>{/if}
</section>

{#if createOpen}
  <!-- 新建 Provider：先选类型；官方走登录流程，自定义保存后进入模型拉取弹窗。 -->
  <div class="editor-backdrop" role="presentation"
    onclick={(event) => event.target === event.currentTarget && closeCreate()}>
    <div class="editor-modal create-modal" role="dialog" aria-modal="true" aria-label={t("新建 Provider")}>
      <header class="editor-head">
        <h2>{createStep === "models" ? t("模型目录 · {name}", { name: officialProviderName }) : t("新建 Provider")}</h2>
        <div class="editor-head-actions">
          <button class="icon-button" type="button" aria-label={t("关闭")} title={t("关闭")}
            onclick={closeCreate}><X size={16} /></button>
        </div>
      </header>

      {#if createStep === "form"}
        <div class="editor-scroll">
          <section class="editor-section">
            <div class="form-grid">
              <label>{t("Provider 类型")}
                <select bind:value={createKind}>
                  <option value="official">{t("官方供应商")}</option>
                  <option value="custom">{t("自定义")}</option>
                </select>
              </label>
              {#if createKind === "official"}
                <label>{t("官方供应商")}
                  <select bind:value={officialProviderId}>
                    <option value="">{t("请选择官方供应商")}</option>
                    {#each officialProviders as provider (provider.id)}
                      <option value={provider.id}>{provider.name}{provider.isSubscription ? t(" · 订阅") : ""}{signedInIds.has(provider.id) ? t("（已登录）") : ""}</option>
                    {/each}
                  </select>
                </label>
              {/if}
            </div>
            {#if createKind === "official"}
              <div class="method-row">
                <span>{t("登录方式")}</span>
                {#if officialMethods.length > 1}
                  <select bind:value={officialLoginMethod}>
                    {#each officialMethods as method (method.id)}
                      <option value={method.id}>{t(method.label)}</option>
                    {/each}
                  </select>
                {:else}
                  <span class="method-name">{t(officialMethods[0]?.label ?? DEFAULT_OFFICIAL_METHODS[0].label)}</span>
                {/if}
              </div>
              <p class="create-hint">{t("创建后会打开登录流程，登录成功即出现在「官方供应商」分组。")}</p>
            {:else}
              <div class="form-grid">
                <label>{t("Provider 预设")}
                  <select bind:value={providerPreset} onchange={(event) => applyProviderPreset(event.currentTarget.value)}>
                    <option value="custom">{t("Custom 自定义")}</option>
                    {#each PROVIDER_PRESETS as preset (preset.key)}
                      <option value={preset.key}>{preset.label}</option>
                    {/each}
                  </select>
                </label>
                <label>{t("标识")}<input bind:value={draft.id} disabled={!isCustomProvider} placeholder="my-provider" autocomplete="off" /></label>
                <label>{t("名称")}<input bind:value={draft.name} disabled={!isCustomProvider} placeholder={t("自定义 Provider")} autocomplete="off" /></label>
                <label>{t("API 类型")}
                  <select bind:value={officialProviderId} onchange={(event) => selectOfficialProvider(event.currentTarget.value)}>
                    {#each API_OPTIONS as option (option.value)}
                      <option value={option.value}>{option.label}</option>
                    {/each}
                  </select>
                </label>
                <label>Base URL<input bind:value={draft.baseUrl} disabled={!isCustomProvider} placeholder="https://api.example.com/v1" autocomplete="url" /></label>
                <label>API Key<input type="password" bind:value={apiKey} placeholder={t("可稍后在编辑弹窗中填写")} autocomplete="new-password" /></label>
              </div>
              <p class="create-hint">{t("创建后进入模型拉取弹窗，选择要启用的模型。")}</p>
            {/if}
          </section>
        </div>
        <footer class="editor-foot">
          <span class="foot-status" role="status">{createMessage}</span>
          <div class="foot-actions">
            <button type="button" class="quiet-button" onclick={closeCreate}>{t("取消")}</button>
            <button type="button" class="primary-button" disabled={!createReady || createBusy}
              onclick={() => void submitCreate()}>{t("创建")}</button>
          </div>
        </footer>
      {:else if createStep === "login"}
        <div class="editor-scroll">
          <PiOfficialLoginFlow
            providerId={officialProviderId}
            providerName={officialProviderName}
            loginMethod={officialLoginMethod}
            invokeCommand={invokeCommand}
            onError={onError}
            onSuccess={(providerId) => void onOfficialLoggedIn(providerId)}
            onCancel={() => (createStep = "form")}
          />
        </div>
      {:else}
        <div class="editor-scroll">
          {#if officialModelsBusy}
            <p class="empty">{t("正在读取模型目录…")}</p>
          {:else if officialModelsError}
            <p class="status" role="alert">{officialModelsError}</p>
            <button type="button" class="quiet-button compact" onclick={() => void loadOfficialModels()}>{t("重试")}</button>
          {:else if officialModels.length === 0}
            <p class="empty">{t("该供应商暂未提供模型目录")}</p>
          {:else}
            <p class="create-hint">{t("模型目录由 pi 运行时提供，登录后无需 API Key 即可使用。")}</p>
            <div class="catalog-list">
              {#each officialModels as model (model.id)}
                <div class="catalog-row">
                  <span class="catalog-copy">
                    <strong>{model.name}</strong>
                    <small>{model.id} · {formatContext(model.contextWindow)}{model.reasoning ? ` · ${t("推理")}` : ""}{model.inputCost !== null ? ` · ${formatCost(model.inputCost)}/${formatCost(model.outputCost)}` : ""}</small>
                  </span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
        <footer class="editor-foot">
          <span class="foot-status" role="status">{t("{count} 个模型", { count: officialModels.length })}</span>
          <div class="foot-actions">
            <button type="button" class="primary-button" onclick={closeCreate}>{t("完成")}</button>
          </div>
        </footer>
      {/if}
    </div>
  </div>
{/if}

{#if editorOpen}
  <!-- 供应商编辑弹窗：左侧模型列表（拉取 / 搜索 / 选择），右侧模型详情与一键填入。 -->
  <div class="editor-backdrop" role="presentation"
    onclick={(event) => event.target === event.currentTarget && closeProviderEditor()}>
    <div class="editor-modal" role="dialog" aria-modal="true" aria-label={t("编辑 Provider")}>
      <header class="editor-head">
        <h2>{t("编辑 Provider")}</h2>
        <div class="editor-head-actions">
          <button type="button" class="quiet-button compact" class:active={showAdvanced} aria-pressed={showAdvanced}
            onclick={() => (showAdvanced = !showAdvanced)}>{t("高级设置")}</button>
          <button class="icon-button" type="button" aria-label={t("关闭")} title={t("关闭")}
            onclick={closeProviderEditor}><X size={16} /></button>
        </div>
      </header>

      <div class="editor-name-row">
        <label>{t("名称")}
          <input bind:value={draft.name} disabled={!isCustomProvider} placeholder={t("自定义 Provider")} autocomplete="off" />
        </label>
      </div>

      {#if showAdvanced}
        <div class="editor-scroll">
          <section class="editor-section">
            <div class="section-heading"><span>Provider</span></div>
            <div class="form-grid">
              <label>{t("Provider 预设")}
                <select bind:value={providerPreset} onchange={(event) => applyProviderPreset(event.currentTarget.value)}>
                  <option value="custom">{t("Custom 自定义")}</option>
                  {#each PROVIDER_PRESETS as preset (preset.key)}
                    <option value={preset.key}>{preset.label}</option>
                  {/each}
                </select>
              </label>
              <label>{t("标识")}<input bind:value={draft.id} disabled={!isCustomProvider || !isNewProvider} placeholder="my-provider" autocomplete="off" /></label>
              <label>{t("API 类型")}
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
              <span>{t("请求选项")}</span>
              <button type="button" class="quiet-button compact" onclick={addHeader}><Plus size={13} />{t("添加 Header")}</button>
            </div>
            <label>{t("代理")}<input bind:value={draft.proxy} placeholder="http://127.0.0.1:7890" autocomplete="url" /></label>
            {#if headerEntries.length === 0}
              <p class="empty">{t("没有自定义 Header。")}</p>
            {:else}
              <div class="header-list">
                {#each headerEntries as header, index (index)}
                  <div class="header-row">
                    <input bind:value={header.name} aria-label={t("Header {index} 名称", { index: index + 1 })} placeholder={t("Header 名称")} oninput={syncDraftHeaders} />
                    <input bind:value={header.value} aria-label={t("Header {index} 值", { index: index + 1 })} placeholder={t("Header 值")} oninput={syncDraftHeaders} />
                    <button class="danger-icon" type="button" aria-label={t("移除 Header {index}", { index: index + 1 })} title={t("移除 Header")} onclick={() => removeHeader(index)}><Trash2 size={14} /></button>
                  </div>
                {/each}
              </div>
            {/if}
          </section>

          <section class="editor-section credential-section">
            <div class="section-heading"><span>Credential Manager</span><span class:configured={credentialConfigured} class="credential-state">{authType === "oauth" ? "Pi OAuth" : nativeAuthConfigured && apiKeyConfigured ? t("Pi 登录 + API Key") : credentialConfigured ? "API Key" : t("未配置")}</span></div>
            <div class="credential-row">
              <KeyRound size={16} />
              <input type="password" bind:value={apiKey} placeholder={credentialConfigured ? t("输入新 Key 以替换") : "API Key"} autocomplete="new-password" />
              <button type="button" class="quiet-button" disabled={!draft.id || !apiKey.trim()} onclick={() => void saveCredential()}><Save size={14} />{t("保存 Key")}</button>
              <button type="button" class="danger-icon" aria-label={t("删除 API Key")} title={t("删除 API Key")} disabled={!credentialConfigured} onclick={() => void deleteCredential()}><Trash2 size={14} /></button>
              <button type="button" class="quiet-button" disabled={!draft.id || !draft.baseUrl} onclick={() => void testConnection()}><Check size={14} />{t("测试连接")}</button>
            </div>
            {#if connectionMessage}<p class="status" role="status">{connectionMessage}</p>{/if}
          </section>
        </div>
      {:else}
        <div class="editor-body">
          <aside class="editor-models" aria-label={t("模型列表")}>
            <div class="models-toolbar">
              <button type="button" class="quiet-button compact"
                disabled={isFetchingProviderModels || busyModel !== null || !draft.id || !draft.baseUrl}
                onclick={() => void refreshProviderModels()}>
                {#if isFetchingProviderModels}<span class="spin"><RefreshCw size={13} /></span>{:else}<RefreshCw size={13} />{/if}
                {t("获取列表")}
              </button>
              <button type="button" class="quiet-button compact"
                disabled={busyModel !== null}
                title={t("手动录入一个模型，不依赖供应商的 /models 接口")}
                onclick={() => (manualAddVisible = !manualAddVisible)}>
                {t("手动添加")}
              </button>
            </div>
            <div class="model-search-row">
              <Search size={13} />
              <input bind:value={modelSearch} placeholder={t("搜索模型…")} aria-label={t("搜索模型")} autocomplete="off" />
            </div>
            {#if manualAddVisible}
              <div class="manual-model-form">
                <input bind:value={manualModelId} placeholder={t("模型 ID（如 deepseek-chat）")} aria-label={t("手动添加模型的 ID")} autocomplete="off" />
                <input bind:value={manualModelName} placeholder={t("显示名称（可选）")} aria-label={t("手动添加模型的显示名称")} autocomplete="off" />
                <button type="button" class="quiet-button compact" disabled={!manualModelId.trim()} onclick={addManualModel}>
                  {t("添加")}
                </button>
              </div>
            {/if}
            {#if modalModelRows().length === 0}
              <p class="empty">{t("暂无模型；可先获取列表或手动添加")}</p>
            {:else}
              {#each modalModelRows() as row (row.id)}
                <div class="model-row" class:selected={detailMode === "model" && selectedModelId === row.id}>
                  <input type="checkbox" checked={row.configured} disabled={busyModel === row.id}
                    aria-label={t("选择 {name}", { name: row.name })} title={row.configured ? t("取消选择") : t("选择模型")}
                    onchange={() => void toggleModel(row)} />
                  <button type="button" class="model-open" onclick={() => openModelDetail(row)}>
                    <strong>{row.name}</strong>
                    <small>{row.sub}</small>
                  </button>
                </div>
              {/each}
            {/if}
          </aside>

          <div class="editor-detail">
            {#if detailMode === "model" && editingModel}
              <div class="section-heading">
                <span>{t("模型 · {id}", { id: editingModel.id })}</span>
                <span class="section-actions">
                  {#if autofillMessage && autofillModelId === editingModel.id}
                    <span class="saved-note" role="status">{autofillMessage}</span>
                  {/if}
                  <button type="button" class="quiet-button compact" disabled={busyModel === editingModel.id}
                    onclick={() => void autofillModel()}>
                    {#if busyModel === editingModel.id}<span class="spin"><RefreshCw size={13} /></span>{:else}<Zap size={13} />{/if}
                    {t("自动填入")}
                  </button>
                  <button type="button" class="quiet-button compact" disabled={!draft.id || busyModel === editingModel.id}
                    onclick={() => void testModelConnection()}>
                    {#if busyModel === editingModel.id}<span class="spin"><RefreshCw size={13} /></span>{:else}<Check size={13} />{/if}
                    {t("测试连通")}
                  </button>
                  <button class="danger-icon" type="button" aria-label={t("移除模型")} title={t("移除模型")}
                    onclick={() => { const id = editingModel.id; draft.models = draft.models.filter((candidate) => candidate.id !== id); selectedModelId = null; detailMode = "provider"; }}>
                    <Trash2 size={14} />
                  </button>
                </span>
              </div>
              <div class="form-grid">
                <label>{t("名称")}<input value={editingModel.name}
                  oninput={(event) => { editingModel.name = event.currentTarget.value; }} /></label>
                <label>{t("模型 ID")}<input value={editingModel.id} disabled /></label>
                <label>{t("API 类型（留空继承 Provider）")}
                  <select
                    value={editingModel.api ?? ""}
                    onchange={(event) => { editingModel.api = event.currentTarget.value === "" ? null : event.currentTarget.value; }}>
                    <option value="">{t("继承 Provider（{api}）", { api: draft.api })}</option>
                    {#each API_OPTIONS as option (option.value)}
                      <option value={option.value}>{option.label}</option>
                    {/each}
                  </select>
                </label>
                <label>{t("总上下文窗口（tokens）")}<input inputmode="numeric" value={editingModel.contextWindow}
                  oninput={(event) => setNumberField(editingModel, "contextWindow", event.currentTarget.value)} /></label>
                <label>{t("最大输出（tokens）")}<input inputmode="numeric" value={editingModel.maxTokens}
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
                {t("支持推理")}
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
              <div class="form-grid cost-grid">
                <label>{t("输入价格（$/M）")}<input inputmode="decimal" value={costValue(editingModel, "input")}
                  oninput={(event) => setCostField(editingModel, "input", event.currentTarget.value)} /></label>
                <label>{t("输出价格（$/M）")}<input inputmode="decimal" value={costValue(editingModel, "output")}
                  oninput={(event) => setCostField(editingModel, "output", event.currentTarget.value)} /></label>
                <label>{t("缓存读（$/M）")}<input inputmode="decimal" value={costValue(editingModel, "cacheRead")}
                  oninput={(event) => setCostField(editingModel, "cacheRead", event.currentTarget.value)} /></label>
                <label>{t("缓存写（$/M）")}<input inputmode="decimal" value={costValue(editingModel, "cacheWrite")}
                  oninput={(event) => setCostField(editingModel, "cacheWrite", event.currentTarget.value)} /></label>
              </div>
              <p class="cost-hint">{t("单位：美元 / 百万 tokens；来自 models.dev 的价格为参考值")}</p>
              {#if modelTest[editingModel.id]}
                <p class="status" class:ok={modelTest[editingModel.id].ok} role="status">{tm(modelTest[editingModel.id].text)}</p>
              {/if}
            {:else}
              <p class="empty">{t("从左侧选择一个模型，查看与编辑参数")}</p>
            {/if}
          </div>
        </div>
      {/if}

      <footer class="editor-foot">
        <span class="foot-status" role="status">{statusMessage}</span>
        <div class="foot-actions">
          <button type="button" class="quiet-button" onclick={closeProviderEditor}>{t("关闭")}</button>
          <button type="button" class="primary-button" disabled={!saveReady}
            title={saveReady ? "" : t("没有未保存的修改")}
            onclick={() => void saveProvider()}><Save size={14} />{t("保存")}</button>
        </div>
      </footer>
    </div>
  </div>
{/if}

<style>
  .provider-page.embedded { padding: 0; }
  .embedded .provider-header { justify-content: flex-end; }
  .provider-page { height: 100%; padding: 14px 18px 18px; display: grid; grid-template-rows: 42px minmax(0, 1fr) 24px; gap: 10px; color: var(--text); overflow: hidden; font-family: var(--text-font); }
  .provider-header, .provider-title, .header-actions, .section-heading, .provider-row, .credential-row, .models-toolbar, .model-row { display: flex; align-items: center; }
  .provider-header, .section-heading { justify-content: space-between; }
  .provider-title { gap: 8px; }
  h1, p { margin: 0; }
  h1 { font-size: 16px; font-weight: 650; }
  .header-actions, .section-actions { gap: 6px; }
  .provider-groups { min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 14px; }
  .detail-panel, .provider-group { border: 1px solid var(--border); border-radius: 6px; background: var(--surface); padding: 12px 14px; }
  .section-heading { min-height: 28px; color: var(--text-muted); font-size: 11px; font-weight: 700; letter-spacing: .04em; text-transform: uppercase; }
  .group-title { display: inline-flex; align-items: center; gap: 8px; }
  .group-count { min-width: 14px; padding: 0 7px; border-radius: 9px; background: var(--surface-raised); color: var(--accent); font-size: 10px; font-weight: 700; line-height: 17px; text-align: center; }
  .provider-rows { display: grid; gap: 6px; margin-top: 10px; }
  .provider-row { gap: 8px; padding: 6px 8px 6px 10px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-raised); color: var(--text); }
  .provider-row:hover, .provider-row.selected { border-color: var(--border-strong); background: var(--surface-hover); color: var(--text-strong); }
  .provider-open { flex: 1; min-width: 0; display: grid; gap: 2px; padding: 2px 4px 2px 0; border: 0; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .provider-copy { min-width: 0; display: grid; gap: 2px; }
  strong { overflow: hidden; color: var(--text-strong); font-size: 12px; font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  small { overflow: hidden; color: var(--text-muted); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .editor-backdrop { position: fixed; inset: 0; z-index: 15; display: grid; place-items: center; padding: 28px; background: rgb(3 6 4 / 62%); }
  .editor-modal { width: min(920px, 100%); max-height: calc(100vh - 56px); display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--border-strong); border-radius: 7px; background: var(--surface); color: var(--text); font-family: var(--text-font); box-shadow: 0 18px 48px rgb(0 0 0 / 38%); }
  /* 新建/模型目录弹窗是单列表单，不需要编辑弹窗的双栏宽度。 */
  .create-modal { width: min(600px, 100%); }
  .editor-head { flex-shrink: 0; display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  .editor-head h2 { margin: 0; color: var(--text-strong); font-size: 14px; font-weight: 650; }
  .editor-head-actions { display: flex; align-items: center; gap: 6px; }
  .editor-name-row { flex-shrink: 0; padding: 12px 16px 0; }
  .editor-scroll { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px; }
  .editor-body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(250px, 320px) minmax(0, 1fr); gap: 12px; padding: 12px 16px; overflow: hidden; }
  .editor-models { min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 6px; padding: 10px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-alt); }
  .editor-detail { min-height: 0; overflow: auto; padding: 12px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-alt); }
  .models-toolbar { justify-content: flex-start; gap: 6px; min-height: 26px; }
  .model-search-row { display: flex; align-items: center; gap: 6px; color: var(--text-muted); }
  .model-search-row input { flex: 1; min-width: 0; height: 28px; }
  .manual-model-form { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin: 4px 0; }
  .manual-model-form input { flex: 1 1 150px; min-width: 0; height: 28px; padding: 2px 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; font-size: 12px; }
  .model-row { gap: 6px; padding: 2px; border: 1px solid transparent; border-radius: 4px; }
  .model-row:hover, .model-row.selected { border-color: var(--border-strong); background: var(--surface-hover); }
  .model-row input[type="checkbox"] { width: 14px; height: 14px; flex-shrink: 0; accent-color: var(--accent); cursor: pointer; }
  .model-open { flex: 1; min-width: 0; display: grid; gap: 2px; padding: 4px; border: 0; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .editor-section + .editor-section { border-top: 1px solid var(--border); margin-top: 14px; padding-top: 12px; }
  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-top: 10px; }
  label { display: grid; gap: 5px; color: var(--text-muted); font-size: 11px; }
  input, select { min-width: 0; height: 30px; padding: 0 9px; border: 1px solid var(--border-strong); border-radius: 4px; outline: none; color: var(--text); background: var(--surface-alt); font: inherit; }
  input:focus, select:focus { border-color: var(--accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 20%, transparent); }
  input:disabled, select:disabled { color: var(--text-subtle); background: var(--surface-raised); }
  input[type="checkbox"] { width: 14px; height: 14px; accent-color: var(--accent); cursor: pointer; }
  .check-line { display: flex; align-items: center; gap: 6px; margin-top: 10px; color: var(--text); font-size: 12px; }
  .level-row { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 8px; }
  .check-inline { display: inline-flex; align-items: center; gap: 4px; color: var(--text); font-size: 11px; }
  button { font: inherit; }
  .icon-button, .danger-icon, .quiet-button, .primary-button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; border-radius: 4px; cursor: pointer; }
  .icon-button, .danger-icon { width: 30px; height: 30px; padding: 0; border: 1px solid transparent; color: var(--text-muted); background: transparent; }
  .icon-button:hover, .danger-icon:hover:not(:disabled) { border-color: var(--border-strong); color: var(--text-strong); background: var(--surface-hover); }
  .danger-icon { color: var(--status-failed); }
  .quiet-button, .primary-button { min-height: 30px; padding: 0 9px; border: 1px solid var(--border-strong); color: var(--text); background: var(--surface-raised); font-size: 11px; }
  .quiet-button:hover:not(:disabled) { border-color: var(--border-strong); color: var(--text-strong); }
  .quiet-button.active { border-color: var(--accent); background: var(--surface-hover); color: var(--text-strong); }
  .primary-button { border-color: var(--accent); color: var(--accent-ink); background: var(--accent); font-weight: 700; }
  .primary-button:hover:not(:disabled) { filter: brightness(1.08); }
  .quiet-button.compact { min-height: 26px; padding: 0 7px; }
  button:disabled { cursor: default; opacity: .45; }
  .request-section > label { margin-bottom: 8px; }
  .header-list { display: grid; gap: 6px; }
  .header-row { display: grid; grid-template-columns: minmax(100px, .8fr) minmax(120px, 1.2fr) 30px; gap: 6px; align-items: center; }
  .credential-row { gap: 7px; }
  .credential-row input { flex: 1; }
  .request-section + .editor-section { margin-top: 14px; }
  .empty { color: var(--text-muted); font-size: 11px; }
  .status { margin: 10px 0 0; color: var(--accent); font-size: 11px; overflow-wrap: anywhere; }
  .cost-hint { margin: 6px 0 0; color: var(--text-muted); font-size: 10px; }
  .saved-note { color: var(--accent); font-size: 11px; white-space: nowrap; }
  .status:not(.ok) { color: var(--status-failed); }
  .page-status { color: var(--accent); font-size: 12px; overflow-wrap: anywhere; }
  .editor-foot { flex-shrink: 0; display: flex; align-items: center; gap: 10px; padding: 10px 16px; border-top: 1px solid var(--border); }
  .foot-status { flex: 1; min-width: 0; overflow: hidden; color: var(--accent); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
  .foot-actions { display: flex; gap: 6px; flex-shrink: 0; }
  .spin { display: inline-flex; animation: provider-spin 1s linear infinite; }
  @keyframes provider-spin { to { transform: rotate(360deg); } }
  @media (max-width: 720px) {
    .editor-body { grid-template-columns: minmax(0, 1fr); overflow: auto; }
  }
  .method-row { display: flex; align-items: center; gap: 8px; margin-top: 10px; }
  .method-row > span { color: var(--text-muted); font-size: 11px; }
  .method-row select { min-width: 220px; }
  .method-name { color: var(--text); font-size: 11px; }
  .create-hint { margin: 10px 0 0; color: var(--text-muted); font-size: 11px; }
  .catalog-list { display: grid; gap: 6px; margin-top: 10px; }
  .catalog-row { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-raised); }
  .catalog-copy { flex: 1; min-width: 0; display: grid; gap: 2px; }
</style>
