//! 主题包（Theme Pack）文件导入 / 导出。
//!
//! 主题文件是纯 JSON（`.deeppi-theme.json`），只包含数据，不含任何可执行内容。
//! 前端负责校验与落地；本模块只做「选路径 + 读写文件」，并限制文件大小与路径形态。

use std::fs;
use std::path::Path;

use crate::message::{msg, msg_with};
use serde_json::Value;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// 主题文件大小上限：正常主题在 2 KB 量级，留出充足余量同时拒绝异常大文件。
const MAX_THEME_BYTES: u64 = 256 * 1024;

fn require_main(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() == "main" {
        Ok(())
    } else {
        Err(msg("theme.main_window_only"))
    }
}

/// 校验导出/导入使用的是完整普通文件路径，拒绝相对路径与非法文件名。
fn ensure_plain_file_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .file_name()
            .is_none_or(|name| name.to_string_lossy().contains(':'))
    {
        return Err(msg("theme.path_required"));
    }
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => {
                    if !matches!(prefix.kind(), Prefix::Disk(_)) {
                        return Err(msg("theme.path_local_disk_only"));
                    }
                }
                Component::ParentDir => return Err(msg("theme.path_parent_dir")),
                _ => {}
            }
        }
    }
    Ok(())
}

/// 导出主题包到用户选择的文件。
///
/// 返回 `Ok(false)` 表示用户在对话框里取消。
#[tauri::command]
pub async fn theme_export(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    theme: Value,
    suggested_name: String,
    // 对话框标题与过滤器名由前端传入（前端掌握当前语言），后端不持有任何文案。
    dialog_title: String,
    filter_name: String,
) -> Result<bool, String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        if !theme.is_object() {
            return Err(msg("theme.export_not_object"));
        }
        let mut bytes = serde_json::to_vec_pretty(&theme).map_err(|error| {
            msg_with("theme.serialize_failed", &[("error", &error.to_string())])
        })?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_THEME_BYTES {
            return Err(msg("theme.export_too_large"));
        }
        let window = app
            .get_webview_window("main")
            .ok_or(msg("app.main_window_closed"))?;
        let file_name = safe_file_name(&suggested_name);
        let Some(file) = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(dialog_title)
            .set_file_name(file_name)
            .add_filter(filter_name, &["json"])
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file
            .into_path()
            .map_err(|_| msg("theme.export_location_unsupported"))?;
        ensure_plain_file_path(&path)?;
        fs::write(&path, bytes)
            .map_err(|error| msg_with("theme.write_failed", &[("error", &error.to_string())]))?;
        Ok(true)
    })
    .await
    .map_err(|error| msg_with("theme.export_failed", &[("error", &error.to_string())]))?
}

/// 从用户选择的文件读取主题文本。
///
/// 返回 `Ok(None)` 表示取消；文本内容由前端做 schema 校验。
#[tauri::command]
pub async fn theme_import(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    dialog_title: String,
    filter_name: String,
) -> Result<Option<String>, String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let window = app
            .get_webview_window("main")
            .ok_or(msg("app.main_window_closed"))?;
        let Some(file) = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(dialog_title)
            .add_filter(filter_name, &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file
            .into_path()
            .map_err(|_| msg("theme.import_location_unsupported"))?;
        ensure_plain_file_path(&path)?;
        let metadata = fs::metadata(&path)
            .map_err(|error| msg_with("theme.file_unreadable", &[("error", &error.to_string())]))?;
        if !metadata.is_file() {
            return Err(msg("theme.path_not_a_file"));
        }
        if metadata.len() > MAX_THEME_BYTES {
            return Err(msg("theme.file_too_large"));
        }
        let text = fs::read_to_string(&path)
            .map_err(|error| msg_with("theme.read_failed", &[("error", &error.to_string())]))?;
        Ok(Some(text))
    })
    .await
    .map_err(|error| msg_with("theme.import_failed", &[("error", &error.to_string())]))?
}

/// 把建议文件名收敛成安全的单段文件名。
fn safe_file_name(suggested: &str) -> String {
    let cleaned: String = suggested
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "theme.deeppi-theme.json".into()
    } else {
        trimmed.chars().take(120).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::safe_file_name;

    #[test]
    fn strips_path_separators_from_suggested_names() {
        assert_eq!(
            safe_file_name("command-flow.deeppi-theme.json"),
            "command-flow.deeppi-theme.json"
        );
        assert_eq!(safe_file_name("../evil/name.json"), "evilname.json");
        assert_eq!(safe_file_name("C:\\Windows\\x.json"), "CWindowsx.json");
    }

    #[test]
    fn falls_back_when_the_name_is_unusable() {
        assert_eq!(safe_file_name(""), "theme.deeppi-theme.json");
        assert_eq!(safe_file_name("..."), "theme.deeppi-theme.json");
    }
}
