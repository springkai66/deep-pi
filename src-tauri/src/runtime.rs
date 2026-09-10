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
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::{
    app_paths::AppPaths,
    dsh::DshManager,
    operation::{Cancellation, OperationManager},
    process_runner, provider,
    recovery::activate_directory,
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
    command_version_cancellable(command, None)
}

fn command_version_cancellable(
    command: &mut Command,
    token: Option<&Cancellation>,
) -> Option<String> {
    let output = process_runner::run_cancellable(command, Duration::from_secs(15), token).ok()?;
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

pub(crate) fn runtime_status_for_paths(paths: &AppPaths) -> Result<Vec<RuntimeComponent>, String> {
    let managed_pi = paths.pi_cli()?;
    let (mut pi_command, pi_source) =
        if let Some(pi_cli) = managed_pi.clone().or_else(|| paths.system_pi_cli()) {
            let mut command = Command::new(paths.node_executable());
            command.arg(pi_cli).arg("--version");
            (
                command,
                if managed_pi.is_some() {
                    "managed"
                } else {
                    "system"
                },
            )
        } else {
            let mut command = Command::new("powershell.exe");
            command.args(["-NoLogo", "-NoProfile", "-Command", "pi --version"]);
            (command, "system")
        };
    pi_command.env("PI_CODING_AGENT_DIR", &paths.pi_home);
    let dsh_runtime = paths.dsh_runtime()?;
    let mut dsh_command = Command::new(paths.node_executable());
    dsh_command
        .arg(AppPaths::dsh_cli_path(&dsh_runtime))
        .arg("--version");
    let (pi_version, dsh_version) = std::thread::scope(|scope| {
        let pi = scope.spawn(|| command_version(&mut pi_command));
        let dsh = command_version(&mut dsh_command);
        (pi.join().unwrap_or(None), dsh)
    });
    let dsh_source = if dsh_runtime == paths.managed_dsh_runtime()? {
        "managed"
    } else {
        "development"
    };

    let market_version = fs::read_to_string(paths.dshmarket_manifest())
        .ok()
        .and_then(|content| package_version_from_json(&content));

    Ok(vec![
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
    ])
}

#[tauri::command]
pub async fn runtime_status(app: AppHandle) -> Result<Vec<RuntimeComponent>, String> {
    tauri::async_runtime::spawn_blocking(move || runtime_status_for_paths(&app.state::<AppPaths>()))
        .await
        .map_err(|error| format!("runtime status worker failed: {error}"))?
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
        .unwrap_or_default()
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
    if crate::runtime_pointer::previous(&root)
        .ok()
        .flatten()
        .is_some()
    {
        return true;
    }
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
    pty_manager: &crate::pty::PtyManager,
) -> Result<(), String> {
    if component == "pi"
        && (pty_manager.is_running()?
            || store.list()?.iter().any(|task| {
                task.agent == "pi"
                    && matches!(
                        task.status,
                        TaskStatus::Queued | TaskStatus::Running | TaskStatus::Waiting
                    )
            }))
    {
        return Err("请先停止所有 Pi Session，再切换 Pi runtime".into());
    }
    if matches!(component, "dsh" | "dshmarket") && dsh_manager.is_running()? {
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

fn npm_install(prefix: &Path, spec: &str, cancellation: &Cancellation) -> Result<(), String> {
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
    let output = process_runner::run_cancellable(
        &mut command,
        Duration::from_secs(600),
        Some(cancellation),
    )?;
    if output.status.success() {
        return Ok(());
    }
    let details = output.text();
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

fn install_dshmarket(
    paths: &AppPaths,
    version: &str,
    cancellation: &Cancellation,
) -> Result<RuntimeOperationResult, String> {
    let profile = paths.dsh_profile();
    if !profile.is_dir() {
        return Err("DSH profile is not installed".into());
    }
    let package = paths.dshmarket_package();
    let backups = dshmarket_backup_root(paths);
    fs::create_dir_all(&backups)
        .map_err(|error| format!("failed to create dshmarket backup directory: {error}"))?;
    let staging = paths
        .runtimes
        .join("dshmarket")
        .join(format!("staging-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let spec = format!("dshmarket@{version}");
    if let Err(error) = npm_install(&staging, &spec, cancellation) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    let candidate = staging.join("node_modules").join("dshmarket");
    let installed_version = fs::read_to_string(candidate.join("package.json"))
        .ok()
        .and_then(|content| package_version_from_json(&content));
    if installed_version.as_deref() != Some(version) {
        let _ = fs::remove_dir_all(&staging);
        return Err("dshmarket failed its version verification".into());
    }
    // Install into the profile only after a restorable snapshot exists, including dependencies.
    let snapshot = crate::snapshot::Snapshot::capture(
        &[
            profile.join("package.json"),
            profile.join("package-lock.json"),
            profile.join("node_modules"),
        ],
        &paths.backups,
    )?;
    snapshot.preserve_entry(
        &package,
        &backups.join(format!("dshmarket-previous-{}", Uuid::new_v4())),
    )?;
    let installed = npm_install(&profile, &spec, cancellation).and_then(|()| {
        let installed = fs::read_to_string(paths.dshmarket_manifest())
            .ok()
            .and_then(|content| package_version_from_json(&content));
        if installed.as_deref() == Some(version) {
            cancellation.commit()
        } else {
            Err("dshmarket failed its profile verification".into())
        }
    });
    let _ = fs::remove_dir_all(&staging);
    if let Err(error) = installed {
        return Err(snapshot.restore_error(error));
    }
    snapshot.commit()?;
    Ok(RuntimeOperationResult {
        component_id: "dshmarket".into(),
        version: version.into(),
    })
}

fn verify_runtime(
    paths: &AppPaths,
    component: &str,
    root: &Path,
    cancellation: &Cancellation,
) -> Result<String, String> {
    cancellation.check()?;
    let cli = runtime_cli_path(root, component)
        .ok_or_else(|| "runtime component is not installable".to_string())?;
    if !cli.is_file() {
        return Err(format!("{component} runtime has no executable entry"));
    }
    let package = runtime_package(component).ok_or("unsupported runtime component")?;
    let manifest_version =
        fs::read_to_string(root.join("node_modules").join(package).join("package.json"))
            .ok()
            .and_then(|content| package_version_from_json(&content))
            .filter(|version| valid_package_version(version))
            .ok_or("runtime package manifest has no valid version")?;
    let mut command = Command::new(paths.node_executable());
    command.arg(cli).arg("--version");
    if component == "pi" {
        command.env("PI_CODING_AGENT_DIR", &paths.pi_home);
    } else {
        command.env("DSH_HOME", &paths.dsh_home);
    }
    let actual = command_version_cancellable(&mut command, Some(cancellation))
        .ok_or_else(|| format!("{component} runtime failed its startup verification"))?;
    cancellation.check()?;
    if actual != manifest_version {
        return Err(format!(
            "{component} CLI version {actual} differs from manifest {manifest_version}"
        ));
    }
    Ok(actual)
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
    operation_id: String,
    component_id: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRollbackRequest {
    operation_id: String,
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

impl RuntimeOperationLock {
    pub(crate) fn acquire(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.0
            .try_lock()
            .map_err(|_| "runtime lifecycle operation is in progress".into())
    }
}

#[tauri::command]
pub async fn install_runtime(
    app: AppHandle,
    request: RuntimeInstallRequest,
) -> Result<RuntimeOperationResult, String> {
    let operation = app
        .state::<OperationManager>()
        .begin(&request.operation_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        install_runtime_inner(
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            request,
            &operation.token,
        )
    })
    .await
    .map_err(|error| format!("runtime install worker failed: {error}"))?
}

fn install_runtime_inner(
    lock: State<'_, RuntimeOperationLock>,
    paths: State<'_, AppPaths>,
    store: State<'_, TaskStore>,
    dsh_manager: State<'_, DshManager>,
    pty_manager: State<'_, crate::pty::PtyManager>,
    request: RuntimeInstallRequest,
    cancellation: &Cancellation,
) -> Result<RuntimeOperationResult, String> {
    let _guard = lock.acquire()?;
    cancellation.check()?;
    crate::snapshot::Snapshot::ensure_ready(&paths.backups)?;
    if !component_installable(&paths, &request.component_id) {
        return Err("该组件当前不可更新，请先满足其运行时兼容条件".into());
    }
    if !valid_package_version(&request.version) {
        return Err("runtime version is invalid".into());
    }
    ensure_runtime_idle(&request.component_id, &store, &dsh_manager, &pty_manager)?;
    let package = runtime_package(&request.component_id)
        .ok_or_else(|| "runtime component is not supported".to_string())?;
    let proxy = configured_update_proxy()?;
    let latest = latest_package_version(package, proxy.as_deref())?;
    cancellation.check()?;
    if latest != request.version {
        return Err("只能安装刚从官方 registry 验证的最新版本".into());
    }
    if request.component_id == "dshmarket" {
        let result = install_dshmarket(&paths, &request.version, cancellation)?;
        log::info!(
            "event=runtime_install component={} version={} status=active",
            request.component_id,
            request.version
        );
        return Ok(result);
    }

    let root = runtime_root(&paths, &request.component_id)?;
    let staging_root = root.join("versions");
    fs::create_dir_all(&staging_root)
        .map_err(|error| format!("failed to create runtime staging directory: {error}"))?;
    let staging = staging_root.join(format!("{}-{}", request.version, Uuid::new_v4()));
    fs::create_dir_all(&staging)
        .map_err(|error| format!("failed to create runtime staging directory: {error}"))?;
    let spec = format!("{package}@{}", request.version);
    if let Err(error) = npm_install(&staging, &spec, cancellation) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    let verify_version = |root: &Path| {
        let actual = verify_runtime(&paths, &request.component_id, root, cancellation)?;
        if actual != request.version {
            return Err(format!(
                "runtime version mismatch: expected {}, got {actual}",
                request.version
            ));
        }
        Ok(actual)
    };
    if let Err(error) = verify_version(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    crate::runtime_pointer::activate(&root, &staging, |candidate| {
        let version = verify_version(candidate)?;
        cancellation.commit()?;
        Ok(version)
    })?;
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
pub async fn rollback_runtime(
    app: AppHandle,
    request: RuntimeRollbackRequest,
) -> Result<RuntimeOperationResult, String> {
    let operation = app
        .state::<OperationManager>()
        .begin(&request.operation_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        rollback_runtime_inner(
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            request,
            &operation.token,
        )
    })
    .await
    .map_err(|error| format!("runtime rollback worker failed: {error}"))?
}

fn rollback_runtime_inner(
    lock: State<'_, RuntimeOperationLock>,
    paths: State<'_, AppPaths>,
    store: State<'_, TaskStore>,
    dsh_manager: State<'_, DshManager>,
    pty_manager: State<'_, crate::pty::PtyManager>,
    request: RuntimeRollbackRequest,
    cancellation: &Cancellation,
) -> Result<RuntimeOperationResult, String> {
    let _guard = lock.acquire()?;
    cancellation.check()?;
    crate::snapshot::Snapshot::ensure_ready(&paths.backups)?;
    if !runtime_installable(&request.component_id) {
        return Err("该组件没有可回滚的独立运行时".into());
    }
    ensure_runtime_idle(&request.component_id, &store, &dsh_manager, &pty_manager)?;
    if request.component_id == "dshmarket" {
        let backup = latest_dshmarket_backup(&paths)
            .ok_or_else(|| "没有可用的 dshmarket 回滚版本".to_string())?;
        let package = paths.dshmarket_package();
        let backups = dshmarket_backup_root(&paths);
        let failed = backups.join(format!("dshmarket-rollback-{}", Uuid::new_v4()));
        let snapshot =
            crate::snapshot::Snapshot::capture(std::slice::from_ref(&package), &paths.backups)?;
        let result = activate_directory(&package, &backup, &failed, || {
            let version = fs::read_to_string(paths.dshmarket_manifest())
                .ok()
                .and_then(|content| package_version_from_json(&content))
                .ok_or_else(|| "dshmarket rollback failed its version verification".to_string())?;
            cancellation.commit()?;
            Ok(version)
        });
        let version = match result {
            Ok(version) => {
                snapshot.commit()?;
                version
            }
            Err(error) => return Err(snapshot.restore_error(error)),
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
    let version =
        crate::runtime_pointer::rollback(&root, latest_runtime_backup(&root), |target| {
            let version = verify_runtime(&paths, &request.component_id, target, cancellation)?;
            cancellation.commit()?;
            Ok(version)
        })?;
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
pub async fn check_runtime_updates(app: AppHandle) -> Result<Vec<RuntimeUpdate>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        check_runtime_updates_inner(app.state(), app.state())
    })
    .await
    .map_err(|error| format!("runtime update worker failed: {error}"))?
}

fn check_runtime_updates_inner(
    cache: State<'_, UpdateCache>,
    paths: State<'_, AppPaths>,
) -> Result<Vec<RuntimeUpdate>, String> {
    if let Some(updates) = cache.get() {
        log::debug!("event=runtime_update_check status=cache_hit");
        return Ok(updates);
    }
    let started = Instant::now();
    let updates = runtime_status_for_paths(&paths)?
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
    #[ignore = "downloads the official Pi runtime into an isolated temporary profile"]
    fn installs_verifies_and_rolls_back_live_pi_in_isolation() {
        let root =
            std::env::temp_dir().join(format!("deeppi-live-runtime-{}", uuid::Uuid::new_v4()));
        let paths = crate::app_paths::AppPaths::from_roots(
            root.join("roaming"),
            root.join("local"),
            root.join("project"),
        )
        .unwrap();
        let runtime = paths.runtimes.join("pi");
        let staging = runtime.join("versions/first");
        std::fs::create_dir_all(&staging).unwrap();
        let proxy = super::configured_update_proxy().unwrap();
        let version =
            super::latest_package_version("@earendil-works/pi-coding-agent", proxy.as_deref())
                .unwrap();
        let cancellation = crate::operation::Cancellation::default();
        super::npm_install(
            &staging,
            &format!("@earendil-works/pi-coding-agent@{version}"),
            &cancellation,
        )
        .unwrap();
        let verified = super::verify_runtime(&paths, "pi", &staging, &cancellation).unwrap();
        assert_eq!(verified, version);
        crate::runtime_pointer::activate(&runtime, &staging, |candidate| {
            super::verify_runtime(&paths, "pi", candidate, &cancellation)
        })
        .unwrap();
        let first = paths.managed_pi_runtime().unwrap();
        let second = runtime.join("versions/second");
        super::npm_install(
            &second,
            &format!("@earendil-works/pi-coding-agent@{version}"),
            &cancellation,
        )
        .unwrap();
        crate::runtime_pointer::activate(&runtime, &second, |candidate| {
            super::verify_runtime(&paths, "pi", candidate, &cancellation)
        })
        .unwrap();
        assert_ne!(paths.managed_pi_runtime().unwrap(), first);
        crate::runtime_pointer::rollback(&runtime, None, |candidate| {
            super::verify_runtime(&paths, "pi", candidate, &cancellation)
        })
        .unwrap();
        assert_eq!(paths.managed_pi_runtime().unwrap(), first);
        eprintln!("isolated Pi install, repair, and rollback verified: {version}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_lock_rejects_overlapping_operations() {
        let lifecycle = super::RuntimeOperationLock::default();
        let active = lifecycle.acquire().unwrap();
        assert!(lifecycle.acquire().is_err());
        drop(active);
        assert!(lifecycle.acquire().is_ok());
    }

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
