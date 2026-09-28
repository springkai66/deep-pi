use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{app_paths::AppPaths, rpc::RpcManager, task::TaskStore};

fn require_main(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() == "main" {
        Ok(())
    } else {
        Err("Pi commands require the main Webview".into())
    }
}

fn package_dir(paths: &AppPaths) -> Result<PathBuf, String> {
    let root = paths
        .managed_pi_runtime()?
        .join("node_modules/@earendil-works/pi-coding-agent");
    if !root.join("dist/core/settings-manager.js").is_file() {
        return Err("Managed Pi package is unavailable".into());
    }
    Ok(root)
}

#[tauri::command]
pub fn pi_slash_changelog(
    webview: tauri::Webview,
    paths: tauri::State<'_, AppPaths>,
) -> Result<String, String> {
    require_main(&webview)?;
    let file = package_dir(&paths)?.join("CHANGELOG.md");
    crate::snapshot::reject_link(&file)?;
    let text = fs::read_to_string(file).map_err(|error| error.to_string())?;
    let first = text.find("\n## ").unwrap_or(0);
    Ok(text[first..].chars().take(18000).collect())
}

const CONFIG_SCRIPT: &str = r#"
import { pathToFileURL } from 'node:url';
const [root, home, cwd, action, payload] = process.argv.slice(1);
const moduleUrl = (name) => pathToFileURL(`${root}/dist/core/${name}.js`).href;
if (action === 'get_scoped' || action === 'set_scoped') {
  const { SettingsManager } = await import(moduleUrl('settings-manager'));
  const manager = SettingsManager.create(cwd, home, { projectTrusted: false });
  if (action === 'set_scoped') manager.setEnabledModels(JSON.parse(payload));
  console.log(JSON.stringify(manager.getEnabledModels() ?? null));
} else if (action === 'get_trust' || action === 'set_trust') {
  const { ProjectTrustStore } = await import(moduleUrl('trust-manager'));
  const store = new ProjectTrustStore(home);
  if (action === 'set_trust') store.set(cwd, JSON.parse(payload));
  console.log(JSON.stringify(store.getEntry(cwd)));
} else { throw new Error('Unsupported Pi configuration action'); }
"#;

fn validate_config(action: &str, payload: &Value) -> Result<(), String> {
    match action {
        "get_scoped" | "get_trust" if payload.is_null() => Ok(()),
        "set_trust" if payload.is_boolean() => Ok(()),
        "set_scoped"
            if payload.is_null()
                || payload.as_array().is_some_and(|items| {
                    items.len() <= 100
                        && items.iter().all(|item| {
                            item.as_str().is_some_and(|value| {
                                value.len() <= 200
                                    && !value.is_empty()
                                    && !value.chars().any(char::is_control)
                            })
                        })
                }) =>
        {
            Ok(())
        }
        _ => Err("Invalid Pi configuration request".into()),
    }
}

#[tauri::command]
pub async fn pi_slash_config(
    webview: tauri::Webview,
    app: AppHandle,
    task_id: String,
    action: String,
    payload: Value,
) -> Result<Value, String> {
    require_main(&webview)?;
    validate_config(&action, &payload)?;
    tauri::async_runtime::spawn_blocking(move || {
        let paths = app.state::<AppPaths>();
        let task = app
            .state::<TaskStore>()
            .get(&task_id)?
            .ok_or("Task not found")?;
        if task.agent != "pi" || task.pi_environment != "managed" {
            return Err("Managed Pi task required".into());
        }
        let root = package_dir(&paths)?;
        let result = Command::new(paths.node_runtime()?)
            .arg("--input-type=module")
            .arg("-e")
            .arg(CONFIG_SCRIPT)
            .arg(root)
            .arg(paths.task_pi_home(&task)?)
            .arg(&task.project_path)
            .arg(&action)
            .arg(payload.to_string())
            .output()
            .map_err(|error| error.to_string())?;
        if !result.status.success() {
            return Err(format!(
                "Pi configuration operation failed: {}",
                String::from_utf8_lossy(&result.stderr)
                    .chars()
                    .take(500)
                    .collect::<String>()
            ));
        }
        serde_json::from_slice::<Value>(&result.stdout)
            .map_err(|error| format!("Invalid Pi configuration response: {error}"))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn check_source(path: &Path, project: &Path) -> Result<(String, String), String> {
    if !path.is_absolute() || path.extension().is_none_or(|ext| ext != "jsonl") {
        return Err("Choose a JSONL session file".into());
    }
    crate::snapshot::reject_link(path)?;
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Session source is not a file".into());
    }
    let mut reader = BufReader::new(file.take(8192));
    let mut first = String::new();
    reader
        .read_line(&mut first)
        .map_err(|error| error.to_string())?;
    let header: Value = serde_json::from_str(&first).map_err(|_| "Invalid session header")?;
    let id = header["id"]
        .as_str()
        .filter(|id| crate::native_pi::valid_session_id(id))
        .ok_or("Invalid session ID")?;
    let cwd = header["cwd"].as_str().ok_or("Session project is missing")?;
    if header["type"] != "session"
        || !matches!(header["version"].as_u64(), Some(1..=3))
        || fs::canonicalize(cwd).map_err(|_| "Session project is unavailable")?
            != fs::canonicalize(project).map_err(|_| "Task project is unavailable")?
    {
        return Err("Session does not belong to this project".into());
    }
    Ok((id.to_owned(), path.display().to_string()))
}

fn confirm_import_id(expected: Option<&str>, actual: &str) -> Result<(), String> {
    if expected == Some(actual) {
        Ok(())
    } else {
        Err("Session changed since preview; review the selected file again".into())
    }
}

#[tauri::command]
pub async fn pi_slash_import(
    webview: tauri::Webview,
    app: AppHandle,
    task_id: String,
    run_id: String,
    source: String,
    confirmed: bool,
    expected_session_id: Option<String>,
) -> Result<Value, String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<TaskStore>();
        let task = store.get(&task_id)?.ok_or("Task not found")?;
        if task.agent != "pi" || task.pi_environment != "managed" { return Err("Managed Pi task required".into()); }
        let path = Path::new(&source);
        let (id, _) = check_source(path, Path::new(&task.project_path))?;
        if store.list()?.iter().any(|candidate| candidate.id != task_id && candidate.agent == "pi" && candidate.session_id == id) {
            return Err("This session is already linked to another task".into());
        }
        if id == task.session_id { return Err("Imported session has the current session ID".into()); }
        if !confirmed { return Ok(json!({"sessionId":id})); }
        confirm_import_id(expected_session_id.as_deref(), &id)?;
        let transport = app.state::<RpcManager>().transport(&task_id, &run_id)?;
        let state = transport.request(json!({"type":"get_state"}), Duration::from_secs(30))?;
        if state["isStreaming"] == true || state["isCompacting"] == true { return Err("Wait for the current turn to finish".into()); }
        let current = state["sessionFile"].as_str().ok_or("Current session file is missing")?;
        crate::native_pi::validate_reported_session(&app.state::<AppPaths>(), &task, &task.session_id, current)?;
        let parent = Path::new(current).parent().ok_or("Session directory is missing")?;
        let target = parent.join(format!("import-{}.jsonl", Uuid::new_v4()));
        let input = fs::File::open(path).map_err(|error| error.to_string())?;
        if input.metadata().map_err(|error| error.to_string())?.len() > 64 * 1024 * 1024 { return Err("Session exceeds the 64 MiB import limit".into()); }
        let copied = (|| -> Result<(), String> {
            let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&target).map_err(|error| error.to_string())?;
            let bytes = std::io::copy(&mut input.take(64 * 1024 * 1024 + 1), &mut output).map_err(|error| error.to_string())?;
            if bytes > 64 * 1024 * 1024 { return Err("Session exceeds the 64 MiB import limit".into()); }
            output.flush().map_err(|error| error.to_string())?;
            Ok(())
        })();
        if let Err(error) = copied { let _ = fs::remove_file(&target); return Err(error); }
        let mut rebound = task.clone();
        rebound.session_id = id.clone();
        rebound.session_file = None;
        let target_str = match target.to_str() {
            Some(path) => path,
            None => { let _ = fs::remove_file(&target); return Err("Invalid destination".into()); }
        };
        if let Err(error) = crate::native_pi::validate_reported_session(&app.state::<AppPaths>(), &rebound, &id, target_str) {
            let _ = fs::remove_file(&target);
            return Err(error);
        }
        let switched = transport.request(json!({"type":"switch_session","sessionPath":target}), Duration::from_secs(60));
        if switched.as_ref().is_ok_and(|value| value["cancelled"] == true) {
            let _ = fs::remove_file(&target);
            return Ok(json!({"cancelled":true}));
        }
        // A timeout can occur after Pi switched. Reconcile against its actual state before deleting the copy.
        let actual = transport.request(json!({"type":"get_state"}), Duration::from_secs(30));
        if actual.as_ref().is_ok_and(|state| state["sessionId"] == id && state["sessionFile"].as_str() == Some(target_str)) {
            store.rebind_session(&task_id, &task.session_id, &id, target_str)
                .map_err(|error| format!("Session switched, but task mapping could not be updated; imported copy retained at {}: {error}", target.display()))?;
            return Ok(json!({"cancelled":false,"sessionId":id}));
        }
        if let Err(error) = actual {
            return Err(format!("Session switch outcome unknown; imported copy retained at {}: {error}", target.display()));
        }
        let _ = fs::remove_file(&target);
        switched?;
        Err("Imported session state did not match the selected file".into())
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn pi_slash_share(
    webview: tauri::Webview,
    app: AppHandle,
    task_id: String,
    run_id: String,
) -> Result<String, String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let task = app
            .state::<TaskStore>()
            .get(&task_id)?
            .ok_or("Task not found")?;
        if task.agent != "pi" || task.pi_environment != "managed" {
            return Err("Managed Pi task required".into());
        }
        let transport = app.state::<RpcManager>().transport(&task_id, &run_id)?;
        let auth = Command::new("gh")
            .args(["auth", "status"])
            .output()
            .map_err(|_| "GitHub CLI is not installed")?;
        if !auth.status.success() {
            return Err("GitHub CLI is not logged in".into());
        }
        let target = app
            .state::<AppPaths>()
            .temp
            .join(format!("pi-share-{}.html", Uuid::new_v4()));
        let shared = (|| {
            let result = transport.request(
                json!({"type":"export_html","outputPath":target}),
                Duration::from_secs(300),
            )?;
            if result["path"].as_str() != target.to_str() {
                return Err("Session export path changed unexpectedly".into());
            }
            let metadata = fs::metadata(&target).map_err(|error| error.to_string())?;
            if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
                return Err("Session export exceeds the share limit".into());
            }
            let result = Command::new("gh")
                .args(["gist", "create", "--public=false"])
                .arg(&target)
                .output()
                .map_err(|error| error.to_string())?;
            if !result.status.success() {
                return Err(format!(
                    "Gist creation failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                        .chars()
                        .take(500)
                        .collect::<String>()
                ));
            }
            let url = String::from_utf8_lossy(&result.stdout).trim().to_owned();
            if !url.starts_with("https://gist.github.com/") {
                return Err("Gist URL is invalid; check GitHub for the created gist".into());
            }
            Ok(url)
        })();
        let _ = fs::remove_file(target);
        shared
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_rejects_unexpected_changes() {
        assert!(validate_config("set_trust", &json!(true)).is_ok());
        assert!(validate_config("set_trust", &json!("true")).is_err());
        assert!(validate_config("set_scoped", &json!(["provider/model"])).is_ok());
        assert!(validate_config("set_scoped", &json!(["model\nnext"])).is_err());
        assert!(validate_config("set_scoped", &json!({"enabledModels":[]})).is_err());
    }
    #[test]
    fn import_confirmation_requires_the_previewed_session_id() {
        assert!(confirm_import_id(Some("previewed"), "previewed").is_ok());
        assert!(confirm_import_id(Some("previewed"), "replaced").is_err());
        assert!(confirm_import_id(None, "previewed").is_err());
    }
    #[test]
    fn import_rejects_wrong_project_and_invalid_header() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        fs::create_dir(&project).unwrap();
        let file = root.path().join("session.jsonl");
        fs::write(
            &file,
            format!(
                "{}\n",
                json!({"type":"session","version":3,"id":"sample","cwd":root.path()})
            ),
        )
        .unwrap();
        assert!(check_source(&file, &project).is_err());
        fs::write(&file, "not json\n").unwrap();
        assert!(check_source(&file, &project).is_err());
        fs::write(
            &file,
            format!(
                "{}\n",
                json!({"type":"session","version":3,"id":"sample","cwd":project})
            ),
        )
        .unwrap();
        assert!(check_source(&file, &project).is_ok());
    }
}
