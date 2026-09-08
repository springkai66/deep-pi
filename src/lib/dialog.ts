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
  resolve: (value: DialogValue) => void;
};

export function createDialogQueue(publish: (request: DialogRequest | null) => void) {
  const pending: DialogRequest[] = [];
  let nextId = 0;
  let disposed = false;
  return {
    request(options: Omit<DialogRequest, "id" | "resolve">): Promise<DialogValue> {
      if (disposed) return Promise.resolve(null);
      return new Promise((resolve) => {
        pending.push({ ...options, id: ++nextId, resolve });
        if (pending.length === 1) publish(pending[0]);
      });
    },
    resolve(value: DialogValue) {
      pending.shift()?.resolve(value);
      publish(pending[0] ?? null);
    },
    dispose() {
      disposed = true;
      for (const request of pending.splice(0)) request.resolve(null);
      publish(null);
    },
  };
}
