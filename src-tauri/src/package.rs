use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::app_paths::AppPaths;

const MAX_OUTPUT_BYTES: usize = 64 * 1024;
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
        .pi_cli()
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

fn truncate_output(output: &[u8]) -> String {
    let output = String::from_utf8_lossy(output);
    if output.len() <= MAX_OUTPUT_BYTES {
        return output.into_owned();
    }
    let end = output
        .char_indices()
        .take_while(|(index, _)| *index < MAX_OUTPUT_BYTES)
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    format!("{}\n[output truncated]", &output[..end])
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
pub fn package_operation(
    paths: State<'_, AppPaths>,
    operation_lock: State<'_, PackageOperationLock>,
    request: PackageRequest,
) -> Result<PackageOperationResult, String> {
    // ponytail: one global lock keeps package settings atomic; split by scope if throughput matters.
    let _lock = operation_lock
        .0
        .lock()
        .map_err(|_| "Pi package operation is already unavailable".to_string())?;
    let (mut command, _) = package_command(&paths, &request)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|error| format!("failed to run Pi package command: {error}"))?;
    let stdout = truncate_output(&output.stdout);
    let stderr = truncate_output(&output.stderr);
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stdout}\n{stderr}"),
    };
    if !output.status.success() {
        return Err(format!(
            "Pi package command failed ({}): {combined}",
            output
                .status
                .code()
                .map_or_else(|| "terminated".into(), |code| code.to_string())
        ));
    }
    Ok(PackageOperationResult { output: combined })
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
