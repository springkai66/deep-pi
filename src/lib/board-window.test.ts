import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const board = readFileSync("src-tauri/src/board.rs", "utf8");
const route = readFileSync("src/routes/board/+page.ts", "utf8");
const mainPage = readFileSync("src/routes/+page.svelte", "utf8");
const lib = readFileSync("src-tauri/src/lib.rs", "utf8");
const capability = readFileSync("src-tauri/capabilities/board-window.json", "utf8");

// 看板浮窗与主窗口解耦，靠任务事件与 IPC 驱动；此处锁住窗口注册、路由与能力清单，
// 避免重建窗口时漏配导致空白或 404。
describe("standalone board window", () => {
  it("prerenders the board route for the standalone window", () => {
    expect(route).toContain("prerender = true");
  });

  it("registers the independent window and loads the extensionless route", () => {
    expect(mainPage).toContain('invoke("open_board_window")');
    expect(board).toContain('pub const BOARD_LABEL: &str = "board"');
    expect(board).toContain("open_board_window");
    // SvelteKit 客户端路由按 URL pathname 匹配：board.html 会渲染 404 页；
    // /board 由 Tauri 资产解析器回退到预渲染的 board.html。
    expect(board).toContain('WebviewUrl::App("board".into())');
    expect(board).not.toContain('"board.html"');
    expect(lib).toContain("board::open_board_window");
    expect(capability).toContain('"windows": ["board"]');
    expect(capability).toContain('"core:default"');
  });
});
