use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::RwLock,
    time::{SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

fn default_color_mode() -> String {
    "system".into()
}

fn default_app_font_name() -> String {
    String::new()
}

fn default_app_font_size() -> f32 {
    13.0
}

fn default_session_font_name() -> String {
    String::new()
}

fn default_session_font_size() -> f32 {
    13.0
}

fn default_code_font() -> String {
    "cascadia".into()
}

fn default_close_behavior() -> String {
    "ask".into()
}

fn default_terminal_shell() -> String {
    if cfg!(windows) {
        "powershell".into()
    } else {
        "bash".into()
    }
}

fn default_chat_detail_level() -> String {
    "standard".into()
}

/// AI 对话内容显示详细程度：只允许 concise / standard / verbose，其余收敛为默认值。
fn deserialize_chat_detail_level<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    Ok(
        if matches!(value.as_str(), "concise" | "standard" | "verbose") {
            value
        } else {
            default_chat_detail_level()
        },
    )
}

fn default_pi_environment() -> String {
    "managed".into()
}

fn default_proxy_mode() -> String {
    "direct".into()
}

fn default_notify_on_task_complete() -> bool {
    true
}

fn default_pet_enabled() -> bool {
    true
}

fn default_pet_always_on_top() -> bool {
    true
}

/// 未知代理模式回落 system（旧设置文件与手改配置容错）。
fn deserialize_proxy_mode<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    Ok(crate::proxy::normalize_mode(&value).to_owned())
}

fn deserialize_pi_environment<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    Ok(if matches!(value.as_str(), "auto" | "native") {
        default_pi_environment()
    } else {
        value
    })
}

fn deserialize_terminal_shell<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    Ok(
        if matches!(value.as_str(), "powershell" | "pwsh" | "bash" | "cmd") {
            value
        } else {
            default_terminal_shell()
        },
    )
}

/// 导入主题数量上限，避免设置文件被无限撑大。
const MAX_CUSTOM_THEMES: usize = 32;

/// 默认界面语言；托盘等原生 UI 在设置尚未加载时也用它兑底。
pub const DEFAULT_LANGUAGE: &str = "zh-CN";

fn default_language() -> String {
    DEFAULT_LANGUAGE.into()
}

/// 语言校验：只接受受支持的三个语言标识，其余收敛为默认值。
fn deserialize_language<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    Ok(if matches!(value.as_str(), "zh-CN" | "zh-TW" | "en") {
        value
    } else {
        default_language()
    })
}

fn default_theme() -> String {
    "command-flow".into()
}

/// 主题 id 只允许字母、数字、短横线与下划线，长度不超过 64。
fn is_valid_theme_id(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 64
        && trimmed.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

/// 主题 id 校验：支持内置主题与用户导入的主题，因此不再把未知 id 收敛为默认值。
fn deserialize_theme<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if is_valid_theme_id(&value) {
        Ok(value.trim().to_string())
    } else {
        Ok(default_theme())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub schema_version: u32,
    pub max_concurrent_tasks: u8,
    pub last_project: Option<String>,
    #[serde(default)]
    pub skipped_updates: BTreeMap<String, String>,
    #[serde(default)]
    pub snoozed_updates: BTreeMap<String, u64>,
    #[serde(default = "default_color_mode")]
    pub color_mode: String,
    #[serde(default = "default_theme", deserialize_with = "deserialize_theme")]
    pub theme: String,
    /// 应用程序字体（本机字体族名）；空串表示使用默认字体栈。
    #[serde(default = "default_app_font_name")]
    pub app_font_name: String,
    #[serde(default = "default_app_font_size")]
    pub app_font_size: f32,
    /// 会话窗口字体（对话/终端）；空串表示使用默认字体栈。
    #[serde(default = "default_session_font_name")]
    pub session_font_name: String,
    #[serde(default = "default_session_font_size")]
    pub session_font_size: f32,
    #[serde(default = "default_code_font")]
    pub code_font: String,
    #[serde(default = "default_close_behavior")]
    pub close_behavior: String,
    #[serde(default)]
    pub external_editor: Option<crate::external_editor::ExternalEditor>,
    #[serde(
        default = "default_terminal_shell",
        deserialize_with = "deserialize_terminal_shell"
    )]
    pub terminal_shell: String,
    #[serde(
        default = "default_pi_environment",
        deserialize_with = "deserialize_pi_environment"
    )]
    pub pi_environment: String,
    /// AI 对话内容的显示详细程度：concise / standard / verbose。
    #[serde(
        default = "default_chat_detail_level",
        deserialize_with = "deserialize_chat_detail_level"
    )]
    pub chat_detail_level: String,
    /// 界面语言：zh-CN / zh-TW / en。
    #[serde(
        default = "default_language",
        deserialize_with = "deserialize_language"
    )]
    pub language: String,
    /// 用户导入的主题包（内置主题之外），原样保存 JSON 以便未来字段兼容。
    #[serde(default)]
    pub custom_themes: Vec<serde_json::Value>,
    /// 全局网络代理：system（跟随系统/环境变量）/ direct（直连）/ manual（手动地址）。
    #[serde(
        default = "default_proxy_mode",
        deserialize_with = "deserialize_proxy_mode"
    )]
    pub proxy_mode: String,
    /// 手动模式的代理地址，如 `http://127.0.0.1:7890`。
    #[serde(default)]
    pub proxy_url: String,
    /// 不走代理的地址列表（NO_PROXY，逗号分隔）。
    #[serde(default)]
    pub proxy_no_proxy: String,
    /// 任务完成/失败时发送系统通知（应用在后台也会提示）。
    #[serde(default = "default_notify_on_task_complete")]
    pub notify_on_task_complete: bool,
    /// 启动时自动显示桌宠浮窗。
    #[serde(default = "default_pet_enabled")]
    pub pet_enabled: bool,
    /// 桌宠浮窗是否置顶（显示在最上方；关闭后可被其他窗口遮挡）。
    #[serde(default = "default_pet_always_on_top")]
    pub pet_always_on_top: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            max_concurrent_tasks: 10,
            last_project: None,
            skipped_updates: std::collections::BTreeMap::new(),
            snoozed_updates: std::collections::BTreeMap::new(),
            color_mode: default_color_mode(),
            theme: default_theme(),
            app_font_name: default_app_font_name(),
            app_font_size: default_app_font_size(),
            session_font_name: default_session_font_name(),
            session_font_size: default_session_font_size(),
            code_font: default_code_font(),
            close_behavior: default_close_behavior(),
            external_editor: None,
            terminal_shell: default_terminal_shell(),
            pi_environment: default_pi_environment(),
            chat_detail_level: default_chat_detail_level(),
            custom_themes: Vec::new(),
            language: default_language(),
            proxy_mode: default_proxy_mode(),
            proxy_url: String::new(),
            proxy_no_proxy: String::new(),
            notify_on_task_complete: default_notify_on_task_complete(),
            pet_enabled: default_pet_enabled(),
            pet_always_on_top: default_pet_always_on_top(),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    backups: PathBuf,
    settings: RwLock<AppSettings>,
}

/// 把旧版 settings.json 字段迁移到当前结构：
/// - `appFont`/`textFont`（枚举）→ `appFontName`/`sessionFontName`（具体字体族名）；
/// - 早期内置主题名 `theme: "winxp"` → 当前默认主题。
pub(crate) fn migrate_legacy_settings(mut value: Value) -> Value {
    if let Some(name) = legacy_font_name(&value, "appFont") {
        value["appFontName"] = Value::String(name);
    }
    if let Some(name) = legacy_font_name(&value, "textFont") {
        value["sessionFontName"] = Value::String(name);
    }
    // 早期内置主题名已不存在，统一迁移到当前默认主题。
    if value.get("theme").and_then(Value::as_str) == Some("winxp") {
        value["theme"] = Value::String(default_theme());
    }
    value
}

/// 旧版字体枚举 → 具体字体族名；"system" 与未知值回落默认字体栈。
fn legacy_font_name(value: &Value, key: &str) -> Option<String> {
    match value.get(key).and_then(Value::as_str)? {
        "segoe" => Some("Segoe UI".into()),
        "yahei" => Some("Microsoft YaHei UI".into()),
        "inter" => Some("Inter".into()),
        _ => None,
    }
}
impl SettingsStore {
    pub fn open(path: PathBuf, backups: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create settings directory: {error}"))?;
        }
        fs::create_dir_all(&backups)
            .map_err(|error| format!("failed to create backup directory: {error}"))?;
        let settings = if path.is_file() {
            let content = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read settings: {error}"))?;
            let migrated = migrate_legacy_settings(
                serde_json::from_str(&content)
                    .map_err(|error| format!("settings.json is invalid: {error}"))?,
            );
            serde_json::from_value(migrated)
                .map_err(|error| format!("settings.json is invalid: {error}"))?
        } else {
            let settings = AppSettings::default();
            write_atomic(&path, &settings)?;
            settings
        };
        validate(&settings)?;
        Ok(Self {
            path,
            backups,
            settings: RwLock::new(settings),
        })
    }

    pub fn get(&self) -> Result<AppSettings, String> {
        self.settings
            .read()
            .map(|settings| settings.clone())
            .map_err(|_| "settings lock is poisoned".into())
    }

    pub fn save(&self, mut settings: AppSettings) -> Result<(), String> {
        if matches!(settings.pi_environment.as_str(), "auto" | "native") {
            settings.pi_environment = default_pi_environment();
        }
        if !is_valid_theme_id(&settings.theme) {
            settings.theme = default_theme();
        }
        // 导入主题数量设上限，避免设置文件被无限撑大。
        settings.custom_themes.truncate(MAX_CUSTOM_THEMES);
        let mut current = self
            .settings
            .write()
            .map_err(|_| "settings lock is poisoned")?;
        settings.external_editor = current.external_editor.clone();
        self.persist(&settings)?;
        *current = settings;
        Ok(())
    }

    pub fn set_external_editor(
        &self,
        editor: Option<crate::external_editor::ExternalEditor>,
    ) -> Result<(), String> {
        let mut current = self
            .settings
            .write()
            .map_err(|_| "settings lock is poisoned")?;
        let mut next = current.clone();
        next.external_editor = editor;
        self.persist(&next)?;
        *current = next;
        Ok(())
    }

    fn persist(&self, settings: &AppSettings) -> Result<(), String> {
        validate(settings)?;
        if self.path.is_file() {
            let backup = self.backups.join(format!(
                "settings-{}-{}.json",
                now_millis()?,
                std::process::id()
            ));
            fs::copy(&self.path, backup)
                .map_err(|error| format!("failed to back up settings: {error}"))?;
        }
        write_atomic(&self.path, settings)?;
        Ok(())
    }
}

fn validate(settings: &AppSettings) -> Result<(), String> {
    if settings.pi_environment != "managed" {
        return Err("piEnvironment must be managed".into());
    }
    if !matches!(settings.language.as_str(), "zh-CN" | "zh-TW" | "en") {
        return Err("language must be zh-CN, zh-TW, or en".into());
    }
    if !matches!(
        settings.terminal_shell.as_str(),
        "powershell" | "pwsh" | "bash" | "cmd"
    ) {
        return Err("terminalShell is invalid".into());
    }
    if !matches!(
        settings.chat_detail_level.as_str(),
        "concise" | "standard" | "verbose"
    ) {
        return Err("chatDetailLevel is invalid".into());
    }
    if let Some(editor) = &settings.external_editor {
        editor.validate()?;
    }
    crate::proxy::validate(settings)?;
    if settings.schema_version != 1 {
        return Err(format!(
            "unsupported settings schema: {}",
            settings.schema_version
        ));
    }
    if !(1..=16).contains(&settings.max_concurrent_tasks) {
        return Err("maxConcurrentTasks must be between 1 and 16".into());
    }
    if settings
        .last_project
        .as_ref()
        .is_some_and(|path| path.trim().is_empty() || path.len() > 4_096)
    {
        return Err("lastProject must be a non-empty path".into());
    }
    if !matches!(settings.color_mode.as_str(), "system" | "light" | "dark") {
        return Err("colorMode must be system, light, or dark".into());
    }
    if !is_valid_theme_id(&settings.theme) {
        return Err("theme must be a valid theme id".into());
    }
    if settings.custom_themes.len() > MAX_CUSTOM_THEMES {
        return Err(format!(
            "customThemes must contain at most {MAX_CUSTOM_THEMES} themes"
        ));
    }
    for (name, label) in [
        (&settings.app_font_name, "appFontName"),
        (&settings.session_font_name, "sessionFontName"),
        (&settings.code_font, "codeFont"),
    ] {
        if name.len() > 128
            || name
                .chars()
                .any(|c| c.is_control() || c == '"' || c == '\\')
        {
            return Err(format!("{label} is invalid"));
        }
    }
    for (size, label) in [
        (settings.app_font_size, "appFontSize"),
        (settings.session_font_size, "sessionFontSize"),
    ] {
        if !size.is_finite() || !(9.0..=32.0).contains(&size) {
            return Err(format!("{label} must be between 9 and 32"));
        }
    }
    if !matches!(
        settings.close_behavior.as_str(),
        "ask" | "minimize" | "exit"
    ) {
        return Err("closeBehavior is invalid".into());
    }
    if settings.skipped_updates.len() > 64 || settings.snoozed_updates.len() > 64 {
        return Err("too many update preferences".into());
    }
    if settings.skipped_updates.iter().any(|(component, version)| {
        component.is_empty()
            || component.len() > 64
            || version.is_empty()
            || version.len() > 64
            || !version.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
            })
    }) {
        return Err("skipped update preference is invalid".into());
    }
    if settings
        .snoozed_updates
        .keys()
        .any(|component| component.is_empty() || component.len() > 64)
    {
        return Err("snoozed update preference is invalid".into());
    }
    Ok(())
}

fn write_atomic(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("failed to serialize settings: {error}"))?;
    let mut file = AtomicWriteFile::open(path)
        .map_err(|error| format!("failed to open atomic settings file: {error}"))?;
    file.write_all(&content)
        .map_err(|error| format!("failed to write settings: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to commit settings: {error}"))
}

fn now_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<SettingsStore>().get())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<SettingsStore>();
        store.save(settings)?;
        // 保存成功后立即应用：新启动的任务/会话就用新代理（已运行的会话需重启任务）。
        let saved = store.get()?;
        crate::proxy::configure(&saved);
        // 桌宠置顶设置即时生效到已打开的浮窗（未打开则无操作）。
        crate::pet::apply_always_on_top(&app, saved.pet_always_on_top);
        // 界面语言即时生效到托盘菜单（原生 UI，须在主线程更新）。
        let language = saved.language;
        let tray_app = app.clone();
        let _ = app.run_on_main_thread(move || crate::tray::apply_language(&tray_app, &language));
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::{default_theme, migrate_legacy_settings, AppSettings, SettingsStore};

    #[test]
    fn loads_old_settings_with_new_defaults() {
        let settings: AppSettings = serde_json::from_str(
            r#"{"schemaVersion":1,"maxConcurrentTasks":3,"lastProject":null}"#,
        )
        .expect("old settings should load");

        assert_eq!(settings.color_mode, "system");
        assert_eq!(settings.code_font, "cascadia");
        assert_eq!(settings.close_behavior, "ask");
        assert_eq!(settings.pi_environment, "managed");
        assert!(settings.external_editor.is_none());
        assert!(settings.skipped_updates.is_empty());
        assert!(settings.snoozed_updates.is_empty());
        assert_eq!(settings.app_font_name, "");
        assert_eq!(settings.app_font_size, 13.0);
        assert_eq!(settings.session_font_name, "");
        assert_eq!(settings.session_font_size, 13.0);
    }

    #[test]
    fn legacy_environments_are_migrated_to_managed() {
        for stored in ["auto", "native", "managed"] {
            let value = serde_json::json!({
                "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
                "piEnvironment": stored,
            });
            let settings: AppSettings = serde_json::from_value(value).unwrap();
            assert_eq!(settings.pi_environment, "managed");
        }
    }

    #[test]
    /// chatDetailLevel：缺省回落 standard，未知值收敛，合法值原样保留。
    fn chat_detail_level_defaults_and_rejects_unknown_values() {
        let base = serde_json::json!({
            "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
        });
        let settings: AppSettings = serde_json::from_value(base).unwrap();
        assert_eq!(settings.chat_detail_level, "standard");

        for stored in ["concise", "standard", "verbose"] {
            let value = serde_json::json!({
                "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
                "chatDetailLevel": stored,
            });
            let settings: AppSettings = serde_json::from_value(value).unwrap();
            assert_eq!(settings.chat_detail_level, stored);
        }

        let value = serde_json::json!({
            "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
            "chatDetailLevel": "ultra",
        });
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.chat_detail_level, "standard");
    }

    #[test]
    fn language_defaults_and_rejects_unknown_values() {
        let base = serde_json::json!({
            "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
        });
        let settings: AppSettings = serde_json::from_value(base).unwrap();
        assert_eq!(settings.language, "zh-CN");

        for stored in ["zh-CN", "zh-TW", "en"] {
            let value = serde_json::json!({
                "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
                "language": stored,
            });
            let settings: AppSettings = serde_json::from_value(value).unwrap();
            assert_eq!(settings.language, stored);
        }

        let value = serde_json::json!({
            "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
            "language": "fr",
        });
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.language, "zh-CN");
    }

    #[test]
    fn legacy_fonts_and_theme_are_migrated() {
        let value = serde_json::json!({
            "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
            "appFont": "yahei", "textFont": "inter", "theme": "winxp",
        });
        let settings: AppSettings = serde_json::from_value(migrate_legacy_settings(value)).unwrap();
        assert_eq!(settings.app_font_name, "Microsoft YaHei UI");
        assert_eq!(settings.session_font_name, "Inter");
        assert_eq!(settings.theme, default_theme());
    }

    /// 内置主题被移除后，存量配置里的旧 id 依然合法（只校验格式），由前端 resolveTheme 回落默认主题。
    #[test]
    fn stored_win11_theme_id_is_accepted_and_kept() {
        let value = serde_json::json!({
            "schemaVersion": 1, "maxConcurrentTasks": 3, "lastProject": null,
            "theme": "win11",
        });
        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.theme, "win11");
        assert!(super::validate(&settings).is_ok());
    }

    #[test]
    fn unknown_font_names_are_rejected() {
        let settings = AppSettings {
            app_font_name: "bad\"name".into(),
            ..AppSettings::default()
        };
        assert!(super::validate(&settings).is_err());
        let settings = AppSettings {
            session_font_size: 40.0,
            ..AppSettings::default()
        };
        assert!(super::validate(&settings).is_err());
    }

    #[test]
    fn saving_legacy_auto_never_reenables_native_detection() {
        let root = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(
            root.path().join("settings.json"),
            root.path().join("backups"),
        )
        .unwrap();
        let mut settings = store.get().unwrap();
        for environment in ["auto", "native"] {
            settings.pi_environment = environment.into();
            store.save(settings.clone()).unwrap();
            assert_eq!(store.get().unwrap().pi_environment, "managed");
        }
        assert_eq!(store.get().unwrap().pi_environment, "managed");
        let reopened = SettingsStore::open(
            root.path().join("settings.json"),
            root.path().join("backups"),
        )
        .unwrap();
        assert_eq!(reopened.get().unwrap().pi_environment, "managed");
    }

    /// 代理设置持久化与校验：手动模式必须有合法地址；未知模式回落 system。
    #[test]
    fn proxy_settings_persist_and_validate() {
        let root =
            std::env::temp_dir().join(format!("deeppi-proxy-settings-{}", uuid::Uuid::new_v4()));
        let path = root.join("settings.json");
        let backups = root.join("backups");
        let store = SettingsStore::open(path.clone(), backups.clone()).unwrap();
        assert_eq!(store.get().unwrap().proxy_mode, "direct");

        // 手动模式缺地址：保存被拒绝，不会写入无效配置。
        assert!(store
            .save(AppSettings {
                proxy_mode: "manual".into(),
                ..AppSettings::default()
            })
            .is_err());

        store
            .save(AppSettings {
                proxy_mode: "manual".into(),
                proxy_url: "http://127.0.0.1:7890".into(),
                proxy_no_proxy: "localhost".into(),
                ..AppSettings::default()
            })
            .expect("manual proxy settings should save");

        let reopened = SettingsStore::open(path.clone(), backups.clone()).unwrap();
        let saved = reopened.get().unwrap();
        assert_eq!(saved.proxy_mode, "manual");
        assert_eq!(saved.proxy_url, "http://127.0.0.1:7890");
        assert_eq!(saved.proxy_no_proxy, "localhost");

        // 旧版/手改配置里的未知模式：加载时归一化为 system。
        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        value["proxyMode"] = serde_json::Value::String("bogus".into());
        std::fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
        let normalized = SettingsStore::open(path, backups).unwrap();
        assert_eq!(normalized.get().unwrap().proxy_mode, "system");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn editor_configuration_survives_a_stale_appearance_save() {
        use crate::external_editor::{EditorKind, ExternalEditor};
        let root =
            std::env::temp_dir().join(format!("deeppi-editor-settings-{}", uuid::Uuid::new_v4()));
        let store = SettingsStore::open(root.join("settings.json"), root.join("backups")).unwrap();
        let mut stale = store.get().unwrap();
        let editor = ExternalEditor {
            kind: EditorKind::Vscode,
            executable: "C:\\Tools\\Code.exe".into(),
        };
        store.set_external_editor(Some(editor.clone())).unwrap();
        stale.color_mode = "light".into();
        store.save(stale).unwrap();
        assert_eq!(store.get().unwrap().external_editor, Some(editor));
        assert_eq!(store.get().unwrap().color_mode, "light");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn creates_updates_and_backups_settings() {
        let root =
            std::env::temp_dir().join(format!("deeppi-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("settings.json");
        let backups = root.join("backups");

        let store =
            SettingsStore::open(path.clone(), backups.clone()).expect("settings store should open");
        assert_eq!(
            store
                .get()
                .expect("settings should read")
                .max_concurrent_tasks,
            10
        );

        store
            .save(AppSettings {
                schema_version: 1,
                max_concurrent_tasks: 5,
                last_project: Some("F:/project".into()),
                ..AppSettings::default()
            })
            .expect("settings should save");

        let reopened = SettingsStore::open(path, backups.clone()).expect("settings should reopen");
        assert_eq!(
            reopened
                .get()
                .expect("settings should read")
                .max_concurrent_tasks,
            5
        );
        assert_eq!(
            std::fs::read_dir(backups)
                .expect("backups should list")
                .count(),
            1
        );

        std::fs::remove_dir_all(root).expect("test paths should be removed");
    }
}
