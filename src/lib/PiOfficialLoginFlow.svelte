<script lang="ts">
  import { onMount } from "svelte";
  import { invoke as nativeInvoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Check, Copy, ExternalLink, LogIn, RefreshCw, RotateCcw, X } from "@lucide/svelte";
  import { notifyModelsChanged } from "./model-config-sync";
  import { t, tm } from "$lib/i18n.svelte";
  import {
    AUTH_EVENT_NAME,
    normalizePastedCode,
    type PiAuthDoneEvent,
    type PiAuthEvent,
    type PiAuthLoginAck,
    type PiAuthSelectOption,
    type PiAuthStatusResponse,
  } from "./pi-auth";
  import { isAppMessage } from "./app-messages";

  interface Props {
    providerId: string;
    providerName: string;
    /// 创建流程里预先选定的登录方式（如 openai-codex 的 browser / device_code）；
    /// null 时由 pi 的默认流程决定，选择提示照常展示。
    loginMethod?: string | null;
    invokeCommand?: typeof nativeInvoke;
    onError: (error: unknown) => void;
    onSuccess: (providerId: string) => void;
    onCancel: () => void;
  }

  interface PromptState {
    promptId: string;
    kind: "text" | "secret" | "select" | "manual_code";
    message: string;
    placeholder: string | null;
    options: PiAuthSelectOption[] | null;
  }

  let {
    providerId,
    providerName,
    loginMethod = null,
    invokeCommand = nativeInvoke,
    onError,
    onSuccess,
    onCancel,
  }: Props = $props();
  const invoke = <T,>(command: string, args?: Parameters<typeof nativeInvoke>[1]) =>
    invokeCommand<T>(command, args);

  let loginId = $state<string | null>(null);
  let url = $state<string | null>(null);
  let instructions = $state<string | null>(null);
  let progress = $state("");
  let deviceCode = $state<{ userCode: string; verificationUri: string } | null>(null);
  let prompt = $state<PromptState | null>(null);
  let manualValue = $state("");
  let busy = $state(false);
  let starting = $state(true);
  let errorText = $state("");
  /// 用户主动取消时置位：done(ok=false) 若来自取消，则按“已取消”处理而不是报错。
  let cancelRequested = $state(false);
  /// 最近事件缓冲：接上已有登录时回放，恢复丢失的 notify / prompt 状态。
  let eventBuffer: PiAuthEvent[] = [];

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
    void begin();
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  async function begin() {
    starting = true;
    errorText = "";
    try {
      const status = await invoke<PiAuthStatusResponse>("pi_auth_status");
      if (status.activeLogin && status.activeLoginProvider === providerId) {
        // 例如设置页曾被关闭：后端登录还在跑，直接接上，避免会话被锁死。
        loginId = status.activeLogin;
        progress = t("检测到进行中的官方登录，已重新接上");
        replay();
        return;
      }
      if (status.activeLogin) {
        // 其它供应商留下的登录会话：先取消，保证本次创建能干净开始。
        await invoke("pi_auth_cancel", { request: { loginId: status.activeLogin } }).catch(() => {});
      }
      const ack = await invoke<PiAuthLoginAck>("pi_auth_start_login", {
        request: { providerId, loginMethod },
      });
      loginId = ack.loginId;
      replay();
    } catch (error) {
      errorText = tm(String(error));
      onError(error);
    } finally {
      starting = false;
    }
  }

  function replay() {
    for (const event of eventBuffer) {
      if (event.login === loginId) applyEvent(event);
    }
  }

  function handleEvent(event: PiAuthEvent) {
    eventBuffer.push(event);
    if (eventBuffer.length > 50) eventBuffer.splice(0, eventBuffer.length - 50);
    if (!loginId || event.login !== loginId) return;
    applyEvent(event);
  }

  function applyEvent(event: PiAuthEvent) {
    switch (event.event) {
      case "notify": {
        if (event.type === "auth_url" && event.url) {
          url = event.url;
          instructions = event.instructions ?? null;
        } else if (event.type === "device_code" && event.userCode && event.verificationUri) {
          deviceCode = { userCode: event.userCode, verificationUri: event.verificationUri };
        } else if ((event.type === "progress" || event.type === "info") && event.message) {
          progress = event.message;
        }
        break;
      }
      case "prompt": {
        prompt = {
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
        if (prompt?.promptId === event.promptId) prompt = null;
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
    if (event.ok) {
      notifyModelsChanged();
      onSuccess(event.provider);
      return;
    }
    if (cancelled) {
      onCancel();
      return;
    }
    const message = failureText(event.error ?? "");
    errorText = message;
    onError(message);
  }

  /// done(ok=false) 的文本：后端消息码交给 tm() 按语言渲染；第三方原文
  ///（pi SDK / 服务端返回）保留原样，只补一句本地化的前缀。
  function failureText(detail: string): string {
    const trimmed = detail.trim();
    if (trimmed === "") return t("官方登录失败");
    return isAppMessage(trimmed) ? trimmed : t("官方登录失败：{detail}", { detail: trimmed });
  }

  async function cancel() {
    if (busy) return;
    if (!loginId) {
      onCancel();
      return;
    }
    const target = loginId;
    cancelRequested = true;
    busy = true;
    try {
      await invoke("pi_auth_cancel", { request: { loginId: target } });
    } catch (error) {
      onError(error);
    } finally {
      busy = false;
      cancelRequested = false;
      // 命令应答即视为取消完成（后端会回收会话）；不再等待 done 事件。
      if (loginId === target) loginId = null;
      onCancel();
    }
  }

  async function respond(value: string) {
    const current = prompt;
    if (!loginId || !current || busy) return;
    busy = true;
    try {
      await invoke("pi_auth_respond", {
        request: { loginId, promptId: current.promptId, value },
      });
    } catch (error) {
      // 面板已被取消/替换时不再打扰用户。
      if (prompt?.promptId === current.promptId) onError(error);
    } finally {
      busy = false;
    }
  }

  async function openLink(target: string) {
    if (!target) return;
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(target);
    } catch (error) {
      onError(error);
    }
  }

  async function copyText(value: string) {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
    } catch (error) {
      onError(error);
    }
  }
</script>

<section class="login-flow" aria-label={t("正在登录 {provider}", { provider: providerName })}>
  <div class="login-head">
    <span class="login-title"><LogIn size={14} />{t("正在登录 {provider}", { provider: providerName })}</span>
    <button class="icon-button" type="button" aria-label={t("取消登录")} title={t("取消登录")}
      onclick={() => void cancel()}><X size={14} /></button>
  </div>

  {#if progress}<p class="login-progress" role="status">{progress}</p>{/if}

  {#if deviceCode}
    <div class="login-code-row">
      <code class="login-code">{deviceCode.userCode}</code>
      <button class="login-button" type="button" onclick={() => void openLink(deviceCode?.verificationUri ?? "")}>
        <ExternalLink size={13} />{t("打开验证页")}
      </button>
      <button class="login-button" type="button" onclick={() => void copyText(deviceCode?.userCode ?? "")}>
        <Copy size={13} />{t("复制设备码")}
      </button>
    </div>
  {/if}

  {#if url}
    <p class="login-url-line">
      {instructions ?? t("已打开浏览器，请在其中完成授权；如未跳转，可点击下方链接重试。")}
    </p>
    <div class="login-code-row">
      <button class="login-button" type="button" onclick={() => void openLink(url ?? "")}>
        <ExternalLink size={13} />{t("打开授权页")}
      </button>
      <button class="login-button" type="button" onclick={() => void copyText(url ?? "")}>
        <Copy size={13} />{t("复制链接")}
      </button>
    </div>
  {/if}

  {#if prompt}
    <div class="login-prompt">
      <p class="login-prompt-message">{prompt.message}</p>
      {#if prompt.kind === "select"}
        <div class="login-options">
          {#each prompt.options ?? [] as option (option.id)}
            <button class="login-button" type="button" disabled={busy} onclick={() => void respond(option.id)}>
              {option.label}
            </button>
          {/each}
        </div>
      {:else}
        <div class="login-input-row">
          <input
            bind:value={manualValue}
            type={prompt.kind === "secret" ? "password" : "text"}
            placeholder={prompt.placeholder ?? t("粘贴回调 URL 或授权码")}
            autocomplete="off"
            aria-label={prompt.message}
            onkeydown={(event) => {
              if (event.key === "Enter" && manualValue.trim()) void respond(normalizePastedCode(manualValue));
            }}
          />
          <button class="login-button primary" type="button" disabled={busy || !manualValue.trim()}
            onclick={() => void respond(normalizePastedCode(manualValue))}>
            <Check size={13} />{t("提交")}
          </button>
        </div>
      {/if}
    </div>
  {:else if !starting && !errorText}
    <p class="login-wait" role="status">{t("等待浏览器端完成授权…")}<span class="login-spin"><RefreshCw size={12} /></span></p>
  {/if}

  {#if errorText}
    <div class="login-error-row">
      <p class="login-error" role="alert">{tm(errorText)}</p>
      <button class="login-button" type="button" disabled={busy} onclick={() => void begin()}>
        <RotateCcw size={13} />{t("重试")}
      </button>
    </div>
  {/if}
</section>

<style>
  .login-flow { display: grid; gap: 8px; padding: 10px; border: 1px solid var(--border-strong); border-radius: 5px; background: var(--surface-alt); }
  .login-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .login-title { display: inline-flex; align-items: center; gap: 6px; color: var(--text); font-size: 12px; font-weight: 650; }
  .login-progress { margin: 0; color: var(--text-muted); font-size: 11px; overflow-wrap: anywhere; }
  .login-code-row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .login-code { padding: 3px 9px; border: 1px dashed var(--border-strong); border-radius: 4px; color: var(--accent); font-family: var(--code-font, monospace); font-size: 12px; letter-spacing: .06em; }
  .login-url-line { margin: 0; color: var(--text-muted); font-size: 11px; }
  .login-prompt { display: grid; gap: 6px; }
  .login-prompt-message { margin: 0; color: var(--text); font-size: 11px; }
  .login-options { display: flex; flex-wrap: wrap; gap: 6px; }
  .login-input-row { display: flex; gap: 6px; }
  .login-input-row input { flex: 1; min-width: 0; height: 28px; padding: 0 8px; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); color: var(--text); font: inherit; font-size: 12px; }
  .login-input-row input:focus { border-color: var(--accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 20%, transparent); outline: none; }
  .login-wait { margin: 0; display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: 11px; }
  .login-error-row { display: flex; align-items: center; gap: 8px; }
  .login-error { flex: 1; min-width: 0; margin: 0; color: var(--status-failed); font-size: 11px; overflow-wrap: anywhere; }
  .login-button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 26px; padding: 0 8px; border: 1px solid var(--border-strong); border-radius: 4px; color: var(--text); background: var(--surface-raised); font: inherit; font-size: 11px; cursor: pointer; flex-shrink: 0; }
  .login-button:hover:not(:disabled) { background: var(--surface-hover); }
  .login-button.primary { border-color: var(--accent); color: var(--accent-ink); background: var(--accent); font-weight: 700; }
  .login-button.primary:hover:not(:disabled) { filter: brightness(1.08); }
  .login-button:disabled { cursor: default; opacity: .45; }
  .icon-button { display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 26px; padding: 0; border: 1px solid transparent; border-radius: 4px; color: var(--text-muted); background: transparent; cursor: pointer; }
  .icon-button:hover { border-color: var(--border-strong); color: var(--text-strong); background: var(--surface-hover); }
  .login-spin { display: inline-flex; animation: login-spin 1s linear infinite; }
  @keyframes login-spin { to { transform: rotate(360deg); } }
</style>
