#!/usr/bin/env node
/**
 * DeepPi ↔ Pi 官方登录桥。
 *
 * 以 JSON Lines（stdin/stdout，每行一个 UTF-8 JSON 对象）驱动 Pi Coding Agent
 * 的公开 SDK（`ModelRuntime.login` 等），完成 provider 的官方 OAuth 登录。
 * 凭据由 Pi 自己的 AuthStorage 写入 `PI_AUTH_AUTH_JSON`（即 DeepPi 托管 pi
 * 目录的 auth.json），DeepPi 不自行复刻任何 OAuth 流程、不引入第三方 client id。
 *
 * 请求：  {"v":1,"id":N,"op":"providers"|"status"|"login"|"respond"|"cancel"|"logout",...}
 * 应答：  {"v":1,"reply":N,"ok":true,...} / {"v":1,"reply":N,"ok":false,"error":"…"}
 * 事件：  {"v":1,"event":"notify|prompt|prompt_closed|done",...}
 *
 * 安全约定：所有输出（含错误信息）不携带令牌明文，错误先经 redact() 收敛。
 */
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const PROTOCOL = 1;
const SDK_PATH = process.env.PI_AUTH_PI_SDK;
const AUTH_PATH = process.env.PI_AUTH_AUTH_JSON || undefined;
const INJECT_PATH = process.env.PI_AUTH_BRIDGE_TEST_INJECT || undefined;

if (!SDK_PATH) {
  process.stderr.write("pi-auth-bridge: PI_AUTH_PI_SDK is required\n");
  process.exit(2);
}

function send(value) {
  process.stdout.write(`${JSON.stringify({ v: PROTOCOL, ...value })}\n`);
}

function reply(id, payload) {
  send({ reply: id, ...payload });
}

/** 错误信息脱敏：任何 ≥32 字符的令牌样连续串一律替换，避免令牌外泄。 */
function redact(text) {
  return String(text).replace(/[A-Za-z0-9_-]{32,}/g, "[redacted]");
}

function errorMessage(error) {
  const message = error instanceof Error ? error.message : String(error);
  const cause = error instanceof Error && error.cause
    ? `: ${error.cause instanceof Error ? error.cause.message : String(error.cause)}`
    : "";
  return redact(`${message}${cause}`);
}

function abortError() {
  const error = new Error("Login cancelled");
  error.name = "AbortError";
  return error;
}

/** 真实 Pi SDK：加载一次，供登录/存储/状态读取使用。 */
let realPiPromise = null;
function getRealPi() {
  if (!realPiPromise) {
    realPiPromise = (async () => {
      const pi = await import(pathToFileURL(SDK_PATH).href);
      if (!INJECT_PATH) {
        try {
          const dispatcher = await import(
            new URL("core/http-dispatcher.js", pathToFileURL(SDK_PATH).href).href
          );
          // 与 pi CLI 启动路径一致：让 OAuth 流程走 pi 自己的 HTTP dispatcher（代理、空闲超时）。
          dispatcher.configureHttpDispatcher?.();
        } catch {
          // dispatcher 不可用时保持默认 fetch；登录仍可进行。
        }
      }
      return pi;
    })();
    realPiPromise.catch(() => {
      realPiPromise = null;
    });
  }
  return realPiPromise;
}

/** auth.json 存储模块（pi 的公开入口未导出 AuthStorage，直接引用核心模块）。 */
let storagePromise = null;
function getStorage() {
  if (!storagePromise) {
    storagePromise = (async () => {
      await getRealPi();
      return import(new URL("core/auth-storage.js", pathToFileURL(SDK_PATH).href).href);
    })();
    storagePromise.catch(() => {
      storagePromise = null;
    });
  }
  return storagePromise;
}

let runtimePromise = null;
function getRuntime() {
  if (!runtimePromise) {
    runtimePromise = (async () => {
      if (INJECT_PATH) {
        // 测试注入点：替身运行时仍走真实桥接协议与真实 auth.json 存储。
        const injected = (await import(pathToFileURL(INJECT_PATH).href)).default;
        return injected.createRuntime(await getRealPi());
      }
      const pi = await getRealPi();
      return pi.ModelRuntime.create({ authPath: AUTH_PATH, refreshOnCreate: false });
    })();
    runtimePromise.catch(() => {
      runtimePromise = null;
    });
  }
  return runtimePromise;
}

function providerInfo(provider) {
  const oauth = provider?.auth?.oauth;
  return {
    id: String(provider?.id ?? ""),
    name: String(provider?.name ?? ""),
    oauth: !!oauth,
    oauthName: oauth?.name ?? null,
    isSubscription: !!oauth?.isSubscription,
  };
}

async function opProviders() {
  const runtime = await getRuntime();
  return runtime.getProviders().map(providerInfo);
}

/** 列出 auth.json 中的凭据（只给元数据与 OAuth 过期时间，不落令牌）。 */
async function opStatus() {
  const runtime = await getRuntime();
  const storage = await getStorage();
  const list = await runtime.listCredentials();
  return list.map((entry) => {
    let expires = null;
    const stored = storage.readStoredCredential?.(entry.providerId, AUTH_PATH);
    if (stored?.type === "oauth" && typeof stored.expires === "number") expires = stored.expires;
    return { provider: entry.providerId, authType: entry.type, expires };
  });
}

const activeLogins = new Map(); // loginId -> { controller, provider }
const pendingPrompts = new Map(); // promptId -> { login, resolve, reject }
let nextPromptId = 0;

/**
 * 关闭某次登录的所有待答提示。
 * `reject` 为 true 时同时解除等待（登录取消时使用；select 等 prompt 没有自带
 * signal，仅 abort interaction.signal 会让流程永远等不到输入）。
 */
function closePromptsOf(loginId, reject = false) {
  for (const [promptId, entry] of [...pendingPrompts]) {
    if (entry.login !== loginId) continue;
    pendingPrompts.delete(promptId);
    send({ event: "prompt_closed", login: loginId, promptId });
    if (reject) {
      try {
        entry.reject(abortError());
      } catch {
        // reject 回调自身不应中断收尾。
      }
    }
  }
}

function interactionFor(loginId, controller, preferredMethod = null) {
  return {
    signal: controller.signal,
    notify(event) {
      send({ event: "notify", login: loginId, ...event });
    },
    prompt(prompt) {
      return new Promise((resolve, reject) => {
        if (controller.signal.aborted) {
          reject(abortError());
          return;
        }
        // 界面已在创建流程里选好登录方式：命中即直接回答该选择提示
        //（未命中则照常转发给界面，行为与以前一致）。
        if (preferredMethod && prompt.type === "select") {
          const option = (prompt.options ?? []).find((entry) => entry?.id === preferredMethod);
          if (option) {
            resolve(option.id);
            return;
          }
        }
        const promptId = String(++nextPromptId);
        const entry = { login: loginId, resolve, reject, prompt };
        pendingPrompts.set(promptId, entry);
        if (prompt.signal) {
          // 流程自带取消（如回调先到、manual_code 输入被放弃）时解除等待并关闭面板。
          prompt.signal.addEventListener("abort", () => {
            if (pendingPrompts.delete(promptId)) {
              send({ event: "prompt_closed", login: loginId, promptId });
            }
            reject(abortError());
          }, { once: true });
        }
        send({
          event: "prompt",
          login: loginId,
          promptId,
          kind: prompt.type,
          message: prompt.message,
          placeholder: prompt.placeholder ?? null,
          options: prompt.options ?? null,
        });
      });
    },
  };
}

async function opLogin(id, request) {
  const provider = String(request.provider ?? "");
  if (!provider) throw new Error("@msg:pi.auth.bridge_provider_missing");
  if (activeLogins.size > 0) throw new Error("@msg:pi.auth.bridge_login_busy");
  // 界面可以提前选定登录方式（如 openai-codex 的 browser / device_code）：
  // 桥接在收到对应的选择提示时直接回答，不再二次询问。
  const preferredMethod =
    typeof request.method === "string" && request.method.trim() !== ""
      ? request.method.trim()
      : null;
  const loginId = `login-${id}`;
  const controller = new AbortController();
  activeLogins.set(loginId, { controller, provider });
  // 先应答 ack（Rust/界面据此记录活动登录），后续交互以事件下发。
  reply(id, { ok: true, login: loginId, provider });
  try {
    const runtime = await getRuntime();
    const credential = await runtime.login(
      provider,
      "oauth",
      interactionFor(loginId, controller, preferredMethod),
    );
    send({
      event: "done",
      login: loginId,
      ok: true,
      provider,
      credentialType: credential?.type ?? "oauth",
      expires: typeof credential?.expires === "number" ? credential.expires : null,
    });
  } catch (error) {
    send({ event: "done", login: loginId, ok: false, provider, error: errorMessage(error) });
  } finally {
    activeLogins.delete(loginId);
    closePromptsOf(loginId);
  }
}

/** 桥内当前活动登录（至多一个）：供 status 操作用于界面重连。 */
function activeLoginInfo() {
  for (const [loginId, entry] of activeLogins) {
    return { login: loginId, provider: entry.provider ?? null };
  }
  return null;
}

/** 供应商模型目录（pi 运行时自带；官方登录后无需 API Key 即可使用）。 */
async function opModels(request) {
  const provider = String(request.provider ?? "");
  if (!provider) throw new Error("provider is required");
  const runtime = await getRuntime();
  let models = [];
  try {
    models = runtime.getModels(provider) ?? [];
  } catch {
    models = [];
  }
  return models
    .map((model) => ({
      id: String(model?.id ?? ""),
      name: String(model?.name ?? model?.id ?? ""),
      contextWindow: typeof model?.contextWindow === "number" ? model.contextWindow : null,
      maxTokens: typeof model?.maxTokens === "number" ? model.maxTokens : null,
      reasoning: !!model?.reasoning,
      input: Array.isArray(model?.input) ? model.input.map(String) : [],
      inputCost: typeof model?.cost?.input === "number" ? model.cost.input : null,
      outputCost: typeof model?.cost?.output === "number" ? model.cost.output : null,
    }))
    .filter((model) => model.id !== "");
}

async function dispatch(id, request) {
  switch (request.op) {
    case "providers":
      reply(id, { ok: true, providers: await opProviders() });
      break;
    case "status": {
      const credentials = await opStatus();
      const active = activeLoginInfo();
      reply(id, {
        ok: true,
        credentials,
        activeLogin: active?.login ?? null,
        activeLoginProvider: active?.provider ?? null,
      });
      // 界面重连：应答之后重发该登录仍在等待的 prompt，让重挂载的面板先按
      // status 重建，再恢复输入项（重发顺序必须在应答之后）。
      if (active) {
        for (const [promptId, entry] of pendingPrompts) {
          if (entry.login !== active.login) continue;
          send({
            event: "prompt",
            login: entry.login,
            promptId,
            kind: entry.prompt.type,
            message: entry.prompt.message,
            placeholder: entry.prompt.placeholder ?? null,
            options: entry.prompt.options ?? null,
          });
        }
      }
      break;
    }
    case "login":
      await opLogin(id, request);
      break;
    case "models":
      reply(id, { ok: true, models: await opModels(request) });
      break;
    case "respond": {
      const promptId = String(request.promptId ?? "");
      const entry = pendingPrompts.get(promptId);
      if (!entry) {
        reply(id, { ok: false, error: "@msg:pi.auth.bridge_prompt_missing" });
        break;
      }
      pendingPrompts.delete(promptId);
      send({ event: "prompt_closed", login: entry.login, promptId });
      entry.resolve(String(request.value ?? ""));
      reply(id, { ok: true });
      break;
    }
    case "cancel": {
      const loginId = String(request.login ?? "");
      const controller = activeLogins.get(loginId)?.controller;
      if (!controller) {
        reply(id, { ok: false, error: "@msg:pi.auth.bridge_login_missing" });
        break;
      }
      // 桥侧先关闭并 reject 该登录的所有等待 prompt，再中断登录本身。
      closePromptsOf(loginId, true);
      controller.abort();
      reply(id, { ok: true });
      break;
    }
    case "logout": {
      const storage = await getStorage();
      // 幂等：条目不存在也视为成功；文件级删除，使用中的 pi 会话在下次启动时生效。
      await storage.AuthStorage.create(AUTH_PATH).delete(String(request.provider ?? ""));
      reply(id, { ok: true });
      break;
    }
    default:
      reply(id, { ok: false, error: `@msg:pi.auth.bridge_unknown_op?op=${encodeURIComponent(redact(String(request.op ?? "")))}` });
  }
}

const readline = createInterface({ input: process.stdin, crlfDelay: Infinity });
readline.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let request;
  try {
    request = JSON.parse(trimmed);
  } catch {
    send({ event: "protocol_error", error: "request is not valid JSON" });
    return;
  }
  const id = Number(request.id);
  if (!Number.isFinite(id) || id <= 0) {
    send({ event: "protocol_error", error: "request id is missing" });
    return;
  }
  dispatch(id, request).catch((error) => {
    reply(id, { ok: false, error: errorMessage(error) });
  });
});
readline.on("close", () => {
  for (const { controller } of activeLogins.values()) controller.abort();
  process.exit(0);
});
