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
use tauri::{AppHandle, Manager};

fn default_color_mode() -> String {
    "system".into()
}

fn default_app_font() -> String {
    "system".into()
}

fn default_text_font() -> String {
    "system".into()
}

fn default_code_font() -> String {
    "cascadia".into()
}

fn default_close_behavior() -> String {
    "ask".into()
}

fn default_pi_environment() -> String {
    "managed".into()
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
    #[serde(default = "default_app_font")]
    pub app_font: String,
    #[serde(default = "default_text_font")]
    pub text_font: String,
    #[serde(default = "default_code_font")]
    pub code_font: String,
    #[serde(default = "default_close_behavior")]
    pub close_behavior: String,
    #[serde(default)]
    pub external_editor: Option<crate::external_editor::ExternalEditor>,
    #[serde(
        default = "default_pi_environment",
        deserialize_with = "deserialize_pi_environment"
    )]
    pub pi_environment: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            max_concurrent_tasks: 3,
            last_project: None,
            skipped_updates: BTreeMap::new(),
            snoozed_updates: BTreeMap::new(),
            color_mode: default_color_mode(),
            app_font: default_app_font(),
            text_font: default_text_font(),
            code_font: default_code_font(),
            close_behavior: default_close_behavior(),
            external_editor: None,
            pi_environment: default_pi_environment(),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    backups: PathBuf,
    settings: RwLock<AppSettings>,
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
            serde_json::from_str(&content)
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
    if let Some(editor) = &settings.external_editor {
        editor.validate()?;
    }
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
    if !matches!(
        settings.app_font.as_str(),
        "system" | "segoe" | "yahei" | "inter"
    ) {
        return Err("appFont is invalid".into());
    }
    if !matches!(
        settings.text_font.as_str(),
        "system" | "segoe" | "yahei" | "inter"
    ) {
        return Err("textFont is invalid".into());
    }
    if !matches!(
        settings.code_font.as_str(),
        "cascadia" | "consolas" | "jetbrains"
    ) {
        return Err("codeFont is invalid".into());
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
    tauri::async_runtime::spawn_blocking(move || app.state::<SettingsStore>().save(settings))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::{AppSettings, SettingsStore};

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
            3
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
