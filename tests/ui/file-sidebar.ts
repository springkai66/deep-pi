import { mount } from "svelte";
import type { FileEntry, FileIndex } from "../../src/lib/files";

const file = (path: string): FileEntry => ({ path, name: path.split("/").at(-1)!, isDirectory: false, size: 1 });
const folder = (path: string): FileEntry => ({ path, name: path, isDirectory: true, size: 0 });
let first = [folder("src"), folder("empty"), file("src/old-main.ts"),
  ...Array.from({ length: 40 }, (_, i) => file(`src/item-${String(i).padStart(2, "0")}.txt`))];
let deferred = false;
let reads = 0;
const pending: Array<{ projectId: string; resolve: (result: FileIndex) => void }> = [];
const snapshot = (projectId: string): FileIndex => ({
  entries: projectId === "second" ? [file("second.txt")] : [...first],
  truncated: false, unreadableDirectories: [],
});

export const fixture = {
  get reads() { return reads; },
  get pendingCount() { return pending.length; },
  defer() { deferred = true; },
  release() {
    deferred = false;
    for (const { projectId, resolve } of pending.splice(0)) resolve(snapshot(projectId));
  },
  update() {
    first = [...first.filter((entry) => entry.path !== "src/old-main.ts"),
      file("src/new-main.ts"), file("empty/child.txt")];
  },
};

Object.assign(window, {
  isTauri: true,
  __TAURI_INTERNALS__: {
    invoke: (command: string, args: { projectId: string; relativePath: string; recursive: boolean }) => {
      if (command !== "list_project_files") throw new Error(`Unsupported fixture command: ${command}`);
      reads++;
      if (args.relativePath) {
        const prefix = `${args.relativePath}/`;
        return Promise.resolve({ ...snapshot(args.projectId),
          entries: snapshot(args.projectId).entries.filter((entry) => entry.path.startsWith(prefix)
            && !entry.path.slice(prefix.length).includes("/")) });
      }
      if (deferred) return new Promise<FileIndex>((resolve) => { pending.push({ projectId: args.projectId, resolve }); });
      return Promise.resolve(snapshot(args.projectId));
    },
  },
});

const { default: FileSidebarFixture } = await import("./FileSidebarFixture.svelte");
mount(FileSidebarFixture, { target: document.getElementById("app")! });
const { run } = await import("./file-sidebar-smoke");
try {
  await run(fixture);
  document.body.dataset.result = "pass";
} catch (error) {
  document.body.dataset.result = "fail";
  document.body.dataset.error = String(error);
  console.error(error);
}
