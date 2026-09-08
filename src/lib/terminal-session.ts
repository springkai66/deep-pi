interface RunEvent {
  taskId: string;
  runId: string;
}

interface OutputEvent extends RunEvent {
  data: string;
}

interface ExitEvent extends RunEvent {
  exitCode: number | null;
  error: string | null;
}

export interface TerminalPorts {
  listen<T>(name: string, handler: (payload: T) => void): Promise<() => void>;
  acknowledge(taskId: string, runId: string): Promise<unknown>;
  output(data: string): void;
  exit(exitCode: number | null, error: string | null): void;
  error(error: unknown): void;
}

export function createTerminalSession(taskId: string, ports: TerminalPorts) {
  let disposed = false;
  let ready = false;
  let currentRun: string | null = null;
  const accepts = (event: RunEvent) =>
    !disposed && event.taskId === taskId && event.runId === currentRun;
  const subscriptions = [
    ports.listen<OutputEvent>("pty-output", (event) => {
      if (accepts(event)) ports.output(event.data);
    }),
    ports.listen<ExitEvent>("pty-exit", (event) => {
      if (accepts(event)) ports.exit(event.exitCode, event.error);
    }),
  ];
  const acknowledge = () => {
    if (disposed || !ready || !currentRun) return;
    const run = currentRun;
    void ports.acknowledge(taskId, run).catch((error) => {
      if (!disposed && currentRun === run) ports.error(error);
    });
  };
  void Promise.all(subscriptions).then(() => {
    ready = true;
    acknowledge();
  }).catch((error) => {
    if (!disposed) ports.error(error);
  });
  return {
    setRun(runId: string | null | undefined) {
      if (disposed || currentRun === runId) return;
      currentRun = runId ?? null;
      acknowledge();
    },
    dispose() {
      disposed = true;
      for (const subscription of subscriptions) {
        void subscription.then((unlisten) => unlisten()).catch(() => {});
      }
    },
  };
}
