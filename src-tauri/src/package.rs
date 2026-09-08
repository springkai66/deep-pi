use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::{
    app_paths::AppPaths, process_runner, runtime::RuntimeOperationLock, snapshot::Snapshot,
};

const MANAGED_ENVIRONMENT: &str = "managed";
const NATIVE_ENVIRONMENT: &str = "native";
const ALL_ENVIRONMENTS: &str = "all";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    pub source: String,
    pub autoload: Option<bool>,
    pub environment: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRequest {
    operation_id: String,
    operation: String,
    spec: String,
    scope: String,
    project_path: Option<String>,
    #[serde(default)]
    environment: Option<String>,
}

pub struct PackageOperationLock(Mutex<()>);

impl Default for PackageOperationLock {
    fn default() -> Self {
        Self(Mutex::new(()))
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageOperationResult {
    pub output: String,
}

pub fn validate_package_spec(spec: &str) -> Result<&str, String> {
    let spec = spec.trim();
    if spec.is_empty() || spec.len() > 300 || spec.starts_with('-') {
        return Err("package source must contain 1 to 300 characters".into());
    }
    if spec
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err("package source cannot contain whitespace or control characters".into());
    }
    let supported = spec.starts_with("npm:")
        || spec.starts_with("git:")
        || spec.starts_with("https://")
        || spec.starts_with("ssh://")
        || Path::new(spec).is_dir();
    if !supported {
        return Err("package source must use npm:, git:, https://, or ssh://".into());
    }
    Ok(spec)
}

fn package_settings_path(
    paths: &AppPaths,
    scope: &str,
    project_path: Option<&str>,
    environment: &str,
) -> Result<PathBuf, String> {
    match scope {
        "global" => {
            let home = match environment {
                MANAGED_ENVIRONMENT => paths.pi_home.clone(),
                NATIVE_ENVIRONMENT => paths
                    .native_pi_home()
                    .ok_or_else(|| "本机 Pi agent 目录未找到".to_string())?,
                _ => return Err("package environment must be managed or native".into()),
            };
            Ok(home.join("settings.json"))
        }
        "project" => {
            let path =
                project_path.ok_or_else(|| "project scope needs a project path".to_string())?;
            let project = Path::new(path);
            if !project.is_dir() {
                return Err("project directory does not exist".into());
            }
            let project = fs::canonicalize(project)
                .map_err(|error| format!("failed to resolve project directory: {error}"))?;
            Ok(project.join(".pi").join("settings.json"))
        }
        _ => Err("package scope must be global or project".into()),
    }
}

fn parse_installed_packages_for(
    content: &str,
    environment: &str,
) -> Result<Vec<InstalledPackage>, String> {
    let value: Value = serde_json::from_str(content)
        .map_err(|error| format!("invalid Pi settings JSON: {error}"))?;
    let Some(packages) = value.get("packages").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    packages
        .iter()
        .filter_map(|package| match package {
            Value::String(source) => Some(Ok(InstalledPackage {
                source: source.clone(),
                autoload: None,
                environment: environment.to_owned(),
            })),
            Value::Object(object) => object.get("source").and_then(Value::as_str).map(|source| {
                Ok(InstalledPackage {
                    source: source.to_owned(),
                    autoload: object.get("autoload").and_then(Value::as_bool),
                    environment: environment.to_owned(),
                })
            }),
            _ => None,
        })
        .collect()
}

fn read_packages_file(path: &Path, environment: &str) -> Result<Vec<InstalledPackage>, String> {
    match fs::read_to_string(path) {
        Ok(content) => parse_installed_packages_for(&content, environment),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("failed to read Pi package settings: {error}")),
    }
}

pub fn list_packages(
    paths: &AppPaths,
    scope: &str,
    project_path: Option<&str>,
    environment: Option<&str>,
) -> Result<Vec<InstalledPackage>, String> {
    let environment = if scope == "global" {
        environment.unwrap_or(ALL_ENVIRONMENTS)
    } else {
        MANAGED_ENVIRONMENT
    };
    if scope == "global" && environment == ALL_ENVIRONMENTS {
        let mut packages =
            read_packages_file(&paths.pi_home.join("settings.json"), MANAGED_ENVIRONMENT)?;
        if let Some(native_home) = paths.native_pi_home() {
            if native_home != paths.pi_home {
                packages.extend(read_packages_file(
                    &native_home.join("settings.json"),
                    NATIVE_ENVIRONMENT,
                )?);
            }
        }
        return Ok(packages);
    }
    let path = package_settings_path(paths, scope, project_path, environment)?;
    read_packages_file(&path, environment)
}

fn package_command(
    paths: &AppPaths,
    request: &PackageRequest,
) -> Result<(Command, PathBuf), String> {
    let environment = request
        .environment
        .as_deref()
        .unwrap_or(MANAGED_ENVIRONMENT);
    if environment == NATIVE_ENVIRONMENT {
        return Err("本机 Pi 全局扩展目前仅支持查看，请在本机 Pi 中管理".into());
    }
    if environment != MANAGED_ENVIRONMENT {
        return Err("package environment must be managed or native".into());
    }
    let spec = if request.operation == "updateAll" {
        None
    } else {
        Some(validate_package_spec(&request.spec)?)
    };
    let project_path = request.project_path.as_deref();
    let cwd = match request.scope.as_str() {
        "global" => paths.pi_home.clone(),
        "project" => {
            let path =
                project_path.ok_or_else(|| "project scope needs a project path".to_string())?;
            let path = Path::new(path);
            if !path.is_dir() {
                return Err("project directory does not exist".into());
            }
            fs::canonicalize(path)
                .map_err(|error| format!("failed to resolve project directory: {error}"))?
        }
        _ => return Err("package scope must be global or project".into()),
    };
    let pi_cli = paths
        .pi_cli()?
        .ok_or_else(|| "managed Pi runtime is not installed".to_string())?;
    let mut command = Command::new(paths.node_executable());
    command
        .arg(pi_cli)
        .env("PI_CODING_AGENT_DIR", &paths.pi_home)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match request.operation.as_str() {
        "install" => {
            command
                .arg("install")
                .arg(spec.expect("install needs a package spec"));
            if request.scope == "project" {
                command.arg("-l");
            }
        }
        "update" => {
            command
                .arg("update")
                .arg(spec.expect("update needs a package spec"));
        }
        "updateAll" => {
            command.args(["update", "--extensions"]);
        }
        "remove" => {
            command
                .arg("remove")
                .arg(spec.expect("remove needs a package spec"));
            if request.scope == "project" {
                command.arg("-l");
            }
        }
        _ => return Err("package operation must be install, update, updateAll, or remove".into()),
    }
    Ok((command, cwd))
}

#[tauri::command]
pub fn list_pi_packages(
    paths: State<'_, AppPaths>,
    scope: String,
    project_path: Option<String>,
    environment: Option<String>,
) -> Result<Vec<InstalledPackage>, String> {
    list_packages(
        &paths,
        &scope,
        project_path.as_deref(),
        environment.as_deref(),
    )
}

#[tauri::command]
pub async fn package_operation(
    app: AppHandle,
    request: PackageRequest,
) -> Result<PackageOperationResult, String> {
    let operation = app
        .state::<crate::operation::OperationManager>()
        .begin(&request.operation_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        package_operation_inner(
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            request,
            &operation.token,
        )
    })
    .await
    .map_err(|error| format!("package worker failed: {error}"))?
}

fn package_operation_inner(
    paths: State<'_, AppPaths>,
    operation_lock: State<'_, PackageOperationLock>,
    lifecycle: State<'_, RuntimeOperationLock>,
    pty_manager: State<'_, crate::pty::PtyManager>,
    request: PackageRequest,
    cancellation: &crate::operation::Cancellation,
) -> Result<PackageOperationResult, String> {
    let _lifecycle = lifecycle.acquire()?;
    cancellation.check()?;
    Snapshot::ensure_ready(&paths.backups)?;
    let _lock = operation_lock
        .0
        .try_lock()
        .map_err(|_| "Pi package operation is already unavailable".to_string())?;
    if pty_manager.is_running()? {
        return Err("请先停止所有 Pi 任务，再修改扩展，以确保可以安全恢复".into());
    }
    let (mut command, cwd) = package_command(&paths, &request)?;
    let roots = [paths.pi_home.clone(), cwd.join(".pi")];
    let targets = roots
        .iter()
        .flat_map(|root| ["settings.json", "npm", "git"].map(|name| root.join(name)))
        .collect::<Vec<_>>();
    validate_settings_files(&roots)?;
    let snapshot = Snapshot::capture(&targets, &paths.backups)?;
    let result =
        process_runner::run_cancellable(&mut command, Duration::from_secs(600), Some(cancellation))
            .and_then(|output| {
                if !output.status.success() {
                    return Err(format!(
                        "Pi package command failed ({}): {}",
                        output.status,
                        output.text()
                    ));
                }
                validate_settings_files(&roots)?;
                cancellation.commit()?;
                Ok(PackageOperationResult {
                    output: output.text(),
                })
            });
    match result {
        Ok(result) => {
            snapshot.commit()?;
            Ok(result)
        }
        Err(error) => Err(snapshot.restore_error(error)),
    }
}

fn validate_settings_files(roots: &[PathBuf]) -> Result<(), String> {
    for root in roots {
        let path = root.join("settings.json");
        if !path.exists() {
            continue;
        }
        let value: Value =
            serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
                .map_err(|error| format!("invalid {}: {error}", path.display()))?;
        if !value.is_object()
            || value
                .get("packages")
                .is_some_and(|packages| !packages.is_array())
        {
            return Err(format!(
                "invalid package settings schema: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_installed_packages_for, validate_package_spec, MANAGED_ENVIRONMENT};

    #[test]
    fn accepts_supported_package_sources() {
        assert!(validate_package_spec("npm:pi-mcp-adapter@2.32.1").is_ok());
        assert!(validate_package_spec("git:github.com/example/pi-tools@v1").is_ok());
        assert!(validate_package_spec("https://github.com/example/pi-tools").is_ok());
        assert!(validate_package_spec(".").is_ok());
    }

    #[test]
    fn rejects_shell_like_package_sources() {
        assert!(validate_package_spec("npm:foo && whoami").is_err());
        assert!(validate_package_spec("--config=/tmp/bad").is_err());
        assert!(validate_package_spec("npm:foo\nbar").is_err());
    }

    #[test]
    fn parses_string_and_object_package_entries() {
        let json = r#"{
          "packages": [
            "npm:one@1.0.0",
            {"source":"git:github.com/two/pkg@v2","autoload":false}
          ]
        }"#;

        let packages = parse_installed_packages_for(json, MANAGED_ENVIRONMENT)
            .expect("package settings should parse");

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].source, "npm:one@1.0.0");
        assert_eq!(packages[0].environment, "managed");
        assert_eq!(packages[1].source, "git:github.com/two/pkg@v2");
        assert_eq!(packages[1].autoload, Some(false));
    }
}
