//! 枚举本机已安装字体，供设置页的字体选择器使用。

#[tauri::command]
pub async fn list_system_fonts() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(list_system_fonts_inner)
        .await
        .map_err(|error| format!("font enumeration worker failed: {error}"))?
}

/// 从 Windows 注册表枚举已安装字体族名（系统级 + 当前用户），
/// 去重并按不区分大小写的顺序排序。
#[cfg(windows)]
fn list_system_fonts_inner() -> Result<Vec<String>, String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    const FONT_PATH: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";
    let mut fonts = Vec::new();
    for root in [
        winreg::RegKey::predef(HKEY_LOCAL_MACHINE),
        winreg::RegKey::predef(HKEY_CURRENT_USER),
    ] {
        let key = match root.open_subkey(FONT_PATH) {
            Ok(key) => key,
            // 用户级字体键可能不存在，忽略即可。
            Err(_) => continue,
        };
        for value in key.enum_values().flatten() {
            fonts.extend(font_families_from_registry_entry(&value.0));
        }
    }
    fonts.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    fonts.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Ok(fonts)
}

/// `"Arial (TrueType)"` → `["Arial"]`；
/// 组合条目（`"A & B (TrueType)"`）拆成 `["A", "B"]`。
fn font_families_from_registry_entry(entry: &str) -> Vec<String> {
    let name = match (entry.find('('), entry.rfind(')')) {
        (Some(open), Some(close)) if close > open => entry[..open].trim(),
        _ => entry.trim(),
    };
    if name.is_empty() || name.len() > 200 {
        return Vec::new();
    }
    if name.chars().any(|c| c.is_control()) {
        return Vec::new();
    }
    name.split(" & ")
        .map(str::trim)
        .filter(|family| !family.is_empty() && family.len() <= 128)
        .map(str::to_owned)
        .collect()
}

#[cfg(not(windows))]
fn list_system_fonts_inner() -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::font_families_from_registry_entry;

    #[test]
    fn strips_font_type_suffix() {
        assert_eq!(
            font_families_from_registry_entry("Arial (TrueType)"),
            vec!["Arial"]
        );
        assert_eq!(
            font_families_from_registry_entry("Segoe UI Variable (OpenType)"),
            vec!["Segoe UI Variable"]
        );
    }

    #[test]
    fn splits_combined_family_entries() {
        assert_eq!(
            font_families_from_registry_entry("Microsoft YaHei & Microsoft YaHei UI (TrueType)"),
            vec!["Microsoft YaHei", "Microsoft YaHei UI"]
        );
    }

    #[test]
    fn rejects_unusable_entries() {
        assert!(font_families_from_registry_entry("").is_empty());
        assert!(font_families_from_registry_entry("(TrueType)").is_empty());
        assert!(font_families_from_registry_entry("bad\u{7}name (TrueType)").is_empty());
        assert!(font_families_from_registry_entry("Not a font value name with no parens but very long indeed over one hundred twenty eight characters aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_empty());
    }
}
