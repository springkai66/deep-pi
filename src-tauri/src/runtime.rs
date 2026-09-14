use std::{
    cmp::Ordering,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

/// v1.1 兼容基线：0.1.5-rc.2。启动 URL 带 launch token，Host API 需要先
/// 用 token 交换签名 cookie（`dsh-auth-…`），且 RPC 端点改为
/// `POST /api/<namespace>/<method>` 信封（`{args:{…}}`）；宿主已适配。
const VERIFIED_DSH_VERSION: &str = "0.1.5-rc.2";

/// DeepPi 托管的 Node 版本。Pi、DSH、dshmarket 均由它执行；
/// 不使用电脑上安装的 Node/npm。版本变更需与兼容矩阵一起验证。
const MANAGED_NODE_VERSION: &str = "24.13.0";
const NODE_DIST_BASE: &str = "https://nodejs.org/dist";

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
    let node = paths.node_runtime().ok();
    let node_version = node
        .as_ref()
        .and_then(|node| command_version(Command::new(node).arg("--version")));

    // 只读托管运行时：托管 Node 缺失时 Pi/DSH 一律显示不可用，
    // 不回落 PATH、全局 npm 或电脑上安装的 pi。
    let pi_cli = paths.pi_cli().ok().flatten();
    let pi_version = match (&node, &pi_cli) {
        (Some(node), Some(cli)) => {
            let mut command = Command::new(node);
            command
                .arg(cli)
                .arg("--version")
                .env("PI_CODING_AGENT_DIR", &paths.pi_home);
            command_version(&mut command)
        }
        _ => None,
    };

    let dsh_runtime = paths.dsh_runtime().ok();
    let dsh_version = match (&node, &dsh_runtime) {
        (Some(node), Some(runtime)) => {
            let mut command = Command::new(node);
            command
                .arg(AppPaths::dsh_cli_path(runtime))
                .arg("--version");
            command_version(&mut command)
        }
        _ => None,
    };
    let dsh_source = match &dsh_runtime {
        Some(runtime)
            if paths
                .managed_dsh_runtime()
                .is_ok_and(|managed| managed == *runtime) =>
        {
            "managed"
        }
        Some(_) => "development",
        None => "managed",
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
            id: "node",
            name: "Node.js",
            available: node_version.is_some(),
            current_version: node_version,
            source: "managed",
        },
        RuntimeComponent {
            id: "pi",
            name: "Pi Coding Agent",
            available: pi_version.is_some(),
            current_version: pi_version,
            source: "managed",
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
    /// 版本说明（例如上游有更新但尚未通过兼容验证）。
    pub note: Option<String>,
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
        "node" | "pi" | "dsh" => Ok(paths.runtimes.join(component)),
        _ => Err("only Node, Pi and DSH runtimes can be replaced".into()),
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
    matches!(component, "node" | "pi" | "dsh" | "dshmarket")
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
    let pi_busy = || -> Result<bool, String> {
        Ok(pty_manager.is_running()?
            || store.list()?.iter().any(|task| {
                task.agent == "pi"
                    && matches!(
                        task.status,
                        TaskStatus::Queued | TaskStatus::Running | TaskStatus::Waiting
                    )
            }))
    };
    if component == "pi" && pi_busy()? {
        return Err("请先停止所有 Pi Session，再切换 Pi runtime".into());
    }
    if component == "node" && (pi_busy()? || dsh_manager.is_running()?) {
        return Err("请先停止所有 Pi 任务并关闭 DSH，再修复 Node 运行时".into());
    }
    if matches!(component, "dsh" | "dshmarket") && dsh_manager.is_running()? {
        return Err("请先关闭 DSH，再切换 DSH runtime".into());
    }
    Ok(())
}

/// 只用托管 Node 自带的 npm；不回落到电脑上全局安装的 npm。
fn npm_install(
    paths: &AppPaths,
    prefix: &Path,
    spec: &str,
    cancellation: &Cancellation,
) -> Result<(), String> {
    let prefix = prefix.to_string_lossy().into_owned();
    let proxy = configured_update_proxy()?;
    let npm = paths.npm_runtime()?;
    let mut command = Command::new(npm);
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
        // 安装预算取安全网而非预期时长：DSH 依赖树很大（慢网下可超过 10 分钟），
        // 但仍可由“取消”立即中断。
        Duration::from_secs(1800),
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
    if let Err(error) = npm_install(paths, &staging, &spec, cancellation) {
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
    let installed = npm_install(paths, &profile, &spec, cancellation).and_then(|()| {
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

/// 下载 Node 用于校验的 SHA-256 清单/小文件。
fn download_node_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| format!("下载失败：{error}"))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(|error| format!("下载响应读取失败：{error}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("下载返回 HTTP {status}"));
    }
    Ok(body)
}

/// 流式下载到文件，同时计算 SHA-256（不把整个归档读进内存）。
fn download_node_archive(
    agent: &ureq::Agent,
    url: &str,
    destination: &Path,
    cancellation: &Cancellation,
    on_progress: Option<&dyn Fn(u8)>,
) -> Result<String, String> {
    cancellation.check()?;
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("下载失败：{error}"))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("下载返回 HTTP {status}"));
    }
    // Node 官方发行包带 Content-Length，可据此计算百分比。
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|total| *total > 0);
    let mut reader = response.into_body().into_reader();
    let mut file =
        fs::File::create(destination).map_err(|error| format!("无法创建下载文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 128 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_reported: u8 = 0;
    loop {
        cancellation.check()?;
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("下载读取失败：{error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| format!("下载写入失败：{error}"))?;
        if let Some(on_progress) = on_progress {
            downloaded += read as u64;
            if let Some(total) = total {
                let percent = ((downloaded.min(total) * 100) / total) as u8;
                // 每 5% 上报一次，避免事件风暴。
                if percent >= last_reported + 5 || percent == 100 {
                    last_reported = percent;
                    on_progress(percent);
                }
            }
        }
    }
    file.sync_all()
        .map_err(|error| format!("下载落盘失败：{error}"))?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn parse_sha256(contents: &str, file_name: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == file_name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// 解压官方 Node 归档，去掉顶层 `node-vX-win-x64/` 目录，并拒绝越界条目。
fn extract_node_archive(
    archive: &Path,
    destination: &Path,
    cancellation: &Cancellation,
) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|error| format!("无法打开 Node 归档：{error}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| format!("Node 归档无效：{error}"))?;
    for index in 0..zip.len() {
        cancellation.check()?;
        let mut entry = zip
            .by_index(index)
            .map_err(|error| format!("Node 归档读取失败：{error}"))?;
        let Some(path) = entry.enclosed_name() else {
            return Err("Node 归档包含非法路径".into());
        };
        let relative: PathBuf = path.components().skip(1).collect();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut output = fs::File::create(&target).map_err(|error| error.to_string())?;
            std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn verify_node(root: &Path, expected: &str) -> Result<String, String> {
    let node = root.join("node.exe");
    if !node.is_file() {
        return Err("Node 运行时缺少 node.exe".into());
    }
    if !root.join("npm.cmd").is_file() {
        return Err("Node 运行时缺少 npm.cmd（安装 Pi/DSH 需要托管 npm）".into());
    }
    let version = command_version(Command::new(&node).arg("--version"))
        .ok_or_else(|| "Node 运行时启动验证失败".to_string())?;
    if version != expected {
        return Err(format!("Node 版本不符：期望 {expected}，实际 {version}"));
    }
    Ok(version)
}

/// 安装内置的托管 Node。下载官方发行包、校对 SHA-256、解压、验证后原子切换；
/// 不使用电脑上已安装的 Node/npm。
fn install_node(
    paths: &AppPaths,
    version: &str,
    cancellation: &Cancellation,
    progress: &dyn RuntimeProgressSink,
) -> Result<RuntimeOperationResult, String> {
    if version != MANAGED_NODE_VERSION {
        return Err(format!(
            "Node 运行时只能安装内置固定版本 {MANAGED_NODE_VERSION}"
        ));
    }
    let root = paths.runtimes.join("node");
    let versions = root.join("versions");
    fs::create_dir_all(&versions)
        .map_err(|error| format!("failed to create Node runtime directory: {error}"))?;
    let staging = versions.join(format!("{version}-{}", Uuid::new_v4()));
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let archive_name = format!("node-v{version}-win-x64.zip");
    let archive_path = versions.join(format!("node-download-{}.zip", Uuid::new_v4()));
    let install = (|| -> Result<(), String> {
        let proxy = configured_update_proxy()?;
        let mut config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(900)))
            .http_status_as_error(false);
        if let Some(proxy) = proxy {
            let proxy =
                ureq::Proxy::new(&proxy).map_err(|error| format!("proxy is invalid: {error}"))?;
            config = config.proxy(Some(proxy));
        }
        let agent = config.build().new_agent();
        let shasums = download_node_text(
            &agent,
            &format!("{NODE_DIST_BASE}/v{version}/SHASUMS256.txt"),
        )?;
        let expected = parse_sha256(&shasums, &archive_name)
            .ok_or_else(|| format!("官方校验文件缺少 {archive_name}"))?;
        progress.report("下载 Node 官方发行包", Some(0));
        let digest = download_node_archive(
            &agent,
            &format!("{NODE_DIST_BASE}/v{version}/{archive_name}"),
            &archive_path,
            cancellation,
            Some(&|percent| progress.report("下载 Node 官方发行包", Some(percent))),
        )?;
        progress.report("校验 SHA-256", None);
        if digest != expected {
            return Err("Node 归档的 SHA-256 校验失败".into());
        }
        extract_node_archive(&archive_path, &staging, cancellation)?;
        verify_node(&staging, version)?;
        Ok(())
    })();
    let _ = fs::remove_file(&archive_path);
    if let Err(error) = install {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    crate::runtime_pointer::activate(&root, &staging, |candidate| {
        let version = verify_node(candidate, MANAGED_NODE_VERSION)?;
        cancellation.commit()?;
        Ok(version)
    })?;
    Ok(RuntimeOperationResult {
        component_id: "node".into(),
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
    let mut command = Command::new(paths.node_runtime()?);
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
    let progress = InstallProgressReporter::new(app.clone(), &request.operation_id);
    tauri::async_runtime::spawn_blocking(move || {
        install_runtime_inner(
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            request,
            &operation.token,
            &progress,
        )
    })
    .await
    .map_err(|error| format!("runtime install worker failed: {error}"))?
}

/// 运行时安装进度上报：通过 `runtime-progress` 事件推送给前端。
/// 阶段为人类可读文案；百分比在可计算的阶段给出（下载/解压）。
pub struct InstallProgressReporter {
    app: AppHandle,
    operation_id: String,
}

impl InstallProgressReporter {
    fn new(app: AppHandle, operation_id: &str) -> Self {
        Self {
            app,
            operation_id: operation_id.to_owned(),
        }
    }

    pub fn report(&self, phase: &str, percent: Option<u8>) {
        use tauri::Emitter;
        let _ = self.app.emit(
            "runtime-progress",
            crate::operation::OperationProgress {
                operation_id: self.operation_id.clone(),
                phase: phase.to_owned(),
                percent: percent.map(|value| value.clamp(0, 100)),
            },
        );
    }
}

/// 进度上报的最小接口，便于测试注入空实现。
pub trait RuntimeProgressSink: Send + Sync {
    fn report(&self, phase: &str, percent: Option<u8>);
}

impl RuntimeProgressSink for InstallProgressReporter {
    fn report(&self, phase: &str, percent: Option<u8>) {
        Self::report(self, phase, percent)
    }
}

/// 测试专用的空进度上报。
#[cfg(test)]
struct NullProgressSink;

#[cfg(test)]
impl RuntimeProgressSink for NullProgressSink {
    fn report(&self, _phase: &str, _percent: Option<u8>) {}
}

#[allow(clippy::too_many_arguments)]
fn install_runtime_inner(
    lock: State<'_, RuntimeOperationLock>,
    paths: State<'_, AppPaths>,
    store: State<'_, TaskStore>,
    dsh_manager: State<'_, DshManager>,
    pty_manager: State<'_, crate::pty::PtyManager>,
    request: RuntimeInstallRequest,
    cancellation: &Cancellation,
    progress: &dyn RuntimeProgressSink,
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
    progress.report(&format!("准备安装 {}", request.version), None);
    ensure_runtime_idle(&request.component_id, &store, &dsh_manager, &pty_manager)?;
    // Node 是自包含的基础运行时：从官方发行包下载，不从 npm 安装。
    if request.component_id == "node" {
        let result = install_node(&paths, &request.version, cancellation, progress)?;
        log::info!(
            "event=runtime_install component={} version={} status=active",
            request.component_id,
            request.version
        );
        return Ok(result);
    }
    let package = runtime_package(&request.component_id)
        .ok_or_else(|| "runtime component is not supported".to_string())?;
    progress.report("查询官方 registry", None);
    let proxy = configured_update_proxy()?;
    let registry_latest = latest_package_version(package, proxy.as_deref())?;
    cancellation.check()?;
    // DSH 固定兼容版本：即使通过 IPC 直接请求，也不能安装未验证的上游版本。
    if request.component_id == "dsh" && request.version != VERIFIED_DSH_VERSION {
        let requested = &request.version;
        return Err(format!(
            "DSH {requested} 尚未通过 DeepPi 兼容验证，请安装 {VERIFIED_DSH_VERSION}"
        ));
    }
    if registry_latest != request.version && request.component_id != "dsh" {
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
    if let Err(error) = npm_install(&paths, &staging, &spec, cancellation) {
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
            let version = if request.component_id == "node" {
                verify_node(target, MANAGED_NODE_VERSION)?
            } else {
                verify_runtime(&paths, &request.component_id, target, cancellation)?
            };
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
                // Node 是内置固定版本：缺失或版本不符时提供“安装/修复”入口。
                let is_node = component.id == "node";
                return RuntimeUpdate {
                    id: component.id.to_owned(),
                    name: component.name.to_owned(),
                    update_available: is_node
                        && component.current_version.as_deref() != Some(MANAGED_NODE_VERSION),
                    current_version: component.current_version,
                    latest_version: is_node.then(|| MANAGED_NODE_VERSION.to_owned()),
                    installable: component_installable(&paths, component.id),
                    can_rollback: runtime_backup_exists(&paths, component.id),
                    stale: false,
                    error: None,
                    note: None,
                };
            };
            match configured_update_proxy()
                .and_then(|proxy| latest_package_version(package, proxy.as_deref()))
            {
                Ok(registry_latest) => {
                    // DSH 固定在已验证版本：上游有更新时只提示，不提供安装。
                    let (latest_version, note) = if component.id == "dsh" {
                        (
                            VERIFIED_DSH_VERSION.to_owned(),
                            (registry_latest != VERIFIED_DSH_VERSION).then(|| {
                                format!(
                                    "上游已有 {registry_latest}；该版本尚未通过 DeepPi 兼容验证，暂时固定 {VERIFIED_DSH_VERSION}"
                                )
                            }),
                        )
                    } else {
                        (registry_latest, None)
                    };
                    RuntimeUpdate {
                        id: component.id.to_owned(),
                        name: component.name.to_owned(),
                        // DSH 固定版本：版本不一致（含高于兼容上限）时都提供安装入口，
                        // 让误装未验证版本的用户能回到已验证版本。
                        update_available: if component.id == "dsh" {
                            component.current_version.as_deref() != Some(latest_version.as_str())
                        } else {
                            component
                                .current_version
                                .as_deref()
                                .is_some_and(|current| version_is_newer(current, &latest_version))
                        },
                        current_version: component.current_version,
                        latest_version: Some(latest_version),
                        installable: component_installable(&paths, component.id),
                        can_rollback: runtime_backup_exists(&paths, component.id),
                        stale: false,
                        error: None,
                        note,
                    }
                }
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
                    note: None,
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
        extract_version, package_version_from_json, parse_sha256, runtime_cli_path,
        runtime_installable, valid_package_version, version_is_newer,
    };

    #[test]
    fn node_is_installable_and_sha256_lines_are_parsed() {
        assert!(runtime_installable("node"));
        assert!(runtime_installable("pi"));
        assert!(!runtime_installable("deeppi"));

        let contents = "\
            0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  node-v24.13.0-win-x64.zip\n\
            abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd *other.zip\n";
        assert_eq!(
            parse_sha256(contents, "node-v24.13.0-win-x64.zip"),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned())
        );
        // 去掉 BSD 风格的 `*` 前缀，以及拒绝非法行。
        assert_eq!(
            parse_sha256(contents, "other.zip"),
            Some("abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd".to_owned())
        );
        assert_eq!(parse_sha256(contents, "missing.zip"), None);
        assert_eq!(parse_sha256("not-a-hash  file.zip\n", "file.zip"), None);
        assert_eq!(parse_sha256("short  file.zip\n", "file.zip"), None);
    }

    #[test]
    #[ignore = "downloads the official Node and Pi runtimes into an isolated temporary profile"]
    fn installs_verifies_and_rolls_back_live_pi_in_isolation() {
        let root =
            std::env::temp_dir().join(format!("deeppi-live-runtime-{}", uuid::Uuid::new_v4()));
        let paths = crate::app_paths::AppPaths::from_roots(
            root.join("roaming"),
            root.join("local"),
            root.join("project"),
        )
        .unwrap();
        let cancellation = crate::operation::Cancellation::default();
        // 自包含流程：先装内置 Node（官方发行包），再用托管 npm 装 Pi。
        // 测试里不需要真实进度上报，用空实现占位。
        let progress = super::NullProgressSink;
        super::install_node(
            &paths,
            super::MANAGED_NODE_VERSION,
            &cancellation,
            &progress,
        )
        .unwrap();
        assert!(paths.node_runtime().unwrap().is_file());
        assert!(paths.npm_runtime().unwrap().is_file());

        let runtime = paths.runtimes.join("pi");
        let staging = runtime.join("versions/first");
        std::fs::create_dir_all(&staging).unwrap();
        let proxy = super::configured_update_proxy().unwrap();
        let version =
            super::latest_package_version("@earendil-works/pi-coding-agent", proxy.as_deref())
                .unwrap();
        super::npm_install(
            &paths,
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
            &paths,
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
