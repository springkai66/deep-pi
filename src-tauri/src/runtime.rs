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
    message::{msg, msg_with},
    operation::{Cancellation, OperationManager},
    process_runner,
    recovery::activate_directory,
    task::{TaskStatus, TaskStore},
};

const MIN_DSHMARKET_DSH_VERSION: &str = "0.1.1-rc.2";

/// 运行环境组件标识。前后端以这些字符串作为契约（`runtime_status` 的 `id`、
/// Tauri 命令的 `component_id`，以及前端 `update.id` 判断），因此集中定义：
/// 新增组件时不会因为漏改某一处 `match` 而静默走错分支。
const COMPONENT_DEEPPI: &str = "deeppi";
const COMPONENT_NODE: &str = "node";
const COMPONENT_PI: &str = "pi";
const COMPONENT_DSH: &str = "dsh";
const COMPONENT_DSHMARKET: &str = "dshmarket";

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
            id: COMPONENT_DEEPPI,
            name: "DeepPi",
            available: true,
            current_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            source: "managed",
        },
        RuntimeComponent {
            id: COMPONENT_NODE,
            name: "Node.js",
            available: node_version.is_some(),
            current_version: node_version,
            source: "managed",
        },
        RuntimeComponent {
            id: COMPONENT_PI,
            name: "Pi Coding Agent",
            available: pi_version.is_some(),
            current_version: pi_version,
            source: "managed",
        },
        RuntimeComponent {
            id: COMPONENT_DSH,
            name: "DeepSeek Harness",
            available: dsh_version.is_some(),
            current_version: dsh_version,
            source: dsh_source,
        },
        RuntimeComponent {
            id: COMPONENT_DSHMARKET,
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
        COMPONENT_PI => Some("@earendil-works/pi-coding-agent"),
        COMPONENT_DSH => Some("@deepseek-ai/dsh"),
        COMPONENT_DSHMARKET => Some(DSHMARKET_PACKAGE_NAME),
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
    // 全局代理设置（手动模式）、或跟随系统时读环境变量；直连模式返回 None。
    Ok(crate::proxy::current_url())
}

/// 按顺序尝试的 npm registry。npmmirror（阿里 CDN）国内最快、最稳，作为主源；
/// 官方源与腾讯镜像作为备用。镜像与官方同步，仅新版本发布后的短时间内可能滞后。
const REGISTRY_CANDIDATES: [&str; 3] = [
    "https://registry.npmmirror.com",
    "https://registry.npmjs.org",
    "https://mirrors.cloud.tencent.com/npm/",
];

/// Node 发行包镜像（npmmirror 托管同一份文件，SHA-256 校验仍然生效）。
const NODE_DIST_MIRROR: &str = "https://npmmirror.com/mirrors/node";

fn registry_label(registry: &str) -> &'static str {
    if registry.starts_with("https://registry.npmmirror.com") {
        "npmmirror"
    } else if registry.starts_with("https://registry.npmjs.org") {
        "npmjs"
    } else {
        "tencent"
    }
}

fn fetch_latest_from_registry(
    registry: &str,
    package: &str,
    proxy: Option<&str>,
) -> Result<String, String> {
    let encoded = package.replace('/', "%2f");
    let mut config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false);
    if let Some(proxy) = proxy {
        let proxy =
            ureq::Proxy::new(proxy).map_err(|error| format!("proxy is invalid: {error}"))?;
        config = config.proxy(Some(proxy));
    }
    let agent = config.build().new_agent();
    let mut body = String::new();
    crate::retry::retry_network(|_attempt| {
        let mut response = agent
            .get(&format!("{registry}/{encoded}"))
            .call()
            .map_err(|error| crate::retry::AttemptFailure {
                message: format!("update check failed: {error}"),
                retryable: crate::retry::is_retryable_ureq_error(&error),
                status: None,
                retry_after: None,
            })?;
        let status = response.status().as_u16();
        body = response
            .body_mut()
            .with_config()
            .limit(512 * 1024)
            .lossy_utf8(true)
            .read_to_string()
            .map_err(|error| crate::retry::AttemptFailure {
                message: format!("update response read failed: {error}"),
                retryable: true,
                status: None,
                retry_after: None,
            })?;
        if !(200..300).contains(&status) {
            return Err(crate::retry::AttemptFailure {
                message: format!("update check returned HTTP {status}"),
                retryable: crate::retry::is_retryable_status(status),
                status: Some(status),
                retry_after: None,
            });
        }
        Ok(())
    })
    .map_err(|failure| failure.message)?;
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

/// 查询包的最新版本：按候选源顺序尝试（npmmirror → 官方 → 腾讯），
/// 首个响应的源胜出。返回 `(版本, 实际使用的 registry)`，安装时优先走同一来源。
fn latest_package_version(
    package: &str,
    proxy: Option<&str>,
) -> Result<(String, &'static str), String> {
    let mut errors = Vec::new();
    for registry in REGISTRY_CANDIDATES {
        match fetch_latest_from_registry(registry, package, proxy) {
            Ok(version) => return Ok((version, registry)),
            Err(error) => errors.push(format!("{}: {error}", registry_label(registry))),
        }
    }
    Err(errors.join("; "))
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
        .find(|component| component.id == COMPONENT_DSH)
        .and_then(|component| component.current_version)
        .is_some_and(|version| {
            compare_versions(&version, MIN_DSHMARKET_DSH_VERSION) != Ordering::Less
        })
}

fn component_installable(paths: &AppPaths, component: &str) -> bool {
    runtime_installable(component)
        && (component != COMPONENT_DSHMARKET || dshmarket_is_compatible(paths))
}

fn runtime_root(paths: &AppPaths, component: &str) -> Result<PathBuf, String> {
    match component {
        COMPONENT_NODE | COMPONENT_PI | COMPONENT_DSH => Ok(paths.runtimes.join(component)),
        _ => Err("only Node, Pi and DSH runtimes can be replaced".into()),
    }
}

fn runtime_cli_path(root: &Path, component: &str) -> Option<PathBuf> {
    match component {
        COMPONENT_PI => Some(
            root.join("node_modules")
                .join("@earendil-works")
                .join("pi-coding-agent")
                .join("dist")
                .join("bundle")
                .join("cli.js"),
        ),
        COMPONENT_DSH => Some(
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
    matches!(
        component,
        COMPONENT_NODE | COMPONENT_PI | COMPONENT_DSH | COMPONENT_DSHMARKET
    )
}

fn runtime_backup_exists(paths: &AppPaths, component: &str) -> bool {
    if component == COMPONENT_DSHMARKET {
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
    if component == COMPONENT_PI && pi_busy()? {
        return Err(msg("runtime.pi.busy"));
    }
    if component == COMPONENT_NODE && (pi_busy()? || dsh_manager.is_running()?) {
        return Err(msg("runtime.node.busy"));
    }
    if matches!(component, COMPONENT_DSH | COMPONENT_DSHMARKET) && dsh_manager.is_running()? {
        return Err(msg("runtime.dsh.busy"));
    }
    Ok(())
}

/// 只用托管 Node 自带的 npm；不回落到电脑上全局安装的 npm。
/// `registry` 为空时使用 npm 默认源。安装期间由 [`npm_install_with_fallback`]
/// 提供心跳进度与失败后的换源重试。
fn npm_install_with_registry(
    paths: &AppPaths,
    prefix: &Path,
    spec: &str,
    registry: Option<&str>,
    cancellation: &Cancellation,
    progress: Option<&dyn RuntimeProgressSink>,
) -> Result<(), String> {
    let prefix = prefix.to_string_lossy().into_owned();
    let proxy = configured_update_proxy()?;
    let npm = paths.npm_runtime()?;
    // npm 默认缓存在用户目录（C 盘）；显式指到 DeepPi 自己的 cache 目录，
    // 安装/升级 Pi、DSH、dshmarket 时也不写 C 盘。
    let npm_cache = paths.cache.join("npm");
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
            // 单请求 30s 上限：死连接最多拖 30s 就报错，慢传输也能完成；
            // 失败后由上层自动换下一个源。
            "--fetch-timeout=30000",
            "--fetch-retries=1",
        ])
        .arg("--cache")
        .arg(&npm_cache);
    if let Some(registry) = registry {
        command.arg("--registry").arg(registry);
    }
    command
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
    // 心跳：大依赖树下载可能超过 1 分钟，界面阶段文本不刷新会让用户以为卡死。
    // 每 10s 上报一次已用时长；安装结束或取消后立即停止。
    let stop_heartbeat = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let output = std::thread::scope(|scope| {
        if let Some(progress) = progress {
            let stop = stop_heartbeat.clone();
            let label = registry.map(registry_label).unwrap_or("npm").to_owned();
            scope.spawn(move || {
                let started = std::time::Instant::now();
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_secs(10));
                    if stop.load(std::sync::atomic::Ordering::Relaxed)
                        || cancellation.check().is_err()
                    {
                        break;
                    }
                    let seconds = started.elapsed().as_secs().to_string();
                    progress.report(
                        &msg_with(
                            "runtime.npm.downloading",
                            &[("registry", label.as_str()), ("seconds", seconds.as_str())],
                        ),
                        None,
                    );
                }
            });
        }
        let output = process_runner::run_cancellable(
            &mut command,
            // 安装预算取安全网而非预期时长：DSH 依赖树很大（慢网下可超过 10 分钟），
            // 但仍可由“取消”立即中断。
            Duration::from_secs(1800),
            Some(cancellation),
        );
        stop_heartbeat.store(true, std::sync::atomic::Ordering::Relaxed);
        output
    })?;
    if output.status.success() {
        return Ok(());
    }
    let details = output.text();
    Err(format!("runtime installation failed: {details}"))
}

/// 安装 npm 包：按候选源顺序尝试（优先 `preferred`，通常是版本查询时
/// 实际响应的源），每个源失败后自动换下一个；全部失败才报错。
/// 官方 registry 在部分网络下不可达或极慢，这是“一直卡在查询/下载”的根因；
/// npmmirror（阿里 CDN）与官方同步，国内网络下快且稳。
fn npm_install_with_fallback(
    paths: &AppPaths,
    prefix: &Path,
    spec: &str,
    preferred: Option<&'static str>,
    cancellation: &Cancellation,
    progress: Option<&dyn RuntimeProgressSink>,
) -> Result<(), String> {
    let mut candidates: Vec<&'static str> = REGISTRY_CANDIDATES.to_vec();
    if let Some(preferred) = preferred {
        candidates.retain(|&registry| registry != preferred);
        candidates.insert(0, preferred);
    }
    let mut errors: Vec<String> = Vec::new();
    for (index, registry) in candidates.iter().enumerate() {
        if index > 0 {
            // 被用户取消时不再换源重试。
            cancellation.check()?;
            let previous = registry_label(candidates[index - 1]);
            if let Some(progress) = progress {
                progress.report(
                    &msg_with(
                        "runtime.npm.fallback",
                        &[("previous", previous), ("next", registry_label(registry))],
                    ),
                    None,
                );
            }
            log::warn!(
                "event=npm_install status=fallback registry={} reason=\"{}\"",
                registry,
                errors
                    .last()
                    .map_or_else(String::new, |error| error.clone())
            );
        }
        match npm_install_with_registry(paths, prefix, spec, Some(registry), cancellation, progress)
        {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("{}: {error}", registry_label(registry))),
        }
    }
    Err(errors.join("; "))
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

const DSHMARKET_PACKAGE_NAME: &str = "dshmarket";

/// 使 DSH profile 的 bundle 层列表与 dshmarket 的实际安装状态一致。
///
/// DSH 只加载 `package.json` 里 `dsh.profile.bundles` 列出的 bundle（`dsh
/// plugin add` 通过 pnpm 安装后按同样规则把包名追加进该列表）。DeepPi 用
/// 托管 npm 直接安装 dshmarket 到 profile，绕过了这一步：包体进了
/// `node_modules`，但 bundle 列表没有它，DSH 永远不会加载插件，设置里就
/// 看不到“插件市场”。此函数对齐 [`dsh plugin` 的 reconcile 语义]：以安装
/// 状态为准 —— node_modules 里有 dshmarket 就加入列表，没有就移除残留项。
///
/// 只处理能解析且 `dsh.profile.bundles` 是字符串数组的 manifest；其余形状
/// 原样不动并报错（由调用方决定是否降级为警告）。
///
/// 返回 manifest 是否被修改。
pub(crate) fn reconcile_dshmarket_bundle(profile: &Path) -> Result<bool, String> {
    let manifest = profile.join("package.json");
    let Ok(content) = fs::read_to_string(&manifest) else {
        return Ok(false);
    };
    let installed = profile
        .join("node_modules")
        .join(DSHMARKET_PACKAGE_NAME)
        .join("package.json")
        .is_file();
    let mut manifest_json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|error| format!("dsh profile manifest is not valid JSON: {error}"))?;
    let Some(bundles) = manifest_json
        .get("dsh")
        .and_then(|dsh| dsh.get("profile"))
        .and_then(|profile| profile.get("bundles"))
        .and_then(serde_json::Value::as_array)
    else {
        return Ok(false);
    };
    // 注意：这里不能用“manifest 文本里是否出现包名”作为快速路径。
    // package.json 的 `dependencies` 中可以合法地列出 dshmarket 而 `bundles`
    // 里没有它 —— 这正是 `dsh plugin add` 安装后、reconcile 前的状态，也是
    // 本函数最需要修的情况；文本匹配会把这种状态误判为“已一致”。
    let Some(mut names) = bundles
        .iter()
        .map(|entry| entry.as_str().map(ToOwned::to_owned))
        .collect::<Option<Vec<String>>>()
    else {
        return Ok(false);
    };
    match (
        installed,
        names.iter().position(|name| name == DSHMARKET_PACKAGE_NAME),
    ) {
        (true, None) => names.push(DSHMARKET_PACKAGE_NAME.to_owned()),
        (false, Some(position)) => {
            names.remove(position);
        }
        _ => return Ok(false),
    }
    manifest_json["dsh"]["profile"]["bundles"] = serde_json::Value::Array(
        names
            .into_iter()
            .map(serde_json::Value::String)
            .collect::<Vec<_>>(),
    );
    let serialized = serde_json::to_vec_pretty(&manifest_json)
        .map_err(|error| format!("failed to serialize dsh profile manifest: {error}"))?;
    let mut file = AtomicWriteFile::open(&manifest)
        .map_err(|error| format!("failed to open dsh profile manifest: {error}"))?;
    file.write_all(&serialized)
        .map_err(|error| format!("failed to write dsh profile manifest: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to commit dsh profile manifest: {error}"))?;
    Ok(true)
}

fn install_dshmarket(
    paths: &AppPaths,
    version: &str,
    cancellation: &Cancellation,
    progress: &dyn RuntimeProgressSink,
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
    if let Err(error) = npm_install_with_fallback(
        paths,
        &staging,
        &spec,
        Some(REGISTRY_CANDIDATES[0]),
        cancellation,
        Some(progress),
    ) {
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
    let installed = npm_install_with_fallback(
        paths,
        &profile,
        &spec,
        Some(REGISTRY_CANDIDATES[0]),
        cancellation,
        Some(progress),
    )
    .and_then(|()| reconcile_dshmarket_bundle(&profile).map(|_| ()))
    .and_then(|()| {
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
    let mut body = String::new();
    crate::retry::retry_network(|_attempt| {
        let mut response = agent
            .get(url)
            .call()
            .map_err(|error| crate::retry::AttemptFailure {
                message: msg_with(
                    "runtime.node.download_failed",
                    &[("detail", &error.to_string())],
                ),
                retryable: crate::retry::is_retryable_ureq_error(&error),
                status: None,
                retry_after: None,
            })?;
        let status = response.status().as_u16();
        body = response
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .lossy_utf8(true)
            .read_to_string()
            .map_err(|error| crate::retry::AttemptFailure {
                message: msg_with(
                    "runtime.node.download_read_failed",
                    &[("detail", &error.to_string())],
                ),
                retryable: true,
                status: None,
                retry_after: None,
            })?;
        if !(200..300).contains(&status) {
            return Err(crate::retry::AttemptFailure {
                message: msg_with(
                    "runtime.node.download_http_failed",
                    &[("status", &status.to_string())],
                ),
                retryable: crate::retry::is_retryable_status(status),
                status: Some(status),
                retry_after: None,
            });
        }
        Ok(())
    })
    .map_err(|failure| failure.message)?;
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
    // 连接建立带重试（网络瞬断）；流式下载中断不自动重放，交由用户重试。
    let response = crate::retry::retry_network(|_attempt| {
        agent
            .get(url)
            .call()
            .map_err(|error| crate::retry::AttemptFailure {
                message: msg_with(
                    "runtime.node.download_failed",
                    &[("detail", &error.to_string())],
                ),
                retryable: crate::retry::is_retryable_ureq_error(&error),
                status: None,
                retry_after: None,
            })
    })
    .map_err(|failure| failure.message)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(msg_with(
            "runtime.node.download_http_failed",
            &[("status", &status.to_string())],
        ));
    }
    // Node 官方发行包带 Content-Length，可据此计算百分比。
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|total| *total > 0);
    let mut reader = response.into_body().into_reader();
    let mut file = fs::File::create(destination).map_err(|error| {
        msg_with(
            "runtime.node.download_file_create_failed",
            &[("detail", &error.to_string())],
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 128 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_reported: u8 = 0;
    loop {
        cancellation.check()?;
        let read = reader.read(&mut buffer).map_err(|error| {
            msg_with(
                "runtime.node.download_stream_read_failed",
                &[("detail", &error.to_string())],
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|error| {
            msg_with(
                "runtime.node.download_write_failed",
                &[("detail", &error.to_string())],
            )
        })?;
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
    file.sync_all().map_err(|error| {
        msg_with(
            "runtime.node.download_flush_failed",
            &[("detail", &error.to_string())],
        )
    })?;
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
    let file = fs::File::open(archive).map_err(|error| {
        msg_with(
            "runtime.node.archive_open_failed",
            &[("detail", &error.to_string())],
        )
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| {
        msg_with(
            "runtime.node.archive_invalid",
            &[("detail", &error.to_string())],
        )
    })?;
    for index in 0..zip.len() {
        cancellation.check()?;
        let mut entry = zip.by_index(index).map_err(|error| {
            msg_with(
                "runtime.node.archive_read_failed",
                &[("detail", &error.to_string())],
            )
        })?;
        let Some(path) = entry.enclosed_name() else {
            return Err(msg("runtime.node.archive_unsafe_path"));
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
        return Err(msg("runtime.node.exe_missing"));
    }
    if !root.join("npm.cmd").is_file() {
        return Err(msg("runtime.node.npm_missing"));
    }
    let version = command_version(Command::new(&node).arg("--version"))
        .ok_or_else(|| msg("runtime.node.verify_failed"))?;
    if version != expected {
        return Err(msg_with(
            "runtime.node.version_mismatch",
            &[("expected", expected), ("actual", &version)],
        ));
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
        return Err(msg_with(
            "runtime.node.fixed_version",
            &[("version", MANAGED_NODE_VERSION)],
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
        // SHASUMS 是小文件：先试官方源（短超时），不可达时回落镜像；
        // 之后归档下载也走同一来源，SHA-256 校验始终对内容本身生效。
        let mut shasums_errors = Vec::new();
        let mut resolved = None;
        for base in [NODE_DIST_BASE, NODE_DIST_MIRROR] {
            match download_node_text(&agent, &format!("{base}/v{version}/SHASUMS256.txt")) {
                Ok(text) => {
                    resolved = Some((base, text));
                    break;
                }
                Err(error) => shasums_errors.push(format!("{base}: {error}")),
            }
        }
        let Some((base, shasums)) = resolved else {
            return Err(shasums_errors.join("; "));
        };
        let expected = parse_sha256(&shasums, &archive_name).ok_or_else(|| {
            msg_with(
                "runtime.node.checksum_entry_missing",
                &[("file", archive_name.as_str())],
            )
        })?;
        progress.report(&msg("runtime.node.downloading_package"), Some(0));
        let digest = download_node_archive(
            &agent,
            &format!("{base}/v{version}/{archive_name}"),
            &archive_path,
            cancellation,
            Some(&|percent| {
                progress.report(&msg("runtime.node.downloading_package"), Some(percent))
            }),
        )?;
        progress.report(&msg("runtime.node.verifying_checksum"), None);
        if digest != expected {
            return Err(msg("runtime.node.checksum_mismatch"));
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
    if component == COMPONENT_PI {
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
        return Err(msg("runtime.install.component_not_ready"));
    }
    if !valid_package_version(&request.version) {
        return Err("runtime version is invalid".into());
    }
    progress.report(
        &msg_with(
            "runtime.install.preparing",
            &[("version", request.version.as_str())],
        ),
        None,
    );
    ensure_runtime_idle(&request.component_id, &store, &dsh_manager, &pty_manager)?;
    // Node 是自包含的基础运行时：从官方发行包下载，不从 npm 安装。
    if request.component_id == COMPONENT_NODE {
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
    progress.report(&msg("runtime.install.checking_registry"), None);
    let proxy = configured_update_proxy()?;
    let (registry_latest, registry_used) = latest_package_version(package, proxy.as_deref())?;
    cancellation.check()?;
    // DSH 固定兼容版本：即使通过 IPC 直接请求，也不能安装未验证的上游版本。
    if request.component_id == COMPONENT_DSH && request.version != VERIFIED_DSH_VERSION {
        let requested = &request.version;
        return Err(msg_with(
            "runtime.dsh.unsupported",
            &[
                ("requested", requested.as_str()),
                ("verified", VERIFIED_DSH_VERSION),
            ],
        ));
    }
    if registry_latest != request.version && request.component_id != COMPONENT_DSH {
        return Err(msg("runtime.install.not_latest"));
    }
    if request.component_id == COMPONENT_DSHMARKET {
        let result = install_dshmarket(&paths, &request.version, cancellation, progress)?;
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
    if let Err(error) = npm_install_with_fallback(
        &paths,
        &staging,
        &spec,
        Some(registry_used),
        cancellation,
        Some(progress),
    ) {
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
        return Err(msg("runtime.rollback.not_available"));
    }
    ensure_runtime_idle(&request.component_id, &store, &dsh_manager, &pty_manager)?;
    if request.component_id == COMPONENT_DSHMARKET {
        let backup =
            latest_dshmarket_backup(&paths).ok_or_else(|| msg("runtime.dshmarket.no_backup"))?;
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
            let version = if request.component_id == COMPONENT_NODE {
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
                let is_node = component.id == COMPONENT_NODE;
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
                .map(|(version, _registry)| version)
            {
                Ok(registry_latest) => {
                    // DSH 固定在已验证版本：上游有更新时只提示，不提供安装。
                    let (latest_version, note) = if component.id == COMPONENT_DSH {
                        (
                            VERIFIED_DSH_VERSION.to_owned(),
                            (registry_latest != VERIFIED_DSH_VERSION).then(|| {
                                msg_with(
                                    "runtime.dsh.pinned",
                                    &[
                                        ("latest", registry_latest.as_str()),
                                        ("pinned", VERIFIED_DSH_VERSION),
                                    ],
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
                        update_available: if component.id == COMPONENT_DSH {
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
    use std::path::{Path, PathBuf};

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
        let (version, registry_used) =
            super::latest_package_version("@earendil-works/pi-coding-agent", proxy.as_deref())
                .unwrap();
        super::npm_install_with_fallback(
            &paths,
            &staging,
            &format!("@earendil-works/pi-coding-agent@{version}"),
            Some(registry_used),
            &cancellation,
            Some(&progress),
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
        super::npm_install_with_fallback(
            &paths,
            &second,
            &format!("@earendil-works/pi-coding-agent@{version}"),
            Some(registry_used),
            &cancellation,
            Some(&progress),
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

    fn profile_fixture(root: &Path, installed: bool, bundles: &str) -> PathBuf {
        let profile = root.join("profiles").join("web");
        std::fs::create_dir_all(profile.join("node_modules")).unwrap();
        let manifest = profile.join("package.json");
        std::fs::write(
            &manifest,
            format!(
                r#"{{"name":"dsh-profile-web","private":true,"dependencies":{{}},"dsh":{{"profile":{{"bundles":{bundles},"patchReload":"live"}}}}}}"#
            ),
        )
        .unwrap();
        if installed {
            let package = profile
                .join("node_modules")
                .join("dshmarket")
                .join("package.json");
            std::fs::create_dir_all(package.parent().unwrap()).unwrap();
            std::fs::write(
                package,
                r#"{"name":"dshmarket","version":"1.46.1","dsh":{"bundle":{"patch":"./cordis.patch.yml"}}}"#,
            )
            .unwrap();
        }
        profile
    }

    fn read_bundles(profile: &Path) -> Vec<String> {
        let manifest = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(profile.join("package.json")).unwrap(),
        )
        .unwrap();
        manifest["dsh"]["profile"]["bundles"]
            .as_array()
            .unwrap()
            .iter()
            .map(serde_json::Value::as_str)
            .map(Option::unwrap)
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn reconciles_dshmarket_listed_as_a_dependency_but_missing_from_bundles() {
        let root = std::env::temp_dir().join(format!("deeppi-reconcile-{}", uuid::Uuid::new_v4()));
        let profile = profile_fixture(
            &root,
            true,
            r#"["@deepseek-ai/dsh-base","@deepseek-ai/dsh-web-app"]"#,
        );
        // `dsh plugin add` 写依赖但尚未 reconcile 的状态：包名已在 dependencies 里、
        // bundles 里没有。任何基于“manifest 文本是否出现包名”的快速路径都会把
        // 这种状态误判为已一致，从而漏掉最需要修复的情况。
        let manifest = profile.join("package.json");
        std::fs::write(
            &manifest,
            r#"{"name":"dsh-profile-web","private":true,"dependencies":{"dshmarket":"^1.46.1"},"dsh":{"profile":{"bundles":["@deepseek-ai/dsh-base","@deepseek-ai/dsh-web-app"]}}}"#,
        )
        .unwrap();
        assert!(super::reconcile_dshmarket_bundle(&profile).unwrap());
        assert_eq!(read_bundles(&profile).last().unwrap(), "dshmarket");
        // 依赖声明原样保留。
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
        assert_eq!(value["dependencies"]["dshmarket"], "^1.46.1");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reconciles_installed_dshmarket_into_the_bundle_stack() {
        let root = std::env::temp_dir().join(format!("deeppi-reconcile-{}", uuid::Uuid::new_v4()));
        let profile = profile_fixture(
            &root,
            true,
            r#"["@deepseek-ai/dsh-base","@deepseek-ai/dsh-web-app"]"#,
        );
        assert!(super::reconcile_dshmarket_bundle(&profile).unwrap());
        assert_eq!(
            read_bundles(&profile),
            vec![
                "@deepseek-ai/dsh-base".to_owned(),
                "@deepseek-ai/dsh-web-app".to_owned(),
                "dshmarket".to_owned()
            ]
        );
        // 已一致时不再改写。
        assert!(!super::reconcile_dshmarket_bundle(&profile).unwrap());
        assert_eq!(read_bundles(&profile).last().unwrap(), "dshmarket");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reconciles_removed_dshmarket_out_of_the_bundle_stack() {
        let root = std::env::temp_dir().join(format!("deeppi-reconcile-{}", uuid::Uuid::new_v4()));
        let profile = profile_fixture(
            &root,
            false,
            r#"["@deepseek-ai/dsh-base","dshmarket","@deepseek-ai/dsh-web-app"]"#,
        );
        // node_modules 里没有 dshmarket 却在列表里：启动会因解析失败报错，应移除。
        assert!(super::reconcile_dshmarket_bundle(&profile).unwrap());
        assert_eq!(
            read_bundles(&profile),
            vec![
                "@deepseek-ai/dsh-base".to_owned(),
                "@deepseek-ai/dsh-web-app".to_owned()
            ]
        );
        assert!(!super::reconcile_dshmarket_bundle(&profile).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn leaves_manifests_without_a_bundle_stack_untouched() {
        let root = std::env::temp_dir().join(format!("deeppi-reconcile-{}", uuid::Uuid::new_v4()));
        let profile = profile_fixture(&root, true, r#"[]"#);
        let manifest = profile.join("package.json");
        // 缺少 dsh.profile.bundles（或形状异常）时不改写，避免覆盖未知结构。
        std::fs::write(&manifest, r#"{"name":"dsh-profile-web","private":true}"#).unwrap();
        assert!(!super::reconcile_dshmarket_bundle(&profile).unwrap());
        assert_eq!(
            std::fs::read_to_string(&manifest).unwrap(),
            r#"{"name":"dsh-profile-web","private":true}"#
        );
        // 非数组同样不动。
        std::fs::write(&manifest, r#"{"dsh":{"profile":{"bundles":"dshmarket"}}}"#).unwrap();
        assert!(!super::reconcile_dshmarket_bundle(&profile).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
}
