import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const page = readFileSync("src/routes/checklist/+page.svelte", "utf8");
const route = readFileSync("src/routes/checklist/+page.ts", "utf8");
const mainPage = readFileSync("src/routes/+page.svelte", "utf8");
const checklist = readFileSync("src-tauri/src/checklist.rs", "utf8");
const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
const capability = readFileSync("src-tauri/capabilities/checklist-window.json", "utf8");

// Keep the standalone checklist wired to both the Rust command surface and its
// prerendered route; this catches an accidentally empty window implementation.
describe("standalone checklist window", () => {
  it("renders a functional route with persistence controls", () => {
    expect(route).toContain("prerender = true");
    expect(page).toContain("getCurrentWindow");
    expect(page).toContain("applyAppearance");
    expect(page).toContain('invoke<ChecklistItem[]>("list_checklist_items")');
    expect(page).toContain('invoke("add_checklist_item"');
    expect(page).toContain('invoke("toggle_checklist_item"');
    expect(page).toContain('invoke("delete_checklist_item"');
    expect(page).toContain("data-tauri-drag-region");
    expect(page).toContain("startResizeDragging");
    expect(page).toContain("ClipboardList");
    expect(page).toContain("重要且紧急");
    expect(page).toContain("重要但不紧急");
    expect(page).toContain("紧急但不重要");
    expect(page).toContain("不重要也不紧急");
    expect(page).toContain("request: { text, quadrant }");
  });

  it("registers the independent window and all required capabilities", () => {
    expect(mainPage).toContain('invoke("open_checklist_window")');
    expect(mainPage).toContain("四象限清单");
    expect(checklist).toContain('pub const CHECKLIST_LABEL: &str = "checklist"');
    expect(checklist).toContain("open_checklist_window");
    expect(checklist).toContain("cfg!(dev)");
    expect(checklist).toContain("checklist.html");
    expect(checklist).toContain("quadrant INTEGER");
    expect(checklist).toContain("checklist.quadrant.invalid");
    expect(lib).toContain("mod checklist;");
    expect(lib).toContain("checklist::ChecklistStore::open");
    expect(lib).toContain("ChecklistStore::open(&paths.checklist)");
    expect(lib).toContain("checklist::list_checklist_items");
    expect(lib).toContain("checklist::add_checklist_item");
    expect(lib).toContain("checklist::toggle_checklist_item");
    expect(lib).toContain("checklist::delete_checklist_item");
    expect(capability).toContain('"windows": ["checklist"]');
    expect(capability).toContain('"core:default"');
    expect(capability).toContain('"core:window:allow-close"');
    expect(capability).toContain('"core:window:allow-set-title"');
    expect(capability).toContain('"core:window:allow-start-dragging"');
    expect(capability).toContain('"core:window:allow-start-resize-dragging"');
  });
});
