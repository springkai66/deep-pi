export interface OperationState {
  id: string;
  cancelling: boolean;
}

type Invoke = (command: string, args: Record<string, unknown>) => Promise<unknown>;

export function createOperationRunner(
  invoke: Invoke,
  onChange: (state: OperationState | null) => void,
) {
  let active: OperationState | null = null;
  return {
    async run(command: string, request: Record<string, unknown>) {
      if (active) throw new Error("operation already in progress");
      const operation = { id: crypto.randomUUID(), cancelling: false };
      active = operation;
      onChange({ ...operation });
      try {
        return await invoke(command, { request: { ...request, operationId: operation.id } });
      } finally {
        active = null;
        onChange(null);
      }
    },
    async cancel() {
      const operation = active;
      if (!operation || operation.cancelling) return false;
      const accepted = await invoke("cancel_operation", { operationId: operation.id });
      if (active === operation && accepted === true) {
        operation.cancelling = true;
        onChange({ ...operation });
      }
      return accepted === true;
    },
  };
}
