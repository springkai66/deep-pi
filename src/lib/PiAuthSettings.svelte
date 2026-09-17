<script lang="ts">
  import { onMount } from "svelte";
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    Check,
    Copy,
    ExternalLink,
    LogIn,
    LogOut,
    RefreshCw,
    X,
  } from "@lucide/svelte";
  import { notifyModelsChanged } from "./model-config-sync";
  import { t } from "$lib/i18n.svelte";
  import {
    AUTH_EVENT_NAME,
    formatValidityText,
    normalizePastedCode,
    validityParts,
    type PiAuthCredentialSummary,
    type PiAuthDoneEvent,
    type PiAuthEvent,
    type PiAuthLoginAck,
    type PiAuthProviderInfo,
    type PiAuthSelectOption,
    type PiAuthStatusResponse,
  } from "./pi-auth";

  interface Props {
    invokeCommand?: typeof nativeInvoke;
    onError: (error: unknown) => void;
  }

  interface PromptState {
    promptId: string;
    kind: "text" | "secret" | "select" | "manual_code";
    message: string;
    placeholder: string | null;
    options: PiAuthSelectOption[] | null;
  }

  interface LoginPanel {
    loginId: string;
    providerId: string;
    providerName: string;
    url: string | null;
    instructions: string | null;
    progress: string;
    deviceCode: { userCode: string; verificationUri: string } | null;
    prompt: PromptState | null;
  }

  let { invokeCommand = nativeInvoke, onError }: Props = $props();
  const invoke = <T,>(command: string, args?: Parameters<typeof nativeInvoke>[1]) =>
    invokeCommand<T>(command, args);

  let providers = $state<PiAuthProviderInfo[]>([]);
  let credentials = $state<Record<string, PiAuthCredentialSummary>>({});
  let loading = $state(true);
  let busy = $state(false);
  let notice = $state("");
  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    void listen<PiAuthEvent>(AUTH_EVENT_NAME, (event) => {
      handleEvent(event.payload);
    }).then((stop) => {
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    });
    void refresh();
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  let panel = $state<LoginPanel | null>(null);
  let manualValue = $state("");
  /// 用户主动取消时置位：done(ok=false) 若来自取消，则按“已取消”提示而不是报错。
  let cancelRequested = $state(false);
  function isSignedIn(providerId: string): boolean {
    return credentials[providerId]?.authType === "oauth";
  }

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

  function handleEvent(event: PiAuthEvent) {
    if (!panel || event.login !== panel.loginId) return;
    switch (event.event) {
      case "notify": {
        if (event.type === "auth_url" && event.url) {
          panel.url = event.url;
          panel.instructions = event.instructions ?? null;
        } else if (event.type === "device_code" && event.userCode && event.verificationUri) {
          panel.deviceCode = { userCode: event.userCode, verificationUri: event.verificationUri };
        } else if ((event.type === "progress" || event.type === "info") && event.message) {
          panel.progress = event.message;
        }
        break;
      }
      case "prompt": {
        panel.prompt = {
          promptId: event.promptId,
          kind: event.kind,
          message: event.message,
          placeholder: event.placeholder,
          options: event.options,
        };
        manualValue = "";
        break;
      }
      case "prompt_closed": {
        if (panel.prompt?.promptId === event.promptId) panel.prompt = null;
        break;
      }
      case "done": {
        finishLogin(event);
        break;
      }
    }
  }

  function finishLogin(event: PiAuthDoneEvent) {
    const cancelled = cancelRequested;
    cancelRequested = false;
    panel = null;
    void refresh();
    if (event.ok) {
      notice = t("{provider} 官方登录成功，pi 任务已可直接使用", { provider: event.provider });
      notifyModelsChanged();
    } else if (cancelled) {
      notice = t("已取消 {provider} 的官方登录", { provider: event.provider });
    } else {
      onError(event.error ?? t("官方登录失败"));
    }
  }

  async function startLogin(providerId: string) {
    if (panel || busy) return;
    const providerName = providers.find((provider) => provider.id === providerId)?.name ?? providerId;
    busy = true;
    notice = "";
    try {
      const ack = await invoke<PiAuthLoginAck>("pi_auth_start_login", {
        request: { providerId },
      });
      panel = {
        loginId: ack.loginId,
        providerId,
        providerName,
        url: null,
        instructions: null,
        progress: "",
        deviceCode: null,
        prompt: null,
      };
      manualValue = "";
      cancelRequested = false;
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }

  async function respond(value: string) {
    const prompt = panel?.prompt;
    if (!panel || !prompt || busy) return;
    busy = true;
    try {
      await invoke("pi_auth_respond", {
        request: { loginId: panel.loginId, promptId: prompt.promptId, value },
      });
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
    }
  }

  async function cancel() {
    if (!panel || busy) return;
    cancelRequested = true;
    busy = true;
    try {
      await invoke("pi_auth_cancel", { request: { loginId: panel.loginId } });
    } catch (error) {
      cancelRequested = false;
      onError(error);
    } finally {
      busy = false;
    }
  }

  async function logout(providerId: string) {
    if (busy || panel) return;
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

  async function openLink(url: string) {
    if (!url) return;
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch (error) {
      onError(error);
    }
  }

  async function copyText(value: string) {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      notice = t("已复制到剪贴板");
    } catch (error) {
      onError(error);
    }
  }
</script>

<section class="auth-section" aria-label={t("官方账号登录")}>
  <div class="auth-heading">
    <span>{t("官方账号登录")}</span>
    <button class="auth-button quiet" type="button" disabled={loading || busy} onclick={() => void refresh()}>
      {#if loading}<span class="auth-spin"><RefreshCw size={13} /></span>{:else}<RefreshCw size={13} />{/if}
      {t("刷新")}
    </button>
  </div>
  <p class="auth-hint">
    {t("用官方账号订阅（Claude Pro/Max、ChatGPT Plus/Pro 等）登录，登录后 pi 任务无需 API Key 即可使用对应模型。")}
  </p>
  {#if providers.length === 0 && !loading}
    <p class="auth-empty">{t("暂无可官方登录的通道（等待 pi 运行时就绪）")}</p>
  {:else}
    <div class="auth-list">
      {#each providers as provider (provider.id)}
        <div class="auth-row">
          <div class="auth-name">
            <strong>{provider.name}</strong>
            <small>{provider.oauthName ?? provider.id}{provider.isSubscription ? t(" · 订阅") : ""}</small>
          </div>
          <span class="auth-state" class:signed={isSignedIn(provider.id)}>{stateLabel(provider.id)}</span>
          {#if isSignedIn(provider.id)}
            <button class="auth-button" type="button" disabled={busy} onclick={() => void logout(provider.id)}>
              <LogOut size={13} />{t("退出登录")}
            </button>
          {:else}
            <button class="auth-button primary" type="button" disabled={busy || !!panel} onclick={() => void startLogin(provider.id)}>
              <LogIn size={13} />{t("官方登录")}
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if panel}
    <div class="auth-panel" role="dialog" aria-label={t("正在登录 {provider}", { provider: panel.providerName })}>
      <div class="auth-panel-head">
        <span>{t("正在登录 {provider}", { provider: panel.providerName })}</span>
        <button class="auth-icon" type="button" aria-label={t("取消登录")} title={t("取消登录")} onclick={() => void cancel()}>
          <X size={14} />
        </button>
      </div>
      {#if panel.progress}<p class="auth-progress" role="status">{panel.progress}</p>{/if}
      {#if panel.deviceCode}
        <div class="auth-code-row">
          <code class="auth-code">{panel.deviceCode.userCode}</code>
          <button class="auth-button" type="button" onclick={() => void openLink(panel?.deviceCode?.verificationUri ?? "")}>
            <ExternalLink size={13} />{t("打开验证页")}
          </button>
          <button class="auth-button" type="button" onclick={() => void copyText(panel?.deviceCode?.userCode ?? "")}>
            <Copy size={13} />{t("复制设备码")}
          </button>
        </div>
      {/if}
      {#if panel.url}
        <p class="auth-url-line">
          {panel.instructions ?? t("已打开浏览器，请在其中完成授权；如未跳转，可点击下方链接重试。")}
        </p>
        <div class="auth-code-row">
          <button class="auth-button" type="button" onclick={() => void openLink(panel?.url ?? "")}>
            <ExternalLink size={13} />{t("打开授权页")}
          </button>
          <button class="auth-button" type="button" onclick={() => void copyText(panel?.url ?? "")}>
            <Copy size={13} />{t("复制链接")}
          </button>
        </div>
      {/if}
      {#if panel.prompt}
        <div class="auth-prompt">
          <p class="auth-prompt-message">{panel.prompt.message}</p>
          {#if panel.prompt.kind === "select"}
            <div class="auth-options">
              {#each panel.prompt.options ?? [] as option (option.id)}
                <button class="auth-button" type="button" disabled={busy} onclick={() => void respond(option.id)}>
                  {option.label}
                </button>
              {/each}
            </div>
          {:else}
            <div class="auth-input-row">
              <input
                bind:value={manualValue}
                type={panel.prompt.kind === "secret" ? "password" : "text"}
                placeholder={panel.prompt.placeholder ?? t("粘贴回调 URL 或授权码")}
                autocomplete="off"
                aria-label={panel.prompt.message}
                onkeydown={(event) => {
                  if (event.key === "Enter" && manualValue.trim()) void respond(normalizePastedCode(manualValue));
                }}
              />
              <button class="auth-button primary" type="button" disabled={busy || !manualValue.trim()}
                onclick={() => void respond(normalizePastedCode(manualValue))}>
                <Check size={13} />{t("提交")}
              </button>
            </div>
          {/if}
        </div>
      {:else if panel.url || panel.deviceCode}
        <p class="auth-wait" role="status">{t("等待浏览器端完成授权…")}<span class="auth-spin"><RefreshCw size={12} /></span></p>
      {/if}
    </div>
  {/if}

  {#if notice}
    <p class="auth-notice" role="status">{notice}</p>
  {/if}
</section>

<style>
  .auth-section { margin-top: 14px; padding-top: 12px; border-top: 1px solid #303832; display: grid; gap: 8px; }
  .auth-heading { display: flex; align-items: center; justify-content: space-between; min-height: 28px; color: #aeb8b0; font-size: 11px; font-weight: 700; letter-spacing: .04em; text-transform: uppercase; }
  .auth-hint { margin: 0; color: #778279; font-size: 11px; }
  .auth-empty { margin: 0; color: #778279; font-size: 11px; }
  .auth-list { display: grid; gap: 4px; }
  .auth-row { display: flex; align-items: center; gap: 10px; padding: 5px 6px; border: 1px solid transparent; border-radius: 4px; }
  .auth-row:hover { border-color: var(--border-strong); background: var(--surface-hover); }
  .auth-name { flex: 1; min-width: 0; display: grid; gap: 2px; text-align: left; }
  .auth-name strong { overflow: hidden; color: #f4f7f5; font-size: 12px; font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .auth-name small { overflow: hidden; color: #7f8b82; font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .auth-state { flex-shrink: 0; color: #778279; font-size: 10px; }
  .auth-state.signed { color: #8fd6ad; }
  .auth-button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 26px; padding: 0 8px; border: 1px solid #39433c; border-radius: 4px; color: #c5cec7; background: #202721; font: inherit; font-size: 11px; cursor: pointer; flex-shrink: 0; }
  .auth-button:hover:not(:disabled) { border-color: #607467; color: #f4f7f5; }
  .auth-button.primary { border-color: #75b894; color: #102017; background: #8fd6ad; font-weight: 700; }
  .auth-button.primary:hover:not(:disabled) { background: #a6e6bd; }
  .auth-button.quiet { min-height: 24px; padding: 0 6px; }
  .auth-button:disabled { cursor: default; opacity: .45; }
  .auth-icon { display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 26px; border: 1px solid transparent; border-radius: 4px; color: #aeb7b0; background: transparent; cursor: pointer; }
  .auth-icon:hover { border-color: #465048; color: #f4f7f5; background: #252b27; }
  .auth-panel { display: grid; gap: 8px; padding: 10px; border: 1px solid #39433c; border-radius: 5px; background: #101411; }
  .auth-panel-head { display: flex; align-items: center; justify-content: space-between; color: #c5cec7; font-size: 12px; font-weight: 650; }
  .auth-progress { margin: 0; color: #7f8b82; font-size: 11px; overflow-wrap: anywhere; }
  .auth-code-row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .auth-code { padding: 3px 9px; border: 1px dashed #39433c; border-radius: 4px; color: #8fd6ad; font-family: var(--mono-font, monospace); font-size: 12px; letter-spacing: .06em; }
  .auth-url-line { margin: 0; color: #9aa59c; font-size: 11px; }
  .auth-prompt { display: grid; gap: 6px; }
  .auth-prompt-message { margin: 0; color: #c5cec7; font-size: 11px; }
  .auth-options { display: flex; flex-wrap: wrap; gap: 6px; }
  .auth-input-row { display: flex; gap: 6px; }
  .auth-input-row input { flex: 1; min-width: 0; height: 28px; padding: 0 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; font-size: 12px; }
  .auth-input-row input:focus { border-color: #78bd96; box-shadow: 0 0 0 2px #78bd9622; outline: none; }
  .auth-wait { margin: 0; display: flex; align-items: center; gap: 6px; color: #7f8b82; font-size: 11px; }
  .auth-notice { margin: 0; color: #8fd6ad; font-size: 11px; overflow-wrap: anywhere; }
  .auth-spin { display: inline-flex; animation: auth-spin 1s linear infinite; }
  @keyframes auth-spin { to { transform: rotate(360deg); } }
</style>
