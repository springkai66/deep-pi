export interface ModelSelectionRow {
  id: string;
  name: string;
  sub: string;
  checked: boolean;
  badge?: string;
  busy?: boolean;
}

/** Serialize immutable snapshots; keep only the newest queued intent. */
export function createModelSelectionSaver<T, R>(ports: {
  save: (value: T) => Promise<R>;
  saved: (value: T, result: R, latest: boolean) => void;
  failed: (error: unknown) => void;
  busy: (value: boolean) => void;
}) {
  let revision = 0;
  let latest: { revision: number; value: T } | undefined;
  let pending: typeof latest;
  let running: Promise<void> | undefined;
  let failed = false;
  let failure: unknown;

  async function drain() {
    while (pending) {
      const snapshot = pending;
      pending = undefined;
      try {
        const result = await ports.save(snapshot.value);
        failed = false;
        ports.saved(snapshot.value, result, latest?.revision === snapshot.revision);
      } catch (error) {
        if (latest?.revision === snapshot.revision) {
          failed = true;
          failure = error;
          ports.failed(error);
        }
      }
    }
  }

  function start() {
    if (running) return;
    running = Promise.resolve().then(drain).finally(() => {
      running = undefined;
      if (pending) start();
      else ports.busy(false);
    });
    ports.busy(true);
  }

  function enqueue(value: T) {
    // Callers pass plain snapshots, never live Svelte proxies or component objects.
    latest = { revision: ++revision, value: structuredClone(value) };
    pending = latest;
    failed = false;
    start();
  }

  return {
    enqueue,
    retry() { if (latest) enqueue(latest.value); },
    async flush() {
      while (running) await running;
      if (failed) throw failure;
    },
  };
}
