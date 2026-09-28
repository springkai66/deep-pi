import type { AppSettings } from "./settings";

export function createSettingsSaver(
  save: (settings: AppSettings) => Promise<unknown>,
  onError: (error: unknown) => void,
  onBusy: (busy: boolean) => void,
) {
  let latest: AppSettings | null = null;
  let pending: AppSettings | null = null;
  let running: Promise<void> | null = null;
  let failure: unknown = null;

  async function drain() {
    onBusy(true);
    while (pending) {
      const snapshot = pending;
      pending = null;
      try { await save(snapshot); failure = null; }
      catch (error) {
        failure = error;
        if (!pending) onError(error);
      }
    }
    onBusy(false);
  }

  function enqueue(settings: AppSettings) {
    latest = {
      ...settings,
      skippedUpdates: { ...settings.skippedUpdates },
      snoozedUpdates: { ...settings.snoozedUpdates },
      externalEditor: settings.externalEditor ? { ...settings.externalEditor } : null,
    };
    pending = latest;
    failure = null;
    start();
  }

  function start() {
    if (!running) running = drain().finally(() => {
      running = null;
      if (pending) start();
    });
  }

  return {
    enqueue,
    async flush() {
      while (running) await running;
      if (failure && latest) {
        enqueue(latest);
        while (running) await running;
      }
      if (failure) throw failure;
    },
  };
}
