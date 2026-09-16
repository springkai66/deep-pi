import { t } from "./i18n.svelte";
export interface ProjectWatchEvent {
  projectId: string;
  watchId: string;
  files: boolean;
  git: boolean;
  status: "changed" | "rootChanged" | "unavailable" | "expired";
}
interface Ports {
  start(id: string, changed: (event: ProjectWatchEvent) => void): Promise<void>;
  close(id: string): Promise<void>;
  ping(id: string): Promise<void>;
  changed(files: boolean, git: boolean): void;
  status(error: string): void;
}
export function createProjectWatch(projectId: string, ports: Ports) {
  const id = crypto.randomUUID();
  let closed = false;
  let failed = false;
  let starting: Promise<void> | undefined;
  let releasing: Promise<void> | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;
  let pinging = false;
  function release() {
    return releasing ??= ports.close(id).catch(() => {
      // The backend's bounded lease expires even if this cleanup IPC is lost.
    });
  }
  function fail(message: string) {
    if (closed || failed) return;
    failed = true;
    clearInterval(timer);
    ports.status(message);
    void starting?.finally(release);
  }
  function receive(event: ProjectWatchEvent) {
    if (closed || failed || event.projectId !== projectId || event.watchId !== id) return;
    ports.changed(event.files, event.git);
    if (event.status !== "changed") {
      fail(t(event.status === "rootChanged" ? "项目目录已变化，请重新连接文件监听"
        : event.status === "expired" ? "文件监听已过期，请重新连接" : "文件监听已中断，可手动刷新或重新连接"));
    }
  }
  return {
    start() {
      if (closed) return Promise.resolve();
      return starting ??= ports.start(id, receive).then(() => {
        if (closed || failed) { void release(); return; }
        ports.status("");
        ports.changed(true, true);
        timer = setInterval(() => {
          if (closed || failed || pinging) return;
          pinging = true;
          void ports.ping(id).catch(() => fail(t("文件监听连接已失效，请重新连接"))).finally(() => { pinging = false; });
        }, 30_000);
      }).catch(() => {
        fail(t("无法启用文件监听，可手动刷新或重新连接"));
        void release();
      });
    },
    async dispose() {
      closed = true;
      clearInterval(timer);
      await starting;
      if (starting) await release();
    },
  };
}
