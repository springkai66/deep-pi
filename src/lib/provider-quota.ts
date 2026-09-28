export interface QuotaTarget {
  providerId: string;
  label: string;
}

export type QuotaResponse =
  | { status: "available" | "exhausted"; amount: number; currency: string }
  | { status: "available" | "exhausted"; windows: { fiveHour?: { remainingPercent: number }; weekly?: { remainingPercent: number } } }
  | { status: "missing" | "unavailable" | "error" };

export type QuotaState = QuotaResponse | { status: "loading" };
export function quotaWindowRows(state: QuotaState | null): { label: "5h" | "weekly"; remainingPercent: number }[] {
  if (!state || !("windows" in state)) return [];
  const rows: { label: "5h" | "weekly"; remainingPercent: number }[] = [];
  for (const [label, window] of [["5h", state.windows.fiveHour], ["weekly", state.windows.weekly]] as const) {
    if (window && Number.isFinite(window.remainingPercent) && window.remainingPercent >= 0 && window.remainingPercent <= 100) {
      rows.push({ label, remainingPercent: window.remainingPercent });
    }
  }
  return rows;
}

/** Provider IDs are account identities. A similar model name on another provider is not the same quota. */
export function quotaTarget(modelKey: string): QuotaTarget | null {
  const separator = modelKey.indexOf("/");
  if (separator <= 0 || separator === modelKey.length - 1) return null;
  const providerId = modelKey.slice(0, separator);
  const labels: Record<string, string> = {
    anthropic: "Claude Code",
    "openai-codex": "Codex",
    google: "Gemini",
    "google-vertex": "Gemini (Vertex)",
    deepseek: "DeepSeek",
    "z-ai": "Z.ai",
    zai: "Z.ai",
    "kimi-coding": "Kimi Coding",
    moonshot: "Kimi API",
    moonshotai: "Kimi API",
    qwen: "Qwen",
    "opencode-go": "OpenCode Go",
  };
  const label = labels[providerId];
  return label ? { providerId, label } : null;
}

export function createQuotaTracker(
  query: (providerId: string) => Promise<QuotaResponse>,
  emit: (state: QuotaState | null) => void,
) {
  let generation = 0;
  return {
    load(target: QuotaTarget | null) {
      const current = ++generation;
      if (!target) { emit(null); return; }
      emit({ status: "loading" });
      void query(target.providerId).then((result) => {
        if (current === generation) emit(result);
      }).catch(() => {
        // Never display upstream error text: responses may contain credentials or account data.
        if (current === generation) emit({ status: "error" });
      });
    },
    stop() { generation++; emit(null); },
  };
}
