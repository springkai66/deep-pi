use std::{fs, path::PathBuf};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::State;

use crate::{
    agent_config::{self, ConfigScope},
    app_paths::AppPaths,
};

const DEFAULT_TOOLS: &[&str] = &["read", "bash", "edit", "write"];
const DEFAULT_INLINE_BUDGET: u32 = 3000;
const MAX_INLINE_BUDGET: u32 = 100_000;
const MAX_SETTINGS_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiCodemodeSaveRequest {
    pub scope: ConfigScope,
    #[serde(default)]
    pub project_path: Option<String>,
    pub enabled: bool,
    pub mode: String,
    pub inline_budget: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiCodemodeSettings {
    /// Whether codemode is present in Pi's resolved startup tool set.
    pub enabled: bool,
    pub mode: String,
    pub inline_budget: u32,
}

fn settings_path(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
) -> Result<PathBuf, String> {
    Ok(agent_config::scope_pi_dir(paths, scope, project_path)?.join("settings.json"))
}

fn read_settings(path: &std::path::Path) -> Result<Value, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Value::Object(Map::new()));
        }
        Err(error) => return Err(format!("failed to read Pi settings: {error}")),
    };
    if content.len() > MAX_SETTINGS_BYTES {
        return Err("Pi settings file exceeds the 1 MiB limit".into());
    }
    let value: Value = serde_json::from_str(&content)
        .map_err(|error| format!("Pi settings JSON is invalid: {error}"))?;
    if !value.is_object() {
        return Err("Pi settings must be a JSON object".into());
    }
    Ok(value)
}

fn string_entries(value: Option<&Value>) -> Option<Vec<String>> {
    value?.as_array().map(|entries| {
        entries
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    })
}

fn is_modifier(entry: &str) -> bool {
    entry.starts_with('+') || entry.starts_with('-')
}

/// Mirrors Pi's global/project merge and `defaultTools` modifier resolution.
fn resolved_default_tools(global: Option<&Value>, project: Option<&Value>) -> Vec<String> {
    let base = string_entries(global.and_then(|settings| settings.get("defaultTools")));
    let override_entries =
        string_entries(project.and_then(|settings| settings.get("defaultTools")));
    let entries = match (base, override_entries) {
        (base, None) => base.unwrap_or_default(),
        (Some(mut base), Some(overrides)) if overrides.iter().all(|entry| is_modifier(entry)) => {
            base.extend(overrides);
            base
        }
        (_, Some(overrides)) => overrides,
    };

    let plain = entries
        .iter()
        .filter(|entry| !is_modifier(entry))
        .cloned()
        .collect::<Vec<_>>();
    let mut tools = if plain.is_empty() {
        DEFAULT_TOOLS
            .iter()
            .map(|tool| (*tool).to_owned())
            .collect()
    } else {
        plain
    };
    for entry in entries.iter().filter(|entry| is_modifier(entry)) {
        let name = &entry[1..];
        if entry.starts_with('+') && !name.is_empty() && !tools.iter().any(|tool| tool == name) {
            tools.push(name.to_owned());
        } else if entry.starts_with('-') {
            tools.retain(|tool| tool != name);
        }
    }
    tools
}

fn string_setting(settings: &Value, key: &str) -> Option<String> {
    settings
        .get("codemode")
        .and_then(Value::as_object)
        .and_then(|codemode| codemode.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn number_setting(settings: &Value) -> Option<u32> {
    settings
        .get("codemode")
        .and_then(Value::as_object)
        .and_then(|codemode| codemode.get("inlineBudget"))
        .and_then(Value::as_u64)
        .and_then(|budget| u32::try_from(budget).ok())
}

fn get_settings(
    paths: &AppPaths,
    scope: ConfigScope,
    project_path: Option<&str>,
) -> Result<PiCodemodeSettings, String> {
    let global = read_settings(&paths.pi_home.join("settings.json"))?;
    let project = if scope == ConfigScope::Project {
        Some(read_settings(&settings_path(paths, scope, project_path)?)?)
    } else {
        None
    };
    let inherited_mode = string_setting(&global, "mode");
    let mode = project
        .as_ref()
        .and_then(|settings| string_setting(settings, "mode"))
        .or(inherited_mode)
        .filter(|mode| matches!(mode.as_str(), "on" | "only"))
        .unwrap_or_else(|| "on".into());
    let inline_budget = project
        .as_ref()
        .and_then(number_setting)
        .or_else(|| number_setting(&global))
        .unwrap_or(DEFAULT_INLINE_BUDGET);
    let enabled = resolved_default_tools(Some(&global), project.as_ref())
        .iter()
        .any(|tool| tool == "codemode");
    Ok(PiCodemodeSettings {
        enabled,
        mode,
        inline_budget,
    })
}

#[tauri::command]
pub async fn pi_codemode_settings(
    paths: State<'_, AppPaths>,
    scope: ConfigScope,
    project_path: Option<String>,
) -> Result<PiCodemodeSettings, String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        get_settings(&paths, scope, project_path.as_deref())
    })
    .await
    .map_err(|error| format!("Pi Codemode settings worker failed: {error}"))?
}

fn update_default_tool(settings: &mut Value, enabled: bool) -> Result<(), String> {
    let object = settings
        .as_object_mut()
        .ok_or_else(|| "Pi settings must be a JSON object".to_string())?;
    let entries = object
        .entry("defaultTools")
        .or_insert_with(|| Value::Array(Vec::new()));
    let entries = entries
        .as_array_mut()
        .ok_or_else(|| "Pi defaultTools must be an array; refusing to overwrite it".to_string())?;
    entries.retain(|entry| !matches!(entry.as_str(), Some("+codemode" | "-codemode")));
    if enabled {
        let already_enabled = entries.iter().any(|entry| {
            entry
                .as_str()
                .is_some_and(|name| name == "codemode" || name == "+codemode")
        });
        if !already_enabled {
            entries.push(Value::String("+codemode".into()));
        }
    } else {
        entries.push(Value::String("-codemode".into()));
    }
    Ok(())
}

fn save_settings(paths: &AppPaths, request: PiCodemodeSaveRequest) -> Result<(), String> {
    if request.mode != "on" && request.mode != "only" {
        return Err("Codemode mode must be 'on' or 'only'".into());
    }
    if request.inline_budget > MAX_INLINE_BUDGET {
        return Err(format!(
            "Codemode inlineBudget must be at most {MAX_INLINE_BUDGET}"
        ));
    }
    let path = settings_path(paths, request.scope, request.project_path.as_deref())?;
    crate::snapshot::reject_link(&path)?;
    let mut settings = read_settings(&path)?;
    update_default_tool(&mut settings, request.enabled)?;
    let root = settings
        .as_object_mut()
        .ok_or_else(|| "Pi settings must be a JSON object".to_string())?;
    if !root.get("codemode").is_some_and(Value::is_object) {
        root.insert("codemode".into(), Value::Object(Map::new()));
    }
    let codemode = root
        .get_mut("codemode")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "Pi codemode settings must be an object".to_string())?;
    codemode.insert("mode".into(), Value::String(request.mode));
    codemode.insert(
        "inlineBudget".into(),
        Value::Number(request.inline_budget.into()),
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create Pi settings directory: {error}"))?;
    }
    let content = serde_json::to_vec_pretty(&settings)
        .map_err(|error| format!("failed to serialize Pi settings: {error}"))?;
    let mut file = AtomicWriteFile::open(path)
        .map_err(|error| format!("failed to open Pi settings: {error}"))?;
    use std::io::Write;
    file.write_all(&content)
        .map_err(|error| format!("failed to write Pi settings: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to commit Pi settings: {error}"))
}

#[tauri::command]
pub async fn save_pi_codemode_settings(
    paths: State<'_, AppPaths>,
    request: PiCodemodeSaveRequest,
) -> Result<(), String> {
    let paths = paths.inner().clone();
    tauri::async_runtime::spawn_blocking(move || save_settings(&paths, request))
        .await
        .map_err(|error| format!("Pi Codemode settings worker failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{json, Value};

    use super::{get_settings, resolved_default_tools, save_settings, PiCodemodeSaveRequest};
    use crate::{agent_config::ConfigScope, app_paths::AppPaths};

    #[test]
    fn resolves_pi_default_tools_modifiers_across_global_and_project_scopes() {
        let global = json!({"defaultTools":["read","bash","edit","write","+codemode"]});
        let project = json!({"defaultTools":["-codemode"]});
        assert!(resolved_default_tools(Some(&global), None).contains(&"codemode".into()));
        assert!(!resolved_default_tools(Some(&global), Some(&project)).contains(&"codemode".into()));
        assert!(
            resolved_default_tools(None, Some(&json!({"defaultTools":["+codemode"]})))
                .contains(&"codemode".into())
        );
    }

    #[test]
    fn modifier_only_tool_lists_start_from_pi_default_tools() {
        assert_eq!(
            resolved_default_tools(None, Some(&json!({"defaultTools":["+codemode","-read"]}))),
            vec!["bash", "edit", "write", "codemode"]
        );
        assert_eq!(
            resolved_default_tools(Some(&json!({"defaultTools":["-write"]})), None),
            vec!["read", "bash", "edit"]
        );
    }

    #[test]
    fn saves_only_codemode_fields_and_preserves_other_pi_settings() {
        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_roots(
            root.path().join("roaming"),
            root.path().join("local"),
            root.path().join("project"),
        )
        .unwrap();
        fs::write(
            paths.pi_home.join("settings.json"),
            r#"{"theme":"custom","defaultTools":["read","bash","edit","write"],"unknown":{"keep":true}}"#,
        )
        .unwrap();
        save_settings(
            &paths,
            PiCodemodeSaveRequest {
                scope: ConfigScope::Global,
                project_path: None,
                enabled: true,
                mode: "only".into(),
                inline_budget: 0,
            },
        )
        .unwrap();
        let value: Value =
            serde_json::from_slice(&fs::read(paths.pi_home.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(value["theme"], "custom");
        assert_eq!(value["unknown"]["keep"], true);
        assert_eq!(value["defaultTools"][4], "+codemode");
        assert_eq!(value["codemode"]["mode"], "only");
        assert_eq!(value["codemode"]["inlineBudget"], 0);
    }

    #[test]
    fn project_scope_codemode_modifier_does_not_overwrite_global_tool_selection() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let paths = AppPaths::from_roots(
            root.path().join("roaming"),
            root.path().join("local"),
            root.path().join("project-root"),
        )
        .unwrap();
        fs::write(
            paths.pi_home.join("settings.json"),
            r#"{"defaultTools":["read","bash","edit","write"]}"#,
        )
        .unwrap();
        save_settings(
            &paths,
            PiCodemodeSaveRequest {
                scope: ConfigScope::Project,
                project_path: Some(project.to_string_lossy().into_owned()),
                enabled: true,
                mode: "on".into(),
                inline_budget: 3000,
            },
        )
        .unwrap();
        let local: Value =
            serde_json::from_slice(&fs::read(project.join(".pi/settings.json")).unwrap()).unwrap();
        assert_eq!(local["defaultTools"], json!(["+codemode"]));
        assert!(resolved_default_tools(
            Some(&json!({"defaultTools":["read","bash","edit","write"]})),
            Some(&local),
        )
        .contains(&"codemode".into()));
        assert_eq!(
            get_settings(
                &paths,
                ConfigScope::Project,
                Some(project.to_str().unwrap())
            )
            .unwrap()
            .enabled,
            true
        );
    }
}
