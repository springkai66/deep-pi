import type { fixture } from "./file-sidebar";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

async function waitFor(check: () => unknown, message: string) {
  const deadline = Date.now() + 6000;
  while (Date.now() < deadline) {
    if (check()) return;
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  throw new Error(`Timed out: ${message}`);
}

const tree = () => document.querySelector<HTMLElement>('[role="tree"][aria-label="文件目录"]')!;
const row = (path: string) => [...tree().querySelectorAll<HTMLButtonElement>('[role="treeitem"]')]
  .find((item) => item.title === path);
const loading = () => document.querySelector('.file-sidebar [role="status"]')?.textContent?.includes("正在读取项目文件");
const click = (selector: string) => document.querySelector<HTMLButtonElement>(selector)!.click();

export async function run(control: typeof fixture) {
  await waitFor(() => row("src"), "initial project index");
  row("src")!.click();
  row("empty")!.click();
  await waitFor(() => tree().textContent?.includes("空目录"), "empty folder marker");
  assert(row("src")?.getAttribute("aria-expanded") === "true", "initial expanded folder");
  const context = (target: HTMLElement) => target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 30, clientY: 30 }));
  const menuLabels = () => [...document.querySelectorAll('[role="menuitem"]')].map((item) => item.textContent?.trim());
  context(row("src")!);
  await waitFor(() => menuLabels().length === 5, "folder context menu");
  assert(menuLabels().includes("新建文件") && menuLabels().includes("新建文件夹") && menuLabels().includes("移到回收站") && !menuLabels().includes("打开并编辑"), "folder actions");
  document.querySelector<HTMLElement>('[role="menuitem"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await waitFor(() => !document.querySelector('[role="menu"]'), "Escape dismisses menu");
  tree().scrollTop = 170;
  tree().dispatchEvent(new Event("scroll"));
  const scroll = tree().scrollTop;
  assert(scroll > 0, "fixture tree is scrollable");

  control.defer();
  click("#focus-change");
  await waitFor(() => control.pendingCount > 0, "focus refresh started");
  assert(!loading() && row("src") && tree().textContent?.includes("空目录"), "focus refresh must keep old tree");
  assert(tree().scrollTop === scroll, "focus refresh must keep scroll position");
  control.release();
  await waitFor(() => control.pendingCount === 0, "focus refresh completed");
  await new Promise((resolve) => setTimeout(resolve, 50));
  assert(tree().scrollTop === scroll, "focus refresh completion must keep scroll position");
  assert(row("src")?.getAttribute("aria-expanded") === "true", "expanded folder survives focus refresh");

  const search = document.querySelector<HTMLInputElement>('.file-search input')!;
  search.value = "main";
  search.dispatchEvent(new Event("input", { bubbles: true }));
  await waitFor(() => row("src/old-main.ts"), "filename search");
  context(row("src/old-main.ts")!);
  await waitFor(() => menuLabels().length === 5, "file context menu");
  assert(menuLabels().includes("打开并编辑") && menuLabels().includes("在外部编辑器中打开") && menuLabels().includes("移到回收站") && !menuLabels().includes("新建文件"), "file actions");
  [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((item) => item.textContent?.includes("复制完整路径"))!.click();
  await waitFor(() => document.querySelector('#last-action')?.textContent === "copyPath:src/old-main.ts", "file action delivered");
  context(tree());
  await waitFor(() => menuLabels().length === 2, "blank area menu");
  assert(menuLabels().join(",") === "新建文件,新建文件夹", "root creation actions");
  document.querySelector<HTMLButtonElement>('[role="menuitem"]')!.click();
  await waitFor(() => document.querySelector('#last-action')?.textContent === "newFile:root", "root action delivered");
  const before = control.reads;
  click("#replace-project");
  await new Promise((resolve) => setTimeout(resolve, 100));
  assert(control.reads === before && search.value === "main", "metadata change must not reset the tree");

  control.defer();
  click("#watch-change");
  await waitFor(() => control.pendingCount > 0, "watch refresh started");
  assert(row("src/old-main.ts") && !loading() && search.value === "main", "watch refresh must keep filtered tree");
  control.update();
  assert(row("src/old-main.ts") && !row("src/new-main.ts"), "pending refresh must not show partial index");
  control.release();
  await waitFor(() => row("src/new-main.ts") && !row("src/old-main.ts"), "watch refresh updated index");
  assert(search.value === "main", "filename search survives watch refresh");

  search.value = "";
  search.dispatchEvent(new Event("input", { bubbles: true }));
  await waitFor(() => row("empty/child.txt"), "new child appears in expanded directory");
  assert(!tree().textContent?.includes("空目录"), "old empty marker removed when child appears");

  const oldReads = control.reads;
  control.defer();
  click('.file-sidebar button[aria-label="刷新文件"]');
  await waitFor(() => control.reads > oldReads && control.pendingCount > 0, "manual refresh issued read");
  await waitFor(() => loading(), "manual refresh shows loading state");
  control.release();
  await waitFor(() => !loading(), "manual refresh finished");
  click("#switch-project");
  await waitFor(() => row("second.txt"), "second project index");
  assert(!row("src") && search.value === "", "project switch resets old directory");
}
