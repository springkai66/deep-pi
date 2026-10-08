import { mount, tick } from "svelte";
import FileSidebar from "../../src/lib/FileSidebar.svelte";
import { applyAppearance } from "../../src/lib/appearance";
import { DEFAULT_APP_SETTINGS } from "../../src/lib/settings";
import { FROSTED_GLASS_THEME } from "../../src/lib/theme";
import type { FileTreeAction } from "../../src/lib/files";
import "../../src/app.css";

const entries = [
  { path: "folder", name: "folder", isDirectory: true, size: 0 },
  { path: "file.txt", name: "file.txt", isDirectory: false, size: 10 },
];
Reflect.set(window, "__TAURI_INTERNALS__", {
  invoke: async (command: string) => {
    if (command !== "list_project_files") throw new Error(`Unexpected fixture command: ${command}`);
    return { entries, truncated: false, unreadableDirectories: [] };
  },
});
let action: FileTreeAction | undefined;
Reflect.set(globalThis, "isTauri", true);
mount(FileSidebar, {
  target: document.querySelector("#file-menu-fixture")!,
  props: {
    project: { id: "fixture", name: "Files fixture", path: "F:/files-fixture", createdAt: 0, lastOpenedAt: 0 },
    onOpen: () => {}, onAction: async (selected) => { action = selected; },
    searchFocusToken: 0, visible: true, refreshToken: 0,
  },
});

async function openMenu(selector: string, x: number, y: number, label: string) {
  const header = document.querySelector<HTMLElement>(".file-sidebar > header")!;
  const before = header.getBoundingClientRect();
  const count = document.querySelectorAll('[role="treeitem"]').length;
  document.querySelector(selector)!.dispatchEvent(new MouseEvent("contextmenu", {
    bubbles: true, cancelable: true, clientX: x, clientY: y,
  }));
  await tick();
  const menu = document.querySelector<HTMLElement>(".context-menu");
  if (!menu) throw new Error(`${label}: menu did not open`);
  const after = header.getBoundingClientRect();
  if (after.x !== before.x || after.y !== before.y || !header.contains(document.elementFromPoint(
    before.x + before.width / 2, before.y + before.height / 2,
  ))) throw new Error(`${label}: file panel disappeared; header x=${before.x} -> ${after.x}, y=${before.y} -> ${after.y}`);
  if (document.querySelectorAll('[role="treeitem"]').length !== count) throw new Error(`${label}: tree rows disappeared`);
  const rect = menu.getBoundingClientRect();
  if (rect.left < 0 || rect.top < 0 || rect.right > innerWidth || rect.bottom > innerHeight) {
    throw new Error(`${label}: menu leaves viewport`);
  }
  for (const button of menu.querySelectorAll("button")) {
    const item = button.getBoundingClientRect();
    for (const point of [item.left + 4, item.right - 4]) {
      if (!button.contains(document.elementFromPoint(point, item.top + item.height / 2))) {
        throw new Error(`${label}: menu action clipped: ${button.textContent}`);
      }
    }
  }
  return menu;
}

async function smoke() {
  await tick();
  await tick();
  await new Promise(resolve => setTimeout(resolve, 0));
  if (document.querySelectorAll('[role="treeitem"]').length !== 2) throw new Error("Fixture files did not load");
  let cases = 0;
  for (const theme of [DEFAULT_APP_SETTINGS.theme, FROSTED_GLASS_THEME.id]) {
    for (const colorMode of ["light", "dark"] as const) {
      Reflect.set(globalThis, "isTauri", false);
      applyAppearance({ ...DEFAULT_APP_SETTINGS, theme, colorMode });
      Reflect.set(globalThis, "isTauri", true);
      for (const [x, y] of [[innerWidth - 180, 300], [innerWidth - 1, innerHeight - 1]]) {
        for (const selector of ['[role="treeitem"][aria-expanded]', '[role="treeitem"]:not([aria-expanded])', ".file-tree"]) {
          const label = `${theme}/${colorMode}/${selector}/${x},${y}`;
          const menu = await openMenu(selector, x, y, label);
          const buttons = [...menu.querySelectorAll<HTMLButtonElement>("button")];
          if (document.activeElement !== buttons[0]) throw new Error(`${label}: first action did not receive focus`);
          for (const [key, index] of [["End", buttons.length - 1], ["Home", 0], ["ArrowDown", 1], ["ArrowUp", 0]] as const) {
            menu.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
            if (document.activeElement !== buttons[index]) throw new Error(`${label}: ${key} did not move focus`);
          }
          menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
          await tick();
          if (document.querySelector(".context-menu")) throw new Error(`${label}: Escape did not dismiss`);
          if (document.activeElement !== document.querySelector(selector)) throw new Error(`${label}: Escape did not restore focus`);
          cases++;
        }
      }
    }
  }
  const menu = await openMenu('[role="treeitem"][aria-expanded]', innerWidth - 180, 300, "copy action");
  menu.querySelectorAll<HTMLButtonElement>("button")[3].click();
  await tick();
  if (action !== "copyPath" || document.querySelector(".context-menu")) throw new Error("Copy action did not fire and dismiss");
  await openMenu(".file-tree", innerWidth - 180, 300, "outside click");
  document.body.click();
  await tick();
  if (document.querySelector(".context-menu")) throw new Error("Outside click did not dismiss");
  return `PASS ${cases} directory/file/blank menu cases; file panel preserved, actions visible, keyboard navigation, focus restoration, copy and outside click`;
}
smoke().then(
  result => { document.querySelector("#glass-theme-result")!.textContent = result; },
  error => { document.querySelector("#glass-theme-result")!.textContent = `FAIL ${error.stack ?? error}`; },
);
