export type DialogValue = boolean | string | null;

export interface DialogChoice {
  value: string;
  label: string;
}

export type DialogRequest = {
  id: number;
  kind: "confirm" | "input" | "alert" | "choice";
  title: string;
  message: string;
  choices?: DialogChoice[];
  initialValue?: string;
  placeholder?: string;
  confirmLabel?: string;
  scope?: string;
  timeoutMs?: number;
  multiline?: boolean;
  maxLength?: number;
  resolve: (value: DialogValue) => void;
};

export function createDialogQueue(publish: (request: DialogRequest | null) => void) {
  const pending: DialogRequest[] = [];
  let nextId = 0;
  let disposed = false;
  const timers = new Map<number, ReturnType<typeof setTimeout>>();
  function removeWhere(predicate: (request: DialogRequest) => boolean) {
    const previous = pending[0];
    for (let index = pending.length - 1; index >= 0; index--) {
      if (!predicate(pending[index])) continue;
      const [request] = pending.splice(index, 1);
      clearTimeout(timers.get(request.id));
      timers.delete(request.id);
      request.resolve(null);
    }
    if (previous !== pending[0]) publish(pending[0] ?? null);
  }
  return {
    request(options: Omit<DialogRequest, "id" | "resolve">): Promise<DialogValue> {
      if (disposed) return Promise.resolve(null);
      return new Promise((resolve) => {
        const request = { ...options, id: ++nextId, resolve };
        pending.push(request);
        if (options.timeoutMs !== undefined && Number.isFinite(options.timeoutMs)) {
          timers.set(request.id, setTimeout(() => removeWhere((item) => item.id === request.id), Math.max(0, options.timeoutMs)));
        }
        if (pending.length === 1) publish(pending[0]);
      });
    },
    resolve(value: DialogValue) {
      const request = pending.shift();
      if (request) {
        clearTimeout(timers.get(request.id));
        timers.delete(request.id);
        request.resolve(value);
      }
      publish(pending[0] ?? null);
    },
    cancelScope(scope: string) { removeWhere((request) => request.scope === scope); },
    dispose() {
      disposed = true;
      removeWhere(() => true);
    },
  };
}
