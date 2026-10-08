import "../../src/app.css";
import { mount, tick } from "svelte";
import { applyAppearance } from "../../src/lib/appearance";
import { DEFAULT_APP_SETTINGS } from "../../src/lib/settings";
import { COMMAND_FLOW_THEME, FROSTED_GLASS_THEME, type ThemePack } from "../../src/lib/theme";

// Mock only the native boundary. The real components, theme compiler and CSS run in Chromium.
const unexpectedCommands: string[] = [];
Object.assign(window, {
  isTauri: true,
  __TAURI_INTERNALS__: {
    metadata: { currentWindow: { label: "glass-fixture" } },
    invoke: async (command: string) => {
      switch (command) {
        case "rpc_dormant_history": return {
          snapshotId: "glass-history", eventSequence: 0, start: 0, end: 2, total: 2,
          messages: [
            { role: "user", content: [{ type: "text", text: "检查用户气泡" }] },
            { role: "assistant", content: [{ type: "text", text: "检查助手气泡" }] },
          ],
        };
        case "list_project_files": return {
          entries: [{ path: "main.ts", name: "main.ts", isDirectory: false, size: 1 }],
          truncated: false, unreadableDirectories: [],
        };
        case "pi_auth_status": return { credentials: [] };
        case "pi_auth_model_selection": return { modelIds: null };
        case "plugin:app|version": return "0.0.0";
        case "list_pi_providers":
        case "list_tasks":
        case "list_installed_workflows":
        case "list_system_fonts": return [];
        default:
          unexpectedCommands.push(command);
          throw new Error(`Unsupported glass fixture command: ${command}`);
      }
    },
  },
});

const { default: GlassThemeFixture } = await import("./GlassThemeFixture.svelte");
mount(GlassThemeFixture, { target: document.getElementById("app")! });

const surfaces = [
  ".project-sidebar", ".file-panel", ".chat-pane", ".settings-modal", ".app-dialog", ".editor-modal",
  ".chat-pane article:not(.user-message) .message-content", ".chat-pane .user-message .message-content",
  ".chat-pane .composer",
];
const clearChildren = [
  ".project-sidebar .task-sidebar", ".file-panel .file-sidebar",
  ".settings-modal .settings-page", ".settings-modal .settings-header", ".settings-modal .settings-navigation",
];

function element(selector: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(selector);
  if (!found) throw new Error(`Missing surface: ${selector}`);
  return found;
}

function backgroundAlpha(node: HTMLElement, pseudo?: string): number {
  // Canvas accepts Chromium's computed rgb/color()/color-mix() serialization.
  const context = document.createElement("canvas").getContext("2d")!;
  context.fillStyle = getComputedStyle(node, pseudo).backgroundColor;
  context.fillRect(0, 0, 1, 1);
  return context.getImageData(0, 0, 1, 1).data[3] / 255;
}

function expectAlpha(selector: string, expected: number, label: string, pseudo?: string) {
  const actual = backgroundAlpha(element(selector), pseudo);
  if (Math.abs(actual - expected) > 0.01) {
    throw new Error(`${label}: ${selector} background alpha ${actual.toFixed(3)}, expected ${expected}`);
  }
}

function expectPanelAlpha(selector: string, expected: number, label: string) {
  let remainingTransparency = 1;
  for (let node: HTMLElement | null = element(selector); node && !node.classList.contains("app-shell"); node = node.parentElement) {
    remainingTransparency *= 1 - backgroundAlpha(node);
  }
  const actual = 1 - remainingTransparency;
  if (Math.abs(actual - expected) > 0.01) {
    throw new Error(`${label}: ${selector} composed panel alpha ${actual.toFixed(3)}, expected ${expected}`);
  }
}

async function appearance(theme: ThemePack, colorMode: "light" | "dark", themeTransparency: number) {
  applyAppearance({ ...DEFAULT_APP_SETTINGS, theme: theme.id, colorMode, themeTransparency, customThemes: [theme] });
  await tick();
}

async function run() {
  const deadline = Date.now() + 10000;
  while (!document.querySelector(".chat-pane .user-message .message-content") || !document.querySelector('.file-tree [role="treeitem"]')) {
    if (Date.now() > deadline) throw new Error("Glass fixture did not finish rendering history and files");
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  element("#settings-nav-models").click();
  await tick();
  while (!document.querySelector(".provider-header .quiet-button")) {
    if (Date.now() > deadline) throw new Error("Glass fixture did not finish rendering providers");
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  element(".provider-header .quiet-button").click();
  await tick();
  while (!document.querySelector(".editor-modal")) {
    if (Date.now() > deadline) throw new Error("Glass fixture did not open the provider dialog");
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  const imported = { ...FROSTED_GLASS_THEME, id: "imported-glass", effects: { ...FROSTED_GLASS_THEME.effects, readingSurfaceOpacity: 90 } };
  const results: string[] = [];
  for (const colorMode of ["light", "dark"] as const) {
    await appearance(COMMAND_FLOW_THEME, colorMode, 50);
    const restoredSurfaces = [...surfaces, ...clearChildren, ".editor-backdrop"];
    const regular = restoredSurfaces.map((selector) => getComputedStyle(element(selector)).backgroundColor);
    for (const theme of [FROSTED_GLASS_THEME, imported]) {
      for (const transparency of [0, 50, 100]) {
        await appearance(theme, colorMode, transparency);
        const label = `${theme.id}/${colorMode}/${transparency}`;
        for (const selector of surfaces) expectAlpha(selector, 1 - transparency / 100, label);
        for (const selector of clearChildren) expectAlpha(selector, 0, label);
        for (const selector of [".project-sidebar .task-sidebar", ".file-panel .file-sidebar", ".chat-pane"]) {
          expectPanelAlpha(selector, 1 - transparency / 100, label);
        }
        // Exercise the host's transparent fallback with the same real components.
        document.documentElement.dataset.windowMaterial = "transparent";
        document.documentElement.dataset.nativeMaterialFallback = "system_transparency_disabled";
        expectAlpha(".app-shell", 0, `${label} native fallback`);
        for (const selector of surfaces) expectAlpha(selector, 1 - transparency / 100, `${label} native fallback`);
        delete document.documentElement.dataset.nativeMaterialFallback;
        document.documentElement.dataset.windowMaterial = "none";
        expectAlpha(".dialog-backdrop", 1 - transparency / 100, label);
        expectAlpha(".editor-backdrop", 1 - transparency / 100, label);
        expectAlpha(".app-dialog", 0, `${label} native backdrop`, "::backdrop");
        const textAlpha = getComputedStyle(element(".chat-pane .message-content")).opacity;
        if (textAlpha !== "1") throw new Error(`${label}: message text opacity changed`);
        results.push(label);
      }
    }
    await appearance(COMMAND_FLOW_THEME, colorMode, 50);
    for (const [index, selector] of restoredSurfaces.entries()) {
      if (getComputedStyle(element(selector)).backgroundColor !== regular[index]) {
        throw new Error(`${colorMode}: ${selector} did not restore its regular theme background`);
      }
    }
  }
  element("#settings-nav-general").click();
  await tick();
  const themePicker = element('select[aria-label="主题"]') as HTMLSelectElement;
  themePicker.value = FROSTED_GLASS_THEME.id;
  themePicker.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  document.documentElement.dataset.nativeMaterialFallback = "system_transparency_disabled";
  if (getComputedStyle(element(".native-material-transparent-notice")).display === "none") {
    throw new Error("Transparent fallback notice is hidden");
  }
  delete document.documentElement.dataset.nativeMaterialFallback;
  if (getComputedStyle(element(".native-material-transparent-notice")).display !== "none") {
    throw new Error("Transparent fallback notice remains visible after recovery");
  }
  if (unexpectedCommands.length) throw new Error(`Unsupported glass fixture commands: ${unexpectedCommands.join(", ")}`);
  const fixtureError = document.querySelector('.settings-modal [role="alert"]');
  if (fixtureError) throw new Error(`Settings fixture error: ${fixtureError.textContent}`);
  return results;
}

try {
  const results = await run();
  document.body.dataset.result = "pass";
  document.body.insertAdjacentHTML("beforeend", `<pre id="glass-theme-result">PASS ${results.join("; ")}; regular themes restored</pre>`);
} catch (error) {
  document.body.dataset.result = "fail";
  const output = document.createElement("pre");
  output.id = "glass-theme-result";
  output.textContent = `FAIL ${String(error)}`;
  document.body.append(output);
  console.error(error);
}
