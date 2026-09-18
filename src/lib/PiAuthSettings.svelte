<script lang="ts">
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { Boxes, LogOut, RefreshCw } from "@lucide/svelte";
  import { notifyModelsChanged } from "./model-config-sync";
  import { t } from "$lib/i18n.svelte";
  import {
    formatValidityText,
    validityParts,
    type PiAuthCredentialSummary,
    type PiAuthProviderInfo,
    type PiAuthStatusResponse,
  } from "./pi-auth";

  interface Props {
    invokeCommand?: typeof nativeInvoke;
    onError: (error: unknown) => void;
    /** 父组件在登录/退出后递增该值，用于刷新列表。 */
    refreshToken?: number;
    /** 查看该官方供应商的模型目录（只读）。 */
    onViewModels?: (providerId: string, providerName: string) => void;
  }

  let { invokeCommand = nativeInvoke, onError, refreshToken = 0, onViewModels }: Props = $props();
  const invoke = <T,>(command: string, args?: Parameters<typeof nativeInvoke>[1]) =>
    invokeCommand<T>(command, args);

  let providers = $state<PiAuthProviderInfo[]>([]);
  let credentials = $state<Record<string, PiAuthCredentialSummary>>({});
  let loading = $state(true);
  let busy = $state(false);
  let notice = $state("");

  // 首次挂载与 refreshToken 变化都会刷新；不依赖 providers/credentials，
  // 避免刷新结果触发自身重跑。
  $effect(() => {
    void refreshToken;
    void refresh();
  });

  /// 只列出已登录的官方供应商：默认空，创建（=完成登录）后才出现在这里。
  const signedInProviders = $derived(
    providers.filter((provider) => credentials[provider.id]?.authType === "oauth"),
  );

  function stateLabel(providerId: string): string {
    const credential = credentials[providerId];
    if (credential?.authType === "oauth") {
      return formatValidityText(validityParts(credential.expires), t);
    }
    return t("未登录");
  }

  async function refresh() {
    loading = true;
    try {
      const [providerList, status] = await Promise.all([
        invoke<PiAuthProviderInfo[]>("pi_auth_providers"),
        invoke<PiAuthStatusResponse>("pi_auth_status"),
      ]);
      providers = providerList.filter((provider) => provider.oauth);
      credentials = Object.fromEntries(
        status.credentials.map((credential) => [credential.provider, credential]),
      );
    } catch (error) {
      onError(error);
    } finally {
      loading = false;
    }
  }

  async function logout(providerId: string) {
    if (busy) return;
    busy = true;
    notice = "";
    try {
      await invoke("pi_auth_logout", { request: { providerId } });
      notice = t("{provider} 已退出官方登录", { provider: providerId });
      await refresh();
      notifyModelsChanged();
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }
</script>

<section class="auth-section" aria-label={t("官方供应商")}>
  <div class="auth-heading">
    <span class="auth-title-group">
      <span>{t("官方供应商")}</span>
      <span class="auth-count">{signedInProviders.length}</span>
    </span>
    <button class="auth-button quiet" type="button" disabled={loading || busy} onclick={() => void refresh()}>
      {#if loading}<span class="auth-spin"><RefreshCw size={13} /></span>{:else}<RefreshCw size={13} />{/if}
      {t("刷新")}
    </button>
  </div>
  <p class="auth-hint">
    {t("用官方账号订阅（Claude Pro/Max、ChatGPT Plus/Pro 等）登录，登录后 pi 任务无需 API Key 即可使用对应模型。")}
  </p>
  {#if signedInProviders.length === 0}
    <p class="auth-empty">{t("还没有官方供应商，点击右上角「新建 Provider」添加")}</p>
  {:else}
    <div class="auth-list">
      {#each signedInProviders as provider (provider.id)}
        <div class="auth-row">
          <div class="auth-name">
            <strong>{provider.name}</strong>
            <small>{provider.oauthName ?? provider.id}{provider.isSubscription ? t(" · 订阅") : ""}</small>
          </div>
          <span class="auth-state signed">{stateLabel(provider.id)}</span>
          {#if onViewModels}
            <button class="auth-button" type="button" disabled={busy}
              onclick={() => onViewModels?.(provider.id, provider.name)}>
              <Boxes size={13} />{t("模型")}
            </button>
          {/if}
          <button class="auth-button" type="button" disabled={busy} onclick={() => void logout(provider.id)}>
            <LogOut size={13} />{t("退出登录")}
          </button>
        </div>
      {/each}
    </div>
  {/if}

  {#if notice}
    <p class="auth-notice" role="status">{notice}</p>
  {/if}
</section>

<style>
  .auth-section { display: grid; gap: 8px; }
  .auth-heading { display: flex; align-items: center; justify-content: space-between; min-height: 28px; color: var(--text-muted); font-size: 11px; font-weight: 700; letter-spacing: .04em; text-transform: uppercase; }
  .auth-title-group { display: inline-flex; align-items: center; gap: 8px; }
  .auth-count { min-width: 14px; padding: 0 7px; border-radius: 9px; background: var(--surface-raised); color: var(--accent); font-size: 10px; font-weight: 700; line-height: 17px; text-align: center; }
  .auth-hint { margin: 0; color: var(--text-muted); font-size: 11px; }
  .auth-empty { margin: 0; color: var(--text-muted); font-size: 11px; }
  .auth-list { display: grid; gap: 4px; }
  .auth-row { display: flex; align-items: center; gap: 10px; padding: 5px 6px; border: 1px solid transparent; border-radius: 4px; }
  .auth-row:hover { border-color: var(--border-strong); background: var(--surface-hover); }
  .auth-name { flex: 1; min-width: 0; display: grid; gap: 2px; text-align: left; }
  .auth-name strong { overflow: hidden; color: var(--text-strong); font-size: 12px; font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .auth-name small { overflow: hidden; color: var(--text-muted); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .auth-state { flex-shrink: 0; color: var(--text-muted); font-size: 10px; }
  .auth-state.signed { color: var(--accent); }
  .auth-button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 26px; padding: 0 8px; border: 1px solid var(--border-strong); border-radius: 4px; color: var(--text); background: var(--surface-raised); font: inherit; font-size: 11px; cursor: pointer; flex-shrink: 0; }
  .auth-button:hover:not(:disabled) { background: var(--surface-hover); }
  .auth-button.quiet { min-height: 24px; padding: 0 6px; }
  .auth-button:disabled { cursor: default; opacity: .45; }
  .auth-notice { margin: 0; color: var(--accent); font-size: 11px; overflow-wrap: anywhere; }
  .auth-spin { display: inline-flex; animation: auth-spin 1s linear infinite; }
  @keyframes auth-spin { to { transform: rotate(360deg); } }
</style>
