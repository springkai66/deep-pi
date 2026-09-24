import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// 桌宠任务环涉及三个窗口的协作（pet / pet-tasks / main）与后端命令注册；
// 此处锁住路由、能力清单与事件契约，避免重建窗口时漏配导致悬浮层失效。
const petPage = readFileSync("src/routes/pet/+page.svelte", "utf8");
const tasksPage = readFileSync("src/routes/pet-tasks/+page.svelte", "utf8");
const tasksRoute = readFileSync("src/routes/pet-tasks/+page.ts", "utf8");
const mainPage = readFileSync("src/routes/+page.svelte", "utf8");
const pet = readFileSync("src-tauri/src/pet.rs", "utf8");
const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
const petCapability = readFileSync("src-tauri/capabilities/pet-window.json", "utf8");
const tasksCapability = readFileSync("src-tauri/capabilities/pet-tasks-window.json", "utf8");

describe("standalone pet task bubbles window", () => {
  it("prerenders the pet-tasks route for the standalone window", () => {
    expect(tasksRoute).toContain("prerender = true");
  });

  it("keeps the pet window untouched and only reports hover", () => {
    // 悬停不再改变桌宠窗口尺寸：本体只上报「指针进/出」，环由浮层窗口渲染。
    expect(petPage).toContain('invoke<void>("set_pet_ring", { open: hovered })');
    expect(petPage).not.toContain("import PetTaskBubbles");
    expect(petPage).not.toContain("<PetTaskBubbles");
    expect(petPage).toContain("onpointerenter={() => { hover = true; reportHover(true); }}");
    expect(petPage).toContain("onpointerleave={ringClose}");
  });

  it("builds the hidden popup without resizing the pet", () => {
    expect(pet).toContain('WebviewUrl::App("pet-tasks".into())');
    expect(pet).toContain("PET_TASKS_LABEL");
    expect(pet).toContain("fn set_pet_tasks_visible");
    expect(pet).toContain("fn get_pet_task_hover");
    expect(lib).toContain("pet::set_pet_tasks_visible");
    expect(lib).toContain("pet::get_pet_task_hover");
  });

  it("centers the square ring window on the pet instead of parking it nearby", () => {
    // 环必须围着本体：窗口中心对准本体中心，且刻意不钳位（钳位会让圈偏心）。
    expect(pet).toContain("fn ring_window_position");
    expect(pet).not.toContain("fn task_popup_position");
    expect(pet).toContain("ring_window_position((pos.x, pos.y, size.width, size.height), target)");
    expect(pet).toContain("const TASKS_SIZE: f64 = 960.0;");
    expect(pet).toContain(".inner_size(TASKS_SIZE, TASKS_SIZE)");
    expect(pet).toContain("LogicalSize::new(TASKS_SIZE, TASKS_SIZE)");
  });

  it("authorizes both pet windows for their exact IPC surface", () => {
    expect(petCapability).toContain('"windows": ["pet"]');
    expect(tasksCapability).toContain('"windows": ["pet-tasks"]');
    expect(tasksCapability).toContain("core:event:allow-emit-to");
  });

  it("delegates the stop action through the main window with acks", () => {
    expect(tasksPage).toContain('emitTo("main", PET_TASK_ACTION_EVENT, request)');
    expect(mainPage).toContain("PET_TASK_ACTION_EVENT");
    expect(mainPage).toContain('emitTo("pet", PET_TASK_RESULT_EVENT, result)');
    expect(pet).toContain('"image/gif"');
  });

  it("has no continue path left anywhere in the ring", () => {
    // 「继续」按钮连同整条动作链路一起删除：只留「结束」。
    for (const source of [petPage, tasksPage, pet]) {
      expect(source).not.toContain("canContinuePetTask");
    }
    expect(tasksPage).not.toContain('"continue"');
    expect(pet).not.toContain("force_close");
  });
});
