import { mount, tick } from "svelte";
import TaskSidebar from "../../src/lib/TaskSidebar.svelte";
import { applyAppearance } from "../../src/lib/appearance";
import { DEFAULT_APP_SETTINGS } from "../../src/lib/settings";
import { FROSTED_GLASS_THEME } from "../../src/lib/theme";
import type { Task } from "../../src/lib/task";
import "../../src/app.css";

const project = { id: "project", name: "Menu fixture", path: "F:/menu-fixture", createdAt: 0, lastOpenedAt: 0 };
const task: Task = {
  id: "task", title: "Session fixture", agent: "pi", status: "waiting",
  projectId: project.id, projectPath: project.path, sessionId: "session", sessionFile: null,
  executionTarget: "local", createdAt: 0, startedAt: null, completedAt: null, archivedAt: null,
};
let renamed = false;
const noop = () => {};
mount(TaskSidebar, {
  target: document.querySelector("#task-menu-fixture")!,
  props: {
    projects: [project], tasks: [task], selectedProjectId: project.id,
    onAddProject: noop, onOpenProject: noop, onOpen: noop, onRename: () => { renamed = true; },
    onStop: noop, onArchive: noop, onRestart: noop, onRestore: noop,
    onDelete: noop, onAddSession: noop, onRemoveProject: noop,
  },
});

async function openMenu(selector: string, x: number, y: number, label: string) {
  document.querySelector(selector)!.dispatchEvent(new MouseEvent("contextmenu", {
    bubbles: true, cancelable: true, clientX: x, clientY: y,
  }));
  await tick();
  const menu = document.querySelector<HTMLElement>(".context-menu")!;
  if (!menu) throw new Error(`${label}: missing menu`);
  const rect = menu.getBoundingClientRect();
  if (rect.left < 0 || rect.top < 0 || rect.right > innerWidth || rect.bottom > innerHeight) {
    throw new Error(`${label}: menu outside viewport: ${JSON.stringify(rect.toJSON())}`);
  }
  for (const button of menu.querySelectorAll("button")) {
    const item = button.getBoundingClientRect();
    for (const x of [item.left + 4, item.right - 4]) {
      const hit = document.elementFromPoint(x, (item.top + item.bottom) / 2);
      if (!button.contains(hit)) {
        throw new Error(`${label}: ${button.textContent?.trim()} clipped at x=${x.toFixed(1)}; hit=${hit?.className}`);
      }
    }
  }
  return menu;
}

async function smoke() {
  await tick();
  let cases = 0;
  for (const theme of [DEFAULT_APP_SETTINGS.theme, FROSTED_GLASS_THEME.id]) {
    for (const colorMode of ["light", "dark"] as const) {
      applyAppearance({ ...DEFAULT_APP_SETTINGS, theme, colorMode });
      for (const [x, y] of [[190, 180], [innerWidth - 1, innerHeight - 1], [1, 1]]) {
        for (const [selector, name] of [[".task-row", "Session"], [".project-header", "project"]]) {
          const menu = await openMenu(selector, x, y, `${theme}/${colorMode}/${name}/${x},${y}`);
          menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
          await tick();
          if (document.querySelector(".context-menu")) throw new Error("Escape did not dismiss menu");
          cases++;
        }
      }
    }
  }
  const menu = await openMenu(".task-row", 190, 180, "rename action");
  menu.querySelector<HTMLButtonElement>("button")!.click();
  await tick();
  if (!renamed || document.querySelector(".context-menu")) throw new Error("Rename did not fire and dismiss");
  await openMenu(".task-row", 190, 180, "outside click");
  document.body.click();
  await tick();
  if (document.querySelector(".context-menu")) throw new Error("Outside click did not dismiss");
  return `PASS ${cases} Session/project menu cases; edge visibility, Escape, outside click and rename`;
}

smoke().then(
  async (result) => {
    document.querySelector("#glass-theme-result")!.textContent = result;
    if (new URLSearchParams(location.search).has("preview")) {
      await openMenu(".task-row", 195, 180, "menu preview");
    }
  },
  (error) => { document.querySelector("#glass-theme-result")!.textContent = `FAIL ${error.stack ?? error}`; },
);
