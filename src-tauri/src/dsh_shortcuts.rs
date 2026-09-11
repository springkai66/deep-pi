use tauri::{
    menu::{Menu, MenuId, MenuItemBuilder, MenuItemKind},
    AppHandle,
};

/// 宿主命令 id 与原生加速键的映射。
///
/// 只包含 host 作用域命令（任务搜索、文件搜索、设置）：它们在 DSH 子 Webview
/// 持有键盘焦点时仍然有意义，并且复用前端同一套 `hostCommandEnabled` /
/// `runHostCommand` 逻辑，不新增产品语义。
pub(crate) const DSH_SHORTCUTS: [(&str, &str); 3] = [
    ("tasks", "CmdOrCtrl+Shift+P"),
    ("files", "CmdOrCtrl+P"),
    ("settings", "CmdOrCtrl+,"),
];

const PREFIX: &str = "deeppi-shortcut:";

/// 菜单项 id → 宿主命令 id。非本模块注册的菜单项一律忽略。
pub(crate) fn command_from_menu_id(id: &str) -> Option<&'static str> {
    let command = id.strip_prefix(PREFIX)?;
    DSH_SHORTCUTS
        .iter()
        .find(|(name, _)| *name == command)
        .map(|(name, _)| *name)
}

fn item_id(name: &str) -> MenuId {
    MenuId::new(format!("{PREFIX}{name}"))
}

/// 启动时安装一个隐藏的、不带加速键的菜单。
///
/// 加速键在 DSH Webview 可见时按需启用、隐藏时清除；菜单本体常驻可以避免
/// 每次显示 DSH 都创建菜单造成的窗口重排或菜单栏闪烁。
pub fn install(app: &AppHandle) -> Result<(), String> {
    let items = DSH_SHORTCUTS
        .iter()
        .map(|(name, _)| {
            MenuItemBuilder::new(*name)
                .id(item_id(name))
                .build(app)
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let menu = Menu::with_items(app, &refs).map_err(|error| error.to_string())?;
    app.set_menu(menu).map_err(|error| error.to_string())?;
    app.hide_menu().map_err(|error| error.to_string())
}

fn set_accelerators(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let menu = app.menu().ok_or("快捷键菜单尚未安装")?;
    for (name, accelerator) in DSH_SHORTCUTS {
        let Some(MenuItemKind::MenuItem(item)) = menu.get(&item_id(name)) else {
            continue;
        };
        item.set_accelerator(if enabled { Some(accelerator) } else { None })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn enable(app: &AppHandle) -> Result<(), String> {
    set_accelerators(app, true)
}

pub fn disable(app: &AppHandle) {
    if let Err(error) = set_accelerators(app, false) {
        log::warn!("event=dsh_shortcuts status=disable_failed reason={error}");
    }
}

#[tauri::command]
pub fn set_dsh_shortcuts(
    webview: tauri::Webview,
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("DSH shortcuts require the main Webview".into());
    }
    if enabled {
        enable(&app)
    } else {
        disable(&app);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_only_registered_menu_ids() {
        assert_eq!(command_from_menu_id("deeppi-shortcut:tasks"), Some("tasks"));
        assert_eq!(command_from_menu_id("deeppi-shortcut:files"), Some("files"));
        assert_eq!(
            command_from_menu_id("deeppi-shortcut:settings"),
            Some("settings")
        );
        for id in [
            "deeppi-shortcut:sidebar",
            "deeppi-shortcut:composer",
            "deeppi-shortcut:save",
            "deeppi-shortcut:",
            "deeppi-shortcut:tasks-extra",
            "other",
        ] {
            assert_eq!(command_from_menu_id(id), None, "{id}");
        }
    }

    #[test]
    fn shortcut_definitions_are_unique_and_use_host_scope_commands() {
        let mut names: Vec<&str> = DSH_SHORTCUTS.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), DSH_SHORTCUTS.len());
        for (name, accelerator) in DSH_SHORTCUTS {
            assert!(matches!(name, "tasks" | "files" | "settings"));
            assert!(accelerator.starts_with("CmdOrCtrl+"));
        }
    }
}
