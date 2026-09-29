<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { notifyModelsChanged } from "./model-config-sync";
  import type { PiOfficialModel, PiModelDefaults, PiModelSelection } from "./pi-auth";
  import { t, tm } from "$lib/i18n.svelte";
  import ModelSelectionList from "./ModelSelectionList.svelte";
  import { createModelSelectionSaver } from "./model-selection";
  import type { PiModelSummary, PiModelProfile } from "./provider";
  import { Check, Zap } from "@lucide/svelte";

  let { models, invokeCommand = nativeInvoke, onModelsChanged, onBusyChange, onError }: {
    models: PiOfficialModel[];
    invokeCommand?: typeof nativeInvoke;
    onModelsChanged: (models: PiOfficialModel[]) => void;
    onBusyChange: (busy: boolean) => void;
    onError: (error: unknown) => void;
  } = $props();
  const providerId = "openai-codex";
  let defaults = $state<PiModelDefaults>({ defaultProvider: null, defaultModel: null, modelThinkingLevels: {} });
  let selectedId = $state("");
  let modelTest = $state<{ ok: boolean; text: string } | null>(null);
  let probing = $state(false);
  let autofilling = $state(false);
  let modelIds = $state<string[] | null>(null);
  let selectionReady = $state(false);
  let selectionError = $state("");
  let selectionSaving = $state(false);
  let detailsTouched = false;
  let thinkingTouched = false;
  let thinkingLevel = $state("");
  let contextWindow = $state<number | undefined>();
  let maxTokens = $state<number | undefined>();
  let loading = $state(true);
  let saving = $state(false);
  let defaultsReady = $state(false);
  let error = $state("");
  let status = $state("");
  const selected = $derived(models.find((model) => model.id === selectedId));
  const busy = $derived(loading || saving);
  const limitsValid = $derived(Number.isSafeInteger(contextWindow) && Number.isSafeInteger(maxTokens)
    && (contextWindow ?? 0) > 0 && (maxTokens ?? 0) > 0 && (maxTokens ?? 0) <= (contextWindow ?? 0));
  const limitsChanged = $derived(contextWindow !== selected?.contextWindow || maxTokens !== selected?.maxTokens);
  const defaultChanged = $derived(defaults.defaultModel !== selectedId || (defaults.modelThinkingLevels[selectedId] ?? "") !== thinkingLevel);
  const rows = $derived(models.map((model) => ({
    id: model.id, name: model.name, sub: model.id,
    checked: selectionReady && (modelIds === null || modelIds.includes(model.id)),
    badge: defaults.defaultModel === model.id ? t("默认模型") : undefined,
  })));

  $effect(() => {
    if (!models.some((model) => model.id === selectedId)) selectedId = models[0]?.id ?? "";
  });
  $effect(() => {
    const model = selected;
    contextWindow = model?.contextWindow ?? undefined;
    maxTokens = model?.maxTokens ?? undefined;
  });
  $effect(() => {
    const id = selectedId;
    thinkingLevel = untrack(() => defaults.modelThinkingLevels[id] ?? "");
    status = "";
  });

  const selectionSaver = createModelSelectionSaver<string[] | null, PiModelSelection>({
    save: (ids) => invokeCommand<PiModelSelection>("pi_auth_save_model_selection", { request: { modelIds: ids } }),
    saved: (_ids, _result, latest) => { if (latest) { selectionError = ""; notifyModelsChanged("selection"); } },
    failed: (cause) => { selectionError = tm(String(cause)); onError(cause); },
    busy: (value) => { selectionSaving = value; onBusyChange(value || saving); },
  });

  function selectModels(ids: string[] | null) {
    if (!selectionReady) return;
    modelIds = ids;
    selectionError = "";
    selectionSaver.enqueue(ids === null ? null : models.filter((model) => ids.includes(model.id)).map((model) => model.id));
  }

  async function testConnection() {
    if (!selected || probing) return;
    const modelId = selected.id;
    probing = true;
    modelTest = { ok: false, text: t("正在测试…") };
    try {
      const result = await invokeCommand<{ ok: boolean; status: number | null; latencyMs: number | null; error: string | null }>(
        "pi_auth_test_model_connection", { request: { providerId, modelId } });
      if (selectedId !== modelId) return;
      modelTest = result.ok
        ? { ok: true, text: result.status === null
          ? t("连通 · {latency}ms", { latency: result.latencyMs ?? 0 })
          : t("连通 · HTTP {status} · {latency}ms", { status: result.status, latency: result.latencyMs ?? 0 }) }
        : { ok: false, text: result.error ?? t("连接失败") };
    } catch (cause) { if (selectedId === modelId) modelTest = { ok: false, text: tm(String(cause)) }; }
    finally { probing = false; }
  }

  async function autofill() {
    if (!selected || autofilling) return;
    const modelId = selected.id;
    autofilling = true;
    status = "";
    try {
      const candidates = await invokeCommand<PiModelSummary[]>("search_pi_models", {
        request: { query: modelId, provider: null },
      });
      const match = candidates.find((model) => (model.provider === "openai" || model.provider === providerId)
        && (model.id === modelId || model.id === `openai/${modelId}` || model.id === `${providerId}/${modelId}`));
      if (!match) { status = t("目录暂无详情"); return; }
      const profile = await invokeCommand<PiModelProfile>("pi_model_profile", {
        request: { path: match.path, provider: match.provider, modelId: match.id },
      });
      if (selectedId !== modelId) return;
      contextWindow = profile.contextWindow ?? selected.contextWindow ?? undefined;
      maxTokens = profile.maxTokens ?? selected.maxTokens ?? undefined;
      status = t("已自动填入");
    } catch (cause) { if (selectedId === modelId) { error = tm(String(cause)); onError(cause); } }
    finally { autofilling = false; }
  }
  async function loadDefaults() {
    loading = true; error = "";
    try {
      const [defaultsResult, selectionResult] = await Promise.allSettled([
        defaultsReady ? Promise.resolve(defaults) : invokeCommand<PiModelDefaults>("pi_auth_model_settings", { request: { providerId } }),
        selectionReady ? Promise.resolve({ modelIds }) : invokeCommand<PiModelSelection>("pi_auth_model_selection"),
      ]);
      if (defaultsResult.status === "fulfilled") {
        if (!defaultsReady) {
          defaults = defaultsResult.value;
          defaultsReady = true;
          if (!detailsTouched && models.some((model) => model.id === defaults.defaultModel)) selectedId = defaults.defaultModel!;
          if (!thinkingTouched) thinkingLevel = defaults.modelThinkingLevels[selectedId] ?? "";
        }
      } else {
        error = tm(String(defaultsResult.reason)); onError(defaultsResult.reason);
      }
      if (selectionResult.status === "fulfilled") {
        if (!selectionReady) {
          modelIds = selectionResult.value.modelIds;
          selectionReady = true;
        }
      } else {
        error = tm(String(selectionResult.reason)); onError(selectionResult.reason);
      }
    } finally { loading = false; }
  }
  onMount(() => { void loadDefaults(); });

  function toggleModel(modelId: string) {
    if (!selectionReady) return;
    const selectedIds = modelIds === null ? models.map((model) => model.id) : modelIds;
    selectModels(selectedIds.includes(modelId) ? selectedIds.filter((id) => id !== modelId) : [...selectedIds, modelId]);
  }

  async function save(action: "defaults" | "limits" | "reset") {
    if (busy || !selected || (action === "limits" && !limitsValid)) return;
    saving = true; error = ""; status = ""; onBusyChange(true);
    const modelId = selected.id;
    try {
      if (action === "defaults") {
        defaults = await invokeCommand<PiModelDefaults>("pi_auth_set_model_defaults", {
          request: { providerId, modelId, thinkingLevel: thinkingLevel || null },
        });
        status = t("默认模型与思考强度已保存");
      } else {
        const updated = await invokeCommand<PiOfficialModel[]>("pi_auth_save_model_limits", {
          request: { providerId, modelId, contextWindow: action === "reset" ? null : contextWindow, maxTokens: action === "reset" ? null : maxTokens },
        });
        onModelsChanged(updated);
        status = action === "reset" ? t("已恢复目录参数") : t("模型参数已保存");
      }
      notifyModelsChanged();
    } catch (cause) { error = tm(String(cause)); onError(cause); }
    finally { saving = false; onBusyChange(selectionSaving); }
  }
</script>

<section class="subscription-models" aria-labelledby="subscription-model-heading" aria-busy={busy}>
  <ModelSelectionList {rows} selectedId={selectedId} ready={selectionReady} saving={selectionSaving} error={selectionError}
    onPick={(id) => { detailsTouched = true; selectedId = id; modelTest = null; }}
    onToggle={toggleModel} onSelectAll={() => selectModels(null)} onClear={() => selectModels([])}
    onRetry={() => selectionSaver.retry()} />
  <div class="editor-detail" id="subscription-model-detail">
    <div class="section-heading"><h3 id="subscription-model-heading">{selected ? t("模型 · {id}", { id: selected.id }) : t("订阅模型设置")}</h3>
      {#if selected}<div class="actions">
        <button type="button" disabled={busy || autofilling} onclick={() => void autofill()}><Zap size={13} />{t("自动填入")}</button>
        <button type="button" disabled={probing} title={t("会向模型发送最小测试请求，可能消耗订阅额度")} onclick={() => void testConnection()}><Check size={13} />{t("测试连通")}</button>
      </div>{/if}
    </div>
    {#if selected}
      <div class="form-grid">
        <label>{t("名称")}<input value={selected.name} readonly /></label>
        <label>{t("模型 ID")}<input value={selected.id} readonly /></label>
        <label>{t("上下文窗口")}<input type="number" aria-label={t("上下文窗口")} min="1" max={Number.MAX_SAFE_INTEGER} step="1" bind:value={contextWindow} oninput={() => { detailsTouched = true; }} disabled={busy} /></label>
        <label>{t("最大输出 Token")}<input type="number" aria-label={t("最大输出 Token")} min="1" max={Number.MAX_SAFE_INTEGER} step="1" bind:value={maxTokens} oninput={() => { detailsTouched = true; }} disabled={busy} /></label>
      </div>
      {#if limitsChanged && !limitsValid}<p class="error" role="alert">{t("参数必须为正整数，最大输出不能超过上下文窗口。")}</p>{/if}
      <p class="hint">{t("这些参数不会增加订阅权限或服务端限制；模型参数修改在新建或重新启动会话后生效。")}</p>
      {#if modelTest}<p class:status={modelTest.ok} class:error={!modelTest.ok} role="status">{modelTest.text}</p>{/if}
      <div class="actions">
        <button type="button" class="primary-button" disabled={busy || !limitsValid || !limitsChanged} onclick={() => void save("limits")}>{t("保存模型参数")}</button>
        <button type="button" disabled={busy} onclick={() => void save("reset")}>{t("恢复目录参数")}</button>
      </div>
      <section class="defaults-section" aria-label={t("默认模型与思考强度")}>
        <h4>{t("默认模型与思考强度")}</h4>
        <p class="hint">{t("此订阅的默认模型：{model}", { model: defaults.defaultModel ?? t("未设置") })}</p>
        <label>{t("默认思考强度")}
          <select aria-label={t("默认思考强度")} bind:value={thinkingLevel} onchange={() => { detailsTouched = true; thinkingTouched = true; }} disabled={busy}>
            <option value="">{t("由 Pi 决定")}</option>
            {#each selected.thinkingLevels ?? [] as level}<option value={level}>{level}</option>{/each}
          </select>
        </label>
        <p class="hint">{t("选择默认模型和思考强度，用于新会话；已有会话请在聊天中切换模型，项目配置可能覆盖默认值。")}</p>
        <div class="actions"><button type="button" disabled={busy || !defaultsReady || !defaultChanged} onclick={() => void save("defaults")}>{t("保存默认模型与思考强度")}</button></div>
      </section>
    {/if}
    {#if loading}<p class="hint" role="status">{t("正在读取模型设置…")}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p><button type="button" disabled={busy} onclick={() => void loadDefaults()}>{t("重新读取设置")}</button>{/if}
    {#if status}<p class="status" role="status">{status}</p>{/if}
  </div>
</section>

<style>
  .subscription-models { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(250px, 320px) minmax(0, 1fr); gap: 12px; overflow: hidden; }
  .editor-detail { min-width: 0; min-height: 0; overflow: auto; padding: 12px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-alt); }
  .section-heading { margin-bottom: 10px; display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
  h3, h4 { margin: 0; color: var(--text-strong); font-size: 12px; font-weight: 650; overflow-wrap: anywhere; }
  .form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
  label { display: grid; gap: 5px; min-width: 0; color: var(--text-muted); font-size: 11px; }
  input, select { box-sizing: border-box; width: 100%; min-width: 0; height: 30px; padding: 0 9px; border: 1px solid var(--border-strong); border-radius: 4px; outline: none; color: var(--text); background: var(--surface-alt); font: inherit; }
  input:focus, select:focus, button:focus-visible { border-color: var(--accent); outline: none; box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 20%, transparent); }
  input[readonly], input:disabled, select:disabled { color: var(--text-subtle); background: var(--surface-raised); }
  .defaults-section { display: grid; gap: 10px; border-top: 1px solid var(--border); margin-top: 14px; padding-top: 12px; }
  .hint, .status, .error { margin: 10px 0; font-size: 11px; line-height: 1.6; overflow-wrap: anywhere; }
  .defaults-section .hint { margin: 0; }
  .hint { color: var(--text-muted); } .status { color: var(--accent); } .error { color: var(--status-failed); }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; }
  button { min-height: 30px; padding: 0 9px; border: 1px solid var(--border-strong); border-radius: 4px; color: var(--text); background: var(--surface-raised); font: inherit; font-size: 11px; cursor: pointer; }
  .primary-button { border-color: var(--accent); color: var(--accent-ink); background: var(--accent); font-weight: 700; }
  button:disabled { cursor: default; opacity: .45; }
  @media (max-width: 720px) { .subscription-models { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(270px, 42vh) minmax(0, 1fr); } }
</style>
