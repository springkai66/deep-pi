// Observe transport failures before Pi reduces nested errors to "terminated".
// Do not patch fetch, dispatchers, timeouts, payloads, or retry policy.
import { subscribe, unsubscribe } from "node:diagnostics_channel";

const CAUSES = new Set([
  "UND_ERR_BODY_TIMEOUT", "UND_ERR_HEADERS_TIMEOUT", "UND_ERR_CONNECT_TIMEOUT",
  "UND_ERR_SOCKET", "ECONNRESET", "ECONNREFUSED", "ETIMEDOUT", "EPIPE",
  "ENOTFOUND", "EAI_AGAIN", "CERT_HAS_EXPIRED", "DEPTH_ZERO_SELF_SIGNED_CERT",
  "UNABLE_TO_VERIFY_LEAF_SIGNATURE", "ERR_TLS_CERT_ALTNAME_INVALID",
]);
const codexPath = (path) => typeof path === "string" && /\/codex\/responses\/?(?:\?|$)/.test(path);
const milliseconds = (value) => Math.max(0, Math.min(86_400_000, Math.floor(value)));

export function installTransportObserver(emit, clock = () => performance.now()) {
  const requests = new WeakMap();
  const sockets = new WeakMap();
  const listeners = [];
  const on = (name, callback) => {
    // A diagnostic subscriber must never throw into the request it observes.
    const guarded = (event) => { try { callback(event); } catch { /* best effort */ } };
    subscribe(name, guarded);
    listeners.push([name, guarded]);
  };
  const report = (state, cause, closeCode) => emit({
    type: "deeppi_transport_diagnostic",
    transport: {
      protocol: state.protocol, phase: state.phase, cause,
      durationMs: milliseconds(clock() - state.started),
      idleMs: state.lastData === undefined ? null : milliseconds(clock() - state.lastData),
      closeCode: closeCode ?? null,
    },
  });
  on("undici:request:create", ({ request }) => {
    if (!codexPath(request?.path)) return;
    requests.set(request, {
      protocol: request.upgrade === "websocket" ? "websocket" : "sse",
      phase: "headers", started: clock(),
    });
  });
  on("undici:request:headers", ({ request, response }) => {
    const state = requests.get(request);
    if (!state) return;
    if (response.statusCode === 101) { requests.delete(request); return; }
    state.phase = "body";
    state.lastData = clock();
  });
  on("undici:request:bodyChunkReceived", ({ request }) => {
    const state = requests.get(request);
    if (state) state.lastData = clock();
  });
  on("undici:request:trailers", ({ request }) => requests.delete(request));
  on("undici:request:error", ({ request, error }) => {
    const state = requests.get(request);
    if (!state) return;
    requests.delete(request);
    if (error?.name === "AbortError" || error?.code === "UND_ERR_ABORTED") return;
    let cause = "UNKNOWN";
    for (let item = error, depth = 0; item && depth < 6; item = item.cause, depth++) {
      if (CAUSES.has(item.code)) { cause = item.code; break; }
    }
    report(state, cause);
  });
  on("undici:websocket:open", ({ websocket }) => {
    if (!codexPath(new URL(websocket.url).pathname)) return;
    sockets.set(websocket, { protocol: "websocket", phase: "stream", started: clock() });
  });
  on("undici:websocket:close", ({ websocket, code }) => {
    const state = sockets.get(websocket);
    if (!state) return;
    sockets.delete(websocket);
    if (!Number.isInteger(code) || code < 1002 || code > 4999) return;
    // No close reason, URL, headers, stack, address, or response body is retained.
    report(state, "WS_ABNORMAL_CLOSE", code);
  });
  return () => { for (const [name, callback] of listeners) unsubscribe(name, callback); };
}

if (process.env.DEEPPI_TRANSPORT_DIAGNOSTICS === "1") {
  // Pi's RPC output guard later redirects process.stdout.write to stderr.
  // Capture the original writer before startup, for these bounded protocol frames only.
  const write = process.stdout.write.bind(process.stdout);
  installTransportObserver((event) => write(`${JSON.stringify(event)}\n`));
}
