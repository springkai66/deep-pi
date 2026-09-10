import type { Project } from "./project";
import type { RuntimeComponent } from "./runtime";
import type { AppSettings } from "./settings";
import type { Task } from "./task";

export interface StartupPorts {
  settings(): Promise<AppSettings>;
  tasks(): Promise<Task[]>;
  projects(): Promise<Project[]>;
  runtimes(): Promise<RuntimeComponent[]>;
  applySettings(settings: AppSettings): void;
  applyWorkspace(tasks: Task[], projects: Project[], settings: AppSettings): void;
  applyRuntimes(runtimes: RuntimeComponent[]): void;
  error(scope: "workspace" | "runtime", error: unknown): void;
}

export function loadAppStartup(ports: StartupPorts) {
  let disposed = false;
  const settings = ports.settings().then((value) => {
    if (!disposed) ports.applySettings(value);
    return value;
  });
  const workspace = Promise.all([ports.tasks(), ports.projects()])
    .then(([tasks, projects]) => ({ tasks, projects }));
  const ready = Promise.all([settings, workspace]).then(([settings, workspace]) => {
    if (disposed) return false;
    ports.applyWorkspace(workspace.tasks, workspace.projects, settings);
    return true;
  }).catch((error: unknown) => {
    if (!disposed) ports.error("workspace", error);
    return false;
  });
  const background = ports.runtimes().then((runtimes) => {
    if (!disposed) ports.applyRuntimes(runtimes);
  }).catch((error: unknown) => {
    if (!disposed) ports.error("runtime", error);
  });
  return { ready, background, dispose: () => { disposed = true; } };
}
