// Apply DeepPi's visible Codex setting without rewriting the user's Pi settings.
const TRANSPORTS = new Set(["sse", "auto", "websocket", "websocket-cached"]);

export function codexTransportOptions(options, transport = "sse") {
  if (!TRANSPORTS.has(transport)) throw new Error("Unsupported DeepPi Codex transport");
  return { ...options, transport };
}

export default async function codexTransport(pi) {
  const moduleUrl = process.env.DEEPPI_CODEX_API_MODULE;
  if (!moduleUrl) throw new Error("DeepPi Codex transport module is unavailable");
  const transport = process.env.DEEPPI_CODEX_TRANSPORT ?? "sse";
  codexTransportOptions(undefined, transport); // Validate before registering the provider.
  const { streamSimple } = await import(moduleUrl);
  if (typeof streamSimple !== "function") throw new Error("Unsupported Pi Codex streaming API");
  pi.registerProvider("openai-codex", {
    api: "openai-codex-responses",
    streamSimple: (model, context, options) => streamSimple(model, context, codexTransportOptions(options, transport)),
  });
}
