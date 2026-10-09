//! 系统托盘（Windows 通知区域图标）。
//!
//! 托盘是原生 UI，不走前端的消息码渲染：菜单文案按用户设置的语言在这里渲染，
//! 语言变化时由 `apply_language` 在主线程重建菜单。托盘「退出」不直接结束进程，
//! 而是复用前端既有的关闭流程（活动任务确认、设置落盘、文件编辑锁、任务清理）。

use std::sync::Mutex;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const TRAY_ID: &str = "deeppi-tray";
const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";
const PI_UPDATE_ID: &str = "pi-update";
const DSH_UPDATE_ID: &str = "dsh-update";

struct TrayView {
    language: String,
    updates: Vec<(String, String)>,
}

struct TrayState(Mutex<TrayView>);

/// 托盘菜单文案；zh-CN 是设置里的默认语言，作为兜底。
fn menu_labels(language: &str) -> (&'static str, &'static str) {
    match language {
        "zh-TW" => ("顯示主視窗", "結束 DeepPi"),
        "en" => ("Show Main Window", "Quit DeepPi"),
        _ => ("显示主窗口", "退出 DeepPi"),
    }
}

fn update_label(language: &str, component: &str, version: &str) -> String {
    let name = if component == "pi" { "Pi" } else { "DSH" };
    match language {
        "zh-TW" => format!("{name} {version} 尚未驗證 · 開啟設定"),
        "en" => format!("{name} {version} not yet verified · Open Settings"),
        _ => format!("{name} {version} 尚未验证 · 打开设置"),
    }
}

fn build_menu(app: &AppHandle, view: &TrayView) -> tauri::Result<Menu<tauri::Wry>> {
    let (show, quit) = menu_labels(&view.language);
    let show_item = MenuItem::with_id(app, SHOW_ID, show, true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT_ID, quit, true, None::<&str>)?;
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&show_item];
    let update_items: Vec<_> = view
        .updates
        .iter()
        .map(|(id, version)| {
            MenuItem::with_id(
                app,
                format!("{id}-update"),
                update_label(&view.language, id, version),
                true,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    for item in &update_items {
        items.push(item);
    }
    items.push(&quit_item);
    Menu::with_items(app, &items)
}

fn badged_icon(icon: &Image<'_>) -> Option<Image<'static>> {
    let (width, height) = (icon.width(), icon.height());
    if width < 8 || height < 8 {
        return None;
    }
    let mut pixels = icon.rgba().to_vec();
    let radius = (width.min(height) / 5) as i64;
    let (cx, cy) = (width as i64 - radius - 1, height as i64 - radius - 1);
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let distance = (x - cx).pow(2) + (y - cy).pow(2);
            if distance > (radius + 1).pow(2) {
                continue;
            }
            let offset = ((y as u32 * width + x as u32) * 4) as usize;
            if offset + 4 > pixels.len() {
                return None;
            }
            pixels[offset..offset + 4].copy_from_slice(if distance <= radius.pow(2) {
                &[220, 55, 45, 255]
            } else {
                &[255, 255, 255, 255]
            });
        }
    }
    Some(Image::new_owned(pixels, width, height))
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
    let view = TrayView {
        language: crate::settings::DEFAULT_LANGUAGE.into(),
        updates: Vec::new(),
    };
    let menu = build_menu(app, &view)?;
    app.manage(TrayState(Mutex::new(view)));
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
            PI_UPDATE_ID | DSH_UPDATE_ID => {
                show_main_window(app);
                let category = if event.id().as_ref() == PI_UPDATE_ID {
                    "pi"
                } else {
                    "dsh"
                };
                let _ = app.emit_to("main", "tray-runtime-update", category);
            }
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

fn refresh(app: &AppHandle, view: &TrayView) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_menu(app, view) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                log::warn!("event=tray_menu_apply status=failed error={error}");
            }
        }
        Err(error) => log::warn!("event=tray_menu_apply status=failed error={error}"),
    }
    let icon = app.default_window_icon().and_then(|icon| {
        if view.updates.is_empty() {
            Some(icon.clone())
        } else {
            badged_icon(icon)
        }
    });
    if let Err(error) = tray.set_icon(icon) {
        log::warn!("event=tray_icon_apply status=failed error={error}");
    }
    let tooltip = if view.updates.is_empty() {
        "DeepPi".into()
    } else {
        format!(
            "DeepPi · {}",
            view.updates
                .iter()
                .map(|(id, version)| update_label(&view.language, id, version))
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    if let Err(error) = tray.set_tooltip(Some(tooltip)) {
        log::warn!("event=tray_tooltip_apply status=failed error={error}");
    }
}

pub fn apply_language(app: &AppHandle, language: &str) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if let Ok(mut view) = state.0.lock() {
        view.language = language.into();
        refresh(app, &view);
    };
}

pub fn apply_runtime_updates(app: &AppHandle, updates: Vec<(String, String)>) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if let Ok(mut view) = state.0.lock() {
        view.updates = updates;
        refresh(app, &view);
    };
}

#[cfg(test)]
mod tests {
    use super::{badged_icon, menu_labels, update_label};

    #[test]
    fn menu_labels_cover_supported_languages() {
        assert_eq!(menu_labels("zh-CN"), ("显示主窗口", "退出 DeepPi"));
        assert_eq!(menu_labels("zh-TW"), ("顯示主視窗", "結束 DeepPi"));
        assert_eq!(menu_labels("en"), ("Show Main Window", "Quit DeepPi"));
        // 未知语言收敛到默认语言，与设置反序列化的兜底行为一致。
        assert_eq!(menu_labels("fr"), ("显示主窗口", "退出 DeepPi"));
    }
    #[test]
    fn update_prompt_is_translated_and_badge_changes_only_corner() {
        assert!(update_label("en", "dsh", "0.2.0").contains("not yet verified"));
        assert!(update_label("zh-CN", "pi", "1.0.0").contains("尚未验证"));
        let pixels = vec![0u8; 16 * 16 * 4];
        let image = tauri::image::Image::new(&pixels, 16, 16);
        let badged = badged_icon(&image).unwrap();
        assert_eq!(&badged.rgba()[..4], &[0, 0, 0, 0]);
        assert!(badged
            .rgba()
            .chunks(4)
            .any(|pixel| pixel == [220, 55, 45, 255]));
    }
}
