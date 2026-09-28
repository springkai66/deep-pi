//! 系统托盘（Windows 通知区域图标）。
//!
//! 托盘是原生 UI，不走前端的消息码渲染：菜单文案按用户设置的语言在这里渲染，
//! 语言变化时由 `apply_language` 在主线程重建菜单。托盘「退出」不直接结束进程，
//! 而是复用前端既有的关闭流程（活动任务确认、设置落盘、文件编辑锁、任务清理）。

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

/// 托盘实例 id：`apply_language` 按语言重建菜单时靠它查找。
const TRAY_ID: &str = "deeppi-tray";

/// 菜单项 id：显示主窗口 / 退出应用。
const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";

/// 托盘菜单文案；zh-CN 是设置里的默认语言，作为兜底。
fn menu_labels(language: &str) -> (&'static str, &'static str) {
    match language {
        "zh-TW" => ("顯示主視窗", "結束 DeepPi"),
        "en" => ("Show Main Window", "Quit DeepPi"),
        _ => ("显示主窗口", "退出 DeepPi"),
    }
}

fn build_menu(app: &AppHandle, language: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let (show, quit) = menu_labels(language);
    let show_item = MenuItem::with_id(app, SHOW_ID, show, true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT_ID, quit, true, None::<&str>)?;
    Menu::with_items(app, &[&show_item, &quit_item])
}

fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// 左键单击切换主窗口：可见时隐藏，否则恢复显示并聚焦。
fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let visible = window.is_visible().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    if visible && !minimized {
        let _ = window.hide();
    } else {
        show_main_window(app);
    }
}

/// 托盘菜单「退出」：把退出请求交给主窗口的前端，由既有关闭链路执行
/// （活动任务确认 → 设置落盘 → 文件编辑锁释放 → 停止任务 → 销毁窗口）。
fn request_quit(app: &AppHandle) {
    let _ = app.emit_to("main", "tray-quit", ());
}

/// 在主线程（setup 阶段）安装托盘。此时设置可能尚未加载完成，先用默认语言文案；
/// 设置加载完成后由 `apply_language` 更新。图标与应用图标同源（bundle icon）。
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, crate::settings::DEFAULT_LANGUAGE)?;
    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("bundle icon is required for the tray".into()))?
        .clone();
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("DeepPi")
        .menu(&menu)
        // 左键留给窗口切换，右键才弹菜单。
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            SHOW_ID => show_main_window(app),
            QUIT_ID => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// 按语言重建托盘菜单；托盘不存在（未安装或已移除）时静默跳过。
pub fn apply_language(app: &AppHandle, language: &str) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_menu(app, language) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                log::warn!("event=tray_menu_apply status=failed error={error}");
            }
        }
        Err(error) => log::warn!("event=tray_menu_apply status=failed error={error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::menu_labels;

    #[test]
    fn menu_labels_cover_supported_languages() {
        assert_eq!(menu_labels("zh-CN"), ("显示主窗口", "退出 DeepPi"));
        assert_eq!(menu_labels("zh-TW"), ("顯示主視窗", "結束 DeepPi"));
        assert_eq!(menu_labels("en"), ("Show Main Window", "Quit DeepPi"));
        // 未知语言收敛到默认语言，与设置反序列化的兜底行为一致。
        assert_eq!(menu_labels("fr"), ("显示主窗口", "退出 DeepPi"));
    }
}
