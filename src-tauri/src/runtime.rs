use std::{
    cmp::Ordering,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::{
    app_paths::AppPaths,
    dsh::DshManager,
    provider,
    task::{TaskStatus, TaskStore},
};

const MIN_DSHMARKET_DSH_VERSION: &str = "0.1.1-rc.2";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeComponent {
    pub id: &'static str,
    pub name: &'static str,
    pub current_version: Option<String>,
    pub source: &'static str,
    pub available: bool,
}

fn extract_version(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|token| {
        let candidate = token
            .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '.')
            .trim_start_matches('v');
        (candidate
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_digit())
            && candidate.contains('.')
            && candidate.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
            }))
        .then(|| candidate.to_string())
    })
}

fn package_version_from_json(content: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

fn command_version(command: &mut Command) -> Option<String> {
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = format!(
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    extract_version(&text)
}

pub(crate) fn runtime_status_for_paths(paths: &AppPaths) -> Vec<RuntimeComponent> {
    let (mut pi_command, pi_source) = if let Some(pi_cli) = paths.pi_cli() {
        let mut command = Command::new(paths.node_executable());
        command.arg(pi_cli).arg("--version");
        (command, "managed")
    } else {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoLogo", "-NoProfile", "-Command", "pi --version"]);
        (command, "system")
    };
    pi_command.env("PI_CODING_AGENT_DIR", &paths.pi_home);
    let pi_version = command_version(&mut pi_command);

    let dsh_runtime = paths.dsh_runtime();
    let mut dsh_command = Command::new(paths.node_executable());
    dsh_command
        .arg(AppPaths::dsh_cli_path(&dsh_runtime))
        .arg("--version");
    let dsh_version = command_version(&mut dsh_command);
    let dsh_source = if dsh_runtime == paths.managed_dsh_runtime() {
        "managed"
    } else {
        "development"
    };

    let market_version = fs::read_to_string(paths.dshmarket_manifest())
        .ok()
        .and_then(|content| package_version_from_json(&content));

    vec![
        RuntimeComponent {
            id: "deeppi",
            name: "DeepPi",
            available: true,
            current_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            source: "managed",
        },
        RuntimeComponent {
            id: "pi",
            name: "Pi Coding Agent",
            available: pi_version.is_some(),
            current_version: pi_version,
            source: pi_source,
        },
        RuntimeComponent {
            id: "dsh",
            name: "DeepSeek Harness",
            available: dsh_version.is_some(),
            current_version: dsh_version,
            source: dsh_source,
        },
        RuntimeComponent {
            id: "dshmarket",
            name: "DSH Plugin Market",
            available: market_version.is_some(),
            current_version: market_version,
            source: "profile",
        },
    ]
}

#[tauri::command]
pub fn runtime_status(paths: State<'_, AppPaths>) -> Vec<RuntimeComponent> {
    runtime_status_for_paths(&paths)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUpdate {
    pub id: String,
    pub name: String,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub installable: bool,
    pub can_rollback: bool,
    pub stale: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct UpdateCache {
    entries: Mutex<Option<(Instant, Vec<RuntimeUpdate>)>>,
}

impl UpdateCache {
    fn get(&self) -> Option<Vec<RuntimeUpdate>> {
        let entries = self.entries.lock().ok()?;
        let (created, updates) = entries.as_ref()?;
        (created.elapsed() < Duration::from_secs(600)).then(|| updates.clone())
    }

    fn insert(&self, updates: Vec<RuntimeUpdate>) {
        if let Ok(mut entries) = self.entries.lock() {
            *entries = Some((Instant::now(), updates));
        }
    }

    fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            *entries = None;
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedRuntimeUpdates {
    checked_at: u64,
    updates: Vec<RuntimeUpdate>,
}

fn runtime_update_cache_path(paths: &AppPaths) -> PathBuf {
    paths.cache.join("runtime-updates.json")
}

fn now_seconds() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn read_persisted_updates(paths: &AppPaths) -> Option<Vec<RuntimeUpdate>> {
    let content = fs::read_to_string(runtime_update_cache_path(paths)).ok()?;
    let cached: PersistedRuntimeUpdates = serde_json::from_str(&content).ok()?;
    let now = now_seconds()?;
    (now.saturating_sub(cached.checked_at) <= 7 * 24 * 60 * 60).then(|| {
        cached
            .updates
            .into_iter()
            .map(|mut update| {
                update.stale = true;
                update
            })
            .collect()
    })
}

fn write_persisted_updates(paths: &AppPaths, updates: &[RuntimeUpdate]) {
    let Some(checked_at) = now_seconds() else {
        return;
    };
    let Ok(content) = serde_json::to_vec_pretty(&PersistedRuntimeUpdates {
        checked_at,
        updates: updates.to_vec(),
    }) else {
        return;
    };
    let path = runtime_update_cache_path(paths);
    if let Ok(mut file) = AtomicWriteFile::open(path) {
        if file.write_all(&content).is_ok() {
            let _ = file.commit();
        }
    }
}

#[tauri::command]
pub fn clear_runtime_update_cache(cache: State<'_, UpdateCache>) {
    cache.clear();
}

fn runtime_package(id: &str) -> Option<&'static str> {
    match id {
        "pi" => Some("@earendil-works/pi-coding-agent"),
        "dsh" => Some("@deepseek-ai/dsh"),
        "dshmarket" => Some("dshmarket"),
        _ => None,
    }
}

fn valid_package_version(version: &str) -> bool {
    let mut parts = version.splitn(2, '-');
    let numbers = parts.next().unwrap_or_default().split('.');
    numbers.clone().count() == 3
        && numbers.into_iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
        && parts.next().is_none_or(|suffix| {
            !suffix.is_empty()
                && suffix.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
                })
        })
}

fn configured_update_proxy() -> Result<Option<String>, String> {
    for variable in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        let Some(value) = std::env::var_os(variable) else {
            continue;
        };
        let value = value.to_string_lossy().trim().to_owned();
        if value.is_empty() {
            continue;
        }
        provider::validate_proxy(Some(&value))?;
        return Ok(Some(value));
    }
    Ok(None)
}

fn latest_package_version(package: &str, proxy: Option<&str>) -> Result<String, String> {
    let encoded = package.replace('/', "%2f");
    let mut config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false);
    if let Some(proxy) = proxy {
        let proxy =
            ureq::Proxy::new(proxy).map_err(|error| format!("proxy is invalid: {error}"))?;
        config = config.proxy(Some(proxy));
    }
    let agent = config.build().new_agent();
    let mut response = agent
        .get(&format!("https://registry.npmjs.org/{encoded}"))
        .call()
        .map_err(|error| format!("update check failed: {error}"))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(512 * 1024)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(|error| format!("update response read failed: {error}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("update check returned HTTP {status}"));
    }
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("update response is invalid JSON: {error}"))?;
    let version = value
        .get("dist-tags")
        .and_then(|tags| tags.get("latest"))
        .and_then(serde_json::Value::as_str)
        .filter(|version| valid_package_version(version))
        .ok_or_else(|| "update response has no valid latest version".to_string())?;
    Ok(version.to_owned())
}

fn version_parts(version: &str) -> Option<([u64; 3], Option<&str>)> {
    if !valid_package_version(version) {
        return None;
    }
    let (numbers, prerelease) = version
        .split_once('-')
        .map_or((version, None), |(base, pre)| (base, Some(pre)));
    let mut parts = numbers.split('.').map(|part| part.parse::<u64>().ok());
    Some(([parts.next()??, parts.next()??, parts.next()??], prerelease))
}

fn compare_prerelease(left: &str, right: &str) -> Ordering {
    for (left_part, right_part) in left.split('.').zip(right.split('.')) {
        let ordering = match (left_part.parse::<u64>(), right_part.parse::<u64>()) {
            (Ok(left), Ok(right)) => left.cmp(&right),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => left_part.cmp(right_part),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.split('.').count().cmp(&right.split('.').count())
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let Some((left_numbers, left_pre)) = version_parts(left) else {
        return Ordering::Equal;
    };
    let Some((right_numbers, right_pre)) = version_parts(right) else {
        return Ordering::Equal;
    };
    let numbers = left_numbers.cmp(&right_numbers);
    if numbers != Ordering::Equal {
        return numbers;
    }
    match (left_pre, right_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => compare_prerelease(left, right),
    }
}

fn version_is_newer(current: &str, latest: &str) -> bool {
    compare_versions(latest, current) == Ordering::Greater
}

fn dshmarket_is_compatible(paths: &AppPaths) -> bool {
    runtime_status_for_paths(paths)
        .into_iter()
        .find(|component| component.id == "dsh")
        .and_then(|component| component.current_version)
        .is_some_and(|version| {
            compare_versions(&version, MIN_DSHMARKET_DSH_VERSION) != Ordering::Less
        })
}

fn component_installable(paths: &AppPaths, component: &str) -> bool {
    runtime_installable(component) && (component != "dshmarket" || dshmarket_is_compatible(paths))
}

fn runtime_root(paths: &AppPaths, component: &str) -> Result<PathBuf, String> {
    match component {
        "pi" | "dsh" => Ok(paths.runtimes.join(component)),
        _ => Err("only Pi and DSH runtimes can be replaced".into()),
    }
}

fn runtime_cli_path(root: &Path, component: &str) -> Option<PathBuf> {
    match component {
        "pi" => Some(
            root.join("node_modules")
                .join("@earendil-works")
                .join("pi-coding-agent")
                .join("dist")
                .join("bundle")
                .join("cli.js"),
        ),
        "dsh" => Some(
            root.join("node_modules")
                .join("@deepseek-ai")
                .join("dsh")
                .join("lib")
                .join("bin.js"),
        ),
        _ => None,
    }
}

fn runtime_installable(component: &str) -> bool {
    matches!(component, "pi" | "dsh" | "dshmarket")
}

fn runtime_backup_exists(paths: &AppPaths, component: &str) -> bool {
    if component == "dshmarket" {
        return latest_dshmarket_backup(paths).is_some();
    }
    let Ok(root) = runtime_root(paths, component) else {
        return false;
    };
    fs::read_dir(root.join("backups"))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry.file_name().to_string_lossy().starts_with("previous-")
        })
}

fn ensure_runtime_idle(
    component: &str,
    store: &TaskStore,
    dsh_manager: &DshManager,
) -> Result<(), String> {
    if component == "pi"
        && store.list()?.iter().any(|task| {
            task.agent == "pi"
                && matches!(
                    task.status,
                    TaskStatus::Queued | TaskStatus::Running | TaskStatus::Waiting
                )
        })
    {
        return Err("请先停止所有 Pi Session，再切换 Pi runtime".into());
    }
    if component == "dsh" && dsh_manager.is_running()? {
        return Err("请先关闭 DSH，再切换 DSH runtime".into());
    }
    Ok(())
}

fn npm_executable() -> &'static str {
    #[cfg(windows)]
    {
        "npm.cmd"
    }
    #[cfg(not(windows))]
    {
        "npm"
    }
}

fn truncate_process_output(output: &[u8]) -> String {
    const MAX_OUTPUT: usize = 64 * 1024;
    let output = String::from_utf8_lossy(output);
    if output.len() <= MAX_OUTPUT {
        return output.into_owned();
    }
    let end = output
        .char_indices()
        .take_while(|(index, _)| *index < MAX_OUTPUT)
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    format!("{}\n[output truncated]", &output[..end])
}

fn npm_install(prefix: &Path, spec: &str) -> Result<(), String> {
    let prefix = prefix.to_string_lossy().into_owned();
    let proxy = configured_update_proxy()?;
    let mut command = Command::new(npm_executable());
    command
        .args([
            "install",
            "--prefix",
            &prefix,
            "--ignore-scripts",
            "--no-save",
            "--no-audit",
            "--no-fund",
            "--package-lock=false",
            "--fetch-timeout=30000",
            "--fetch-retries=2",
        ])
        .arg(spec)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    if let Some(proxy) = proxy {
        command.env("HTTP_PROXY", &proxy).env("HTTPS_PROXY", &proxy);
    }
    let output = command
        .output()
        .map_err(|error| format!("failed to install runtime package: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = truncate_process_output(&output.stdout);
    let stderr = truncate_process_output(&output.stderr);
    let details = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "no output".to_owned(),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stdout}\n{stderr}"),
    };
    Err(format!("runtime installation failed: {details}"))
}

fn dshmarket_backup_root(paths: &AppPaths) -> PathBuf {
    paths.runtimes.join("dshmarket").join("backups")
}

fn latest_dshmarket_backup(paths: &AppPaths) -> Option<PathBuf> {
    fs::read_dir(dshmarket_backup_root(paths))
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("dshmarket-previous-")
        })
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        })
        .map(|entry| entry.path())
}

fn install_dshmarket(paths: &AppPaths, version: &str) -> Result<RuntimeOperationResult, String> {
    let profile = paths.dsh_profile();
    if !profile.is_dir() {
        return Err("DSH profile is not installed".into());
    }
    let package = paths.dshmarket_package();
    let backups = dshmarket_backup_root(paths);
    fs::create_dir_all(&backups)
        .map_err(|error| format!("failed to create dshmarket backup directory: {error}"))?;
    let backup = if package.exists() {
        let backup = backups.join(format!("dshmarket-previous-{}", Uuid::new_v4()));
        fs::rename(&package, &backup)
            .map_err(|error| format!("failed to stage old dshmarket: {error}"))?;
        Some(backup)
    } else {
        None
    };
    let spec = format!("dshmarket@{version}");
    if let Err(error) = npm_install(&profile, &spec) {
        let _ = fs::remove_dir_all(&package);
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, &package);
        }
        return Err(error);
    }
    let installed_version = fs::read_to_string(paths.dshmarket_manifest())
        .ok()
        .and_then(|content| package_version_from_json(&content));
    if installed_version.as_deref() != Some(version) {
        let failed = backups.join(format!("dshmarket-failed-{}", Uuid::new_v4()));
        let _ = fs::rename(&package, failed);
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, &package);
        }
        return Err("dshmarket failed its version verification".into());
    }
    Ok(RuntimeOperationResult {
        component_id: "dshmarket".into(),
        version: version.into(),
    })
}

fn verify_runtime(paths: &AppPaths, component: &str, root: &Path) -> Result<String, String> {
    let cli = runtime_cli_path(root, component)
        .ok_or_else(|| "runtime component is not installable".to_string())?;
    if !cli.is_file() {
        return Err(format!("{component} runtime has no executable entry"));
    }
    let mut command = Command::new(paths.node_executable());
    command.arg(cli).arg("--version");
    if component == "pi" {
        command.env("PI_CODING_AGENT_DIR", &paths.pi_home);
    } else {
        command.env("DSH_HOME", &paths.dsh_home);
    }
    command_version(&mut command)
        .ok_or_else(|| format!("{component} runtime failed its startup verification"))
}

fn latest_runtime_backup(root: &Path) -> Option<PathBuf> {
    fs::read_dir(root.join("backups"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry.file_name().to_string_lossy().starts_with("previous-")
        })
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        })
        .map(|entry| entry.path())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallRequest {
    component_id: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRollbackRequest {
    component_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOperationResult {
    pub component_id: String,
    pub version: String,
}

pub struct RuntimeOperationLock(Mutex<()>);

impl Default for RuntimeOperationLock {
    fn default() -> Self {
        Self(Mutex::new(()))
    }
}

#[tauri::command]
pub fn install_runtime(
    lock: State<'_, RuntimeOperationLock>,
    paths: State<'_, AppPaths>,
    store: State<'_, TaskStore>,
    dsh_manager: State<'_, DshManager>,
    request: RuntimeInstallRequest,
) -> Result<RuntimeOperationResult, String> {
    let _guard = lock
        .0
        .lock()
        .map_err(|_| "runtime operation is unavailable".to_string())?;
    if !component_installable(&paths, &request.component_id) {
        return Err("该组件当前不可更新，请先满足其运行时兼容条件".into());
    }
    if !valid_package_version(&request.version) {
        return Err("runtime version is invalid".into());
    }
    ensure_runtime_idle(&request.component_id, &store, &dsh_manager)?;
    let package = runtime_package(&request.component_id)
        .ok_or_else(|| "runtime component is not supported".to_string())?;
    let proxy = configured_update_proxy()?;
    let latest = latest_package_version(package, proxy.as_deref())?;
    if latest != request.version {
        return Err("只能安装刚从官方 registry 验证的最新版本".into());
    }
    if request.component_id == "dshmarket" {
        let result = install_dshmarket(&paths, &request.version)?;
        log::info!(
            "event=runtime_install component={} version={} status=active",
            request.component_id,
            request.version
        );
        return Ok(result);
    }

    let root = runtime_root(&paths, &request.component_id)?;
    let staging_root = root.join("staging");
    fs::create_dir_all(&staging_root)
        .map_err(|error| format!("failed to create runtime staging directory: {error}"))?;
    let staging = staging_root.join(format!("{}-{}", request.version, Uuid::new_v4()));
    fs::create_dir_all(&staging)
        .map_err(|error| format!("failed to create runtime staging directory: {error}"))?;
    let spec = format!("{package}@{}", request.version);
    if let Err(error) = npm_install(&staging, &spec) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = verify_runtime(&paths, &request.component_id, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let current = root.join("current");
    let backups = root.join("backups");
    fs::create_dir_all(&backups)
        .map_err(|error| format!("failed to create runtime backup directory: {error}"))?;
    let backup = if current.exists() {
        let backup = backups.join(format!("previous-{}", Uuid::new_v4()));
        fs::rename(&current, &backup).map_err(|error| {
            let _ = fs::remove_dir_all(&staging);
            format!("failed to stage old runtime for rollback: {error}")
        })?;
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(&staging, &current) {
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, &current);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("failed to activate runtime: {error}"));
    }
    if let Err(error) = verify_runtime(&paths, &request.component_id, &current) {
        let failed = backups.join(format!("failed-{}", Uuid::new_v4()));
        let _ = fs::rename(&current, failed);
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, &current);
        }
        return Err(error);
    }
    log::info!(
        "event=runtime_install component={} version={} status=active",
        request.component_id,
        request.version
    );
    Ok(RuntimeOperationResult {
        component_id: request.component_id,
        version: request.version,
    })
}

#[tauri::command]
pub fn rollback_runtime(
    lock: State<'_, RuntimeOperationLock>,
    paths: State<'_, AppPaths>,
    store: State<'_, TaskStore>,
    dsh_manager: State<'_, DshManager>,
    request: RuntimeRollbackRequest,
) -> Result<RuntimeOperationResult, String> {
    let _guard = lock
        .0
        .lock()
        .map_err(|_| "runtime operation is unavailable".to_string())?;
    if !runtime_installable(&request.component_id) {
        return Err("该组件没有可回滚的独立运行时".into());
    }
    ensure_runtime_idle(&request.component_id, &store, &dsh_manager)?;
    if request.component_id == "dshmarket" {
        let backup = latest_dshmarket_backup(&paths)
            .ok_or_else(|| "没有可用的 dshmarket 回滚版本".to_string())?;
        let package = paths.dshmarket_package();
        let backups = dshmarket_backup_root(&paths);
        let failed = backups.join(format!("dshmarket-rollback-{}", Uuid::new_v4()));
        if package.exists() {
            fs::rename(&package, &failed)
                .map_err(|error| format!("failed to stage current dshmarket: {error}"))?;
        }
        if let Err(error) = fs::rename(&backup, &package) {
            if failed.exists() {
                let _ = fs::rename(&failed, &package);
            }
            return Err(format!("failed to activate dshmarket rollback: {error}"));
        }
        let version = fs::read_to_string(paths.dshmarket_manifest())
            .ok()
            .and_then(|content| package_version_from_json(&content))
            .ok_or_else(|| "dshmarket rollback failed its version verification".to_string());
        let version = match version {
            Ok(version) => version,
            Err(error) => {
                let broken = backups.join(format!("dshmarket-failed-{}", Uuid::new_v4()));
                let _ = fs::rename(&package, broken);
                if failed.exists() {
                    let _ = fs::rename(&failed, &package);
                }
                return Err(error);
            }
        };
        log::info!(
            "event=runtime_rollback component=dshmarket version={} status=active",
            version
        );
        return Ok(RuntimeOperationResult {
            component_id: request.component_id,
            version,
        });
    }
    let root = runtime_root(&paths, &request.component_id)?;
    let backup =
        latest_runtime_backup(&root).ok_or_else(|| "没有可用的 runtime 回滚版本".to_string())?;
    let current = root.join("current");
    let failed = root
        .join("backups")
        .join(format!("rollback-{}", Uuid::new_v4()));
    if current.exists() {
        fs::rename(&current, &failed)
            .map_err(|error| format!("failed to stage current runtime: {error}"))?;
    }
    if let Err(error) = fs::rename(&backup, &current) {
        if failed.exists() {
            let _ = fs::rename(&failed, &current);
        }
        return Err(format!("failed to activate rollback runtime: {error}"));
    }
    let version = match verify_runtime(&paths, &request.component_id, &current) {
        Ok(version) => version,
        Err(error) => {
            let broken = root
                .join("backups")
                .join(format!("failed-{}", Uuid::new_v4()));
            let _ = fs::rename(&current, broken);
            if failed.exists() {
                let _ = fs::rename(&failed, &current);
            }
            return Err(error);
        }
    };
    log::info!(
        "event=runtime_rollback component={} version={} status=active",
        request.component_id,
        version
    );
    Ok(RuntimeOperationResult {
        component_id: request.component_id,
        version,
    })
}

#[tauri::command]
pub fn check_runtime_updates(
    cache: State<'_, UpdateCache>,
    paths: State<'_, AppPaths>,
) -> Result<Vec<RuntimeUpdate>, String> {
    if let Some(updates) = cache.get() {
        log::debug!("event=runtime_update_check status=cache_hit");
        return Ok(updates);
    }
    let started = Instant::now();
    let updates = runtime_status_for_paths(&paths)
        .into_iter()
        .map(|component| {
            let Some(package) = runtime_package(component.id) else {
                return RuntimeUpdate {
                    id: component.id.to_owned(),
                    name: component.name.to_owned(),
                    current_version: component.current_version,
                    latest_version: None,
                    update_available: false,
                    installable: component_installable(&paths, component.id),
                    can_rollback: runtime_backup_exists(&paths, component.id),
                    stale: false,
                    error: None,
                };
            };
            match configured_update_proxy()
                .and_then(|proxy| latest_package_version(package, proxy.as_deref()))
            {
                Ok(latest_version) => RuntimeUpdate {
                    id: component.id.to_owned(),
                    name: component.name.to_owned(),
                    update_available: component
                        .current_version
                        .as_deref()
                        .is_some_and(|current| version_is_newer(current, &latest_version)),
                    current_version: component.current_version,
                    latest_version: Some(latest_version),
                    installable: component_installable(&paths, component.id),
                    can_rollback: runtime_backup_exists(&paths, component.id),
                    stale: false,
                    error: None,
                },
                Err(error) => RuntimeUpdate {
                    id: component.id.to_owned(),
                    name: component.name.to_owned(),
                    current_version: component.current_version,
                    latest_version: None,
                    update_available: false,
                    installable: component_installable(&paths, component.id),
                    can_rollback: runtime_backup_exists(&paths, component.id),
                    stale: false,
                    error: Some(error),
                },
            }
        })
        .collect::<Vec<_>>();
    if updates.iter().all(|update| update.error.is_none()) {
        cache.insert(updates.clone());
        write_persisted_updates(&paths, &updates);
    } else if let Some(cached) = read_persisted_updates(&paths) {
        log::warn!(
            "event=runtime_update_check status=offline_cache duration_ms={}",
            started.elapsed().as_millis()
        );
        return Ok(cached);
    }
    log::info!(
        "event=runtime_update_check status=complete duration_ms={} errors={} available={}",
        started.elapsed().as_millis(),
        updates
            .iter()
            .filter(|update| update.error.is_some())
            .count(),
        updates
            .iter()
            .filter(|update| update.update_available)
            .count()
    );
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::{
        extract_version, package_version_from_json, runtime_cli_path, runtime_installable,
        valid_package_version, version_is_newer,
    };

    #[test]
    fn validates_runtime_packages_and_entries() {
        assert!(runtime_installable("pi"));
        assert!(runtime_installable("dsh"));
        assert!(runtime_installable("dshmarket"));
        assert!(!runtime_installable("unknown"));
        assert!(valid_package_version("1.2.3-rc.1"));
        assert!(!valid_package_version("1.2"));
        assert!(runtime_cli_path(std::path::Path::new("F:/runtime"), "pi")
            .expect("Pi entry should exist")
            .ends_with("dist/bundle/cli.js"));
    }

    #[test]
    fn compares_release_versions() {
        assert!(version_is_newer("1.2.3", "1.2.4"));
        assert!(version_is_newer("1.2.3", "2.0.0"));
        assert!(!version_is_newer("1.2.3", "1.2.3"));
        assert!(!version_is_newer("2.0.0", "1.9.9"));
        assert!(version_is_newer("1.2.3-rc.1", "1.2.3-rc.2"));
        assert!(version_is_newer("1.2.3-rc.2", "1.2.3"));
        assert!(!version_is_newer("1.2.3", "1.2.3-rc.2"));
    }

    #[test]
    fn extracts_plain_and_prefixed_versions() {
        assert_eq!(extract_version("0.84.4"), Some("0.84.4".into()));
        assert_eq!(extract_version("pi v0.84.4"), Some("0.84.4".into()));
        assert_eq!(extract_version("dsh 0.1.1-rc.2"), Some("0.1.1-rc.2".into()));
    }

    #[test]
    fn reads_version_from_package_json() {
        assert_eq!(
            package_version_from_json(r#"{"name":"dshmarket","version":"1.40.0"}"#),
            Some("1.40.0".into())
        );
    }
}
