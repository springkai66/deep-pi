import { t } from "./i18n.svelte";
interface InputPorts {
  currentRun(): string | null | undefined;
  write(run: string, data: string): Promise<unknown>;
  error(error: unknown): void;
}

export function createTerminalInput(ports: InputPorts, limit = 1024 * 1024) {
  let pending: { run: string; data: string; bytes: number }[] = [];
  let bytes = 0;
  let busy = false;
  let disposed = false;
  async function drain() {
    if (busy) return;
    busy = true;
    try {
      while (pending.length && !disposed) {
        const item = pending.shift()!;
        try {
          if (ports.currentRun() === item.run) await ports.write(item.run, item.data);
        } catch (error) {
          // Input is not replayable after a pipe error; discard this run's queued suffix.
          pending = pending.filter((entry) => {
            if (entry.run !== item.run) return true;
            bytes -= entry.bytes;
            return false;
          });
          if (!disposed && ports.currentRun() === item.run) ports.error(error);
        } finally { bytes -= item.bytes; }
      }
    } finally { busy = false; }
  }
  return {
    send(data: string) {
      const run = ports.currentRun();
      if (disposed || !run || !data) return;
      const size = new TextEncoder().encode(data).length;
      if (bytes + size > limit) { ports.error(new Error(t("终端输入队列已满，请等待当前输入完成"))); return; }
      pending.push({ run, data, bytes: size });
      bytes += size;
      void drain();
    },
    dispose() {
      disposed = true;
      for (const item of pending) bytes -= item.bytes;
      pending = [];
    },
  };
}
