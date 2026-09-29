export type RecoveryResult = "resubscribed" | "restarted";

interface RecoveryActions {
  stillCurrent: () => boolean;
  probe: (runId: string) => Promise<boolean>;
  resubscribe: () => void;
  restart: () => Promise<string | null | undefined>;
}

/** Never send a prompt as part of connection recovery. */
export async function recoverRpcSession(
  runId: string | null | undefined,
  actions: RecoveryActions,
): Promise<RecoveryResult> {
  if (!actions.stillCurrent()) throw new Error("RPC run changed during recovery");
  if (runId && await actions.probe(runId)) {
    if (!actions.stillCurrent()) throw new Error("RPC run changed during recovery");
    actions.resubscribe();
    return "resubscribed";
  }
  if (!actions.stillCurrent()) throw new Error("RPC run changed during recovery");
  const restartedRunId = await actions.restart();
  if (!restartedRunId || restartedRunId === runId) {
    throw new Error("RPC session could not be restarted; check task status before trying again");
  }
  return "restarted";
}

/** A timed-out mutating RPC may have been accepted even though its response was lost. */
export function isUnknownPromptOutcome(cause: unknown): boolean {
  return String(cause).includes("RPC_OUTCOME_UNKNOWN:");
}
