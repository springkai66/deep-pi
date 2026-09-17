//! 任务看板浮窗：贴着主窗口右侧停靠的独立 OS 窗口。
//!
//! 与 DSH 的「主窗口内嵌子 webview」不同，看板是真正的独立窗口：
//! 无边框（自绘标题栏 + 关闭按钮）、不进任务栏，打开时停靠在主窗口
//! 右缘，主窗口移动/缩放时跟随（Moved/Resized 事件高频触发，60ms 防抖
//! 只让最后一次计算生效）。跟随只同步位置：看板大小由用户通过页面的
//! 八方向贴边手柄自由调整，仅首次打开时高度对齐主窗口。
//! 主窗口销毁时一并销毁看板，避免孤儿浮窗。

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};
/// 看板窗口标签。能力清单（capabilities/board-window.json）按它授权。
pub const BOARD_LABEL: &str = "board";
/// 看板默认/最小宽度（物理像素近似值；逻辑宽度按 DPI 换算差异可接受）。
pub const BOARD_DEFAULT_WIDTH: u32 = 420;
pub const BOARD_MIN_WIDTH: u32 = 300;
/// open_board_window 的在途创建标志：并发双击时只允许一次 build。
static BOARD_OPEN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
/// 计算看板在主窗口右侧的停靠位置（全部物理像素）。
/// 规则：x 贴主窗口右缘，y 顶对齐主窗口；看板宽高由调用方传入（用户当前值）。
/// 右侧放不下时翻转贴主窗口左缘；仍越界时收进屏幕内。
/// 返回 (x, y, width, height)。
// 参数是主窗口 / 显示器 / 看板的坐标尺寸原语，与 OS 窗口 API 一一对应，不再拆分。
#[allow(clippy::too_many_arguments)]
fn board_dock_bounds(
    main_x: i32,
    main_y: i32,
    main_width: u32,
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    board_width: u32,
    board_height: u32,
) -> (i32, i32, u32, u32) {
    let board_width = board_width.max(BOARD_MIN_WIDTH).min(monitor_width.max(1));
    let board_height = board_height.max(1);
    let monitor_left = monitor_x;
    let monitor_right = monitor_x + monitor_width as i32;
    let monitor_top = monitor_y;
    let monitor_bottom = monitor_y + monitor_height as i32;

    // 横向：贴主窗口右缘；右侧放不下翻到主窗口左缘；仍出屏时钳进屏幕
    //（先贴屏幕右缘，宽度比屏幕还窄时兜底贴屏幕左缘）。
    let mut x = main_x + main_width as i32;
    if x + board_width as i32 > monitor_right {
        x = main_x - board_width as i32;
    }
    if x < monitor_left {
        x = monitor_left;
    }
    if x + board_width as i32 > monitor_right {
        x = monitor_right - board_width as i32;
    }
    if x < monitor_left {
        x = monitor_left;
    }

    // 垂直：对齐主窗口顶部，超屏底部时上收；主窗口本身越屏时贴屏幕顶部并裁高度。
    let mut y = main_y;
    let mut height = board_height;
    if y + height as i32 > monitor_bottom {
        y = monitor_bottom - height as i32;
    }
    if y < monitor_top {
        y = monitor_top;
        height = monitor_bottom.saturating_sub(monitor_top) as u32;
    }
    (x, y, board_width, height)
}

/// 读取主窗口与看板当前的几何信息并重新停靠看板；任一窗口缺失则静默跳过。
///
/// `align_height` 只在首次打开时为 true：让看板高度对齐主窗口（贴边观感）。
/// 之后的所有停靠调用（跟随主窗口移动/缩放、聚焦）只同步位置——x 贴主窗口
/// 右缘、y 顶对齐并钳在屏幕内——宽高一律保留用户拖拽调整后的值。
pub fn dock_board_to_main(app: &AppHandle, align_height: bool) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let Some(board) = app.get_webview_window(BOARD_LABEL) else {
        return;
    };
    let (Ok(position), Ok(main_size)) = (main.outer_position(), main.outer_size()) else {
        return;
    };
    let Ok(inner) = board.inner_size() else {
        return;
    };
    let monitor = main
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| (*monitor.position(), *monitor.size()));
    let board_width = inner.width.max(1);
    // 首次打开：高度贴主窗口；此后只改位置，大小完全交给用户。
    let board_height = if align_height {
        main_size.height.max(1)
    } else {
        inner.height.max(1)
    };
    let (fallback_x, fallback_y) = (position.x, position.y);
    let (x, y, _, _) = match monitor {
        Some((m_position, m_size)) => board_dock_bounds(
            position.x,
            position.y,
            main_size.width,
            m_position.x,
            m_position.y,
            m_size.width,
            m_size.height,
            board_width,
            board_height,
        ),
        // 拿不到显示器信息（少见）时不做钳制，按主窗口右缘直接停靠。
        None => (
            fallback_x + main_size.width as i32,
            fallback_y,
            board_width,
            board_height,
        ),
    };
    let _ = board.set_position(PhysicalPosition::new(x, y));
    if align_height {
        let _ = board.set_size(PhysicalSize::new(board_width, board_height));
    }
}

/// 打开（或聚焦已存在的）任务看板浮窗。
#[tauri::command]
pub async fn open_board_window(app: AppHandle) -> Result<(), String> {
    if let Some(board) = app.get_webview_window(BOARD_LABEL) {
        let _ = board.unminimize();
        let _ = board.show();
        let _ = board.set_focus();
        dock_board_to_main(&app, false);
        return Ok(());
    }
    // 双击按钮会连续触发两次 invoke：竞态下第二次 build 会因 label 冲突失败，
    // 反而弹错误并误启用应用内看板。用「在途」标志把并发创建折叠成一次；
    // 撞上时静默返回，首个创建总会完成，下次点击会走聚焦分支。
    if BOARD_OPEN_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = build_board_window(&app);
    BOARD_OPEN_IN_FLIGHT.store(false, Ordering::SeqCst);
    result
}

fn build_board_window(app: &AppHandle) -> Result<(), String> {
    // dev 模式走 vite 开发服务器（/board 路由）；发布构建用预渲染出的 board.html。
    let url_path: &str = if cfg!(dev) { "board" } else { "board.html" };
    let board = WebviewWindowBuilder::new(app, BOARD_LABEL, WebviewUrl::App(url_path.into()))
        .title("DeepPi")
        .decorations(false)
        .resizable(true)
        .minimizable(false)
        .maximizable(false)
        .closable(true)
        .skip_taskbar(true)
        .inner_size(BOARD_DEFAULT_WIDTH as f64, 640.0)
        .min_inner_size(BOARD_MIN_WIDTH as f64, 420.0)
        .visible(false)
        .build()
        .map_err(|error| format!("failed to create board window: {error}"))?;
    dock_board_to_main(app, true);
    let _ = board.show();
    let _ = board.set_focus();
    Ok(())
}

/// 在 setup 阶段挂上主窗口的事件钩子：移动/缩放跟随、销毁时带走看板。
pub fn install_main_window_hooks(app: &AppHandle) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let generation = Arc::new(AtomicU64::new(0));
    let app_for_geometry = app.clone();
    let generation_for_geometry = generation.clone();
    main.on_window_event(move |event| {
        if !matches!(
            event,
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
        ) {
            return;
        }
        // 拖动期间事件极密：只让 60ms 后仍是最新一次的计算真正落盘。
        let stamp = generation_for_geometry.fetch_add(1, Ordering::Relaxed) + 1;
        let app = app_for_geometry.clone();
        let generation = generation_for_geometry.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            if generation.load(Ordering::Relaxed) == stamp {
                dock_board_to_main(&app, false);
            }
        });
    });
    let app_for_exit = app.clone();
    main.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            if let Some(board) = app_for_exit.get_webview_window(BOARD_LABEL) {
                let _ = board.destroy();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::board_dock_bounds;

    #[test]
    fn docks_at_right_edge_aligned_with_main() {
        // 主窗口 (100,100) 1280×800，屏幕 1920×1080：看板贴右缘、顶对齐、高度同步。
        let bounds = board_dock_bounds(100, 100, 1280, 0, 0, 1920, 1080, 420, 800);
        assert_eq!(bounds, (1380, 100, 420, 800));
    }

    #[test]
    fn clamps_into_monitor_when_flip_would_leave_the_screen() {
        // 主窗口已接近屏幕右缘，翻转后左缘也出屏：钳到屏幕内（贴屏幕左缘）。
        let bounds = board_dock_bounds(200, 0, 1600, 0, 0, 1920, 1080, 420, 1000);
        assert_eq!(bounds.0, 0);
    }

    #[test]
    fn clamps_to_monitor_when_flipping_also_overflows() {
        // 主窗口几乎占满屏幕：翻转后仍出屏，最终贴屏幕左缘。
        let bounds = board_dock_bounds(0, 0, 1900, 0, 0, 1920, 1080, 420, 1080);
        assert_eq!(bounds.0, 0);
    }

    #[test]
    fn keeps_height_inside_monitor_bottom() {
        // 主窗口底部超出屏幕：看板上移，始终落在屏幕内。
        let bounds = board_dock_bounds(0, 600, 800, 0, 0, 1920, 1080, 420, 900);
        assert_eq!(bounds, (800, 1080 - 900, 420, 900));
    }

    #[test]
    fn clamps_height_when_main_is_taller_than_monitor() {
        let bounds = board_dock_bounds(0, 0, 800, 0, 0, 1920, 1080, 420, 2000);
        assert_eq!(bounds, (800, 0, 420, 1080));
    }

    #[test]
    fn enforces_minimum_board_width() {
        let bounds = board_dock_bounds(0, 0, 800, 0, 0, 1920, 1080, 100, 800);
        assert_eq!(bounds.2, 300);
    }
}
