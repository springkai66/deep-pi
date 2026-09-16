use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Mutex},
    thread,
    time::Duration,
};

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{
    webview::{NewWindowResponse, WebviewBuilder},
    AppHandle, Emitter, Manager, State, Url, WebviewUrl,
};

use tauri_plugin_opener::OpenerExt;

use crate::{
    app_paths::AppPaths,
    dsh_api,
    message::{msg, msg_with},
    task::TaskStore,
};

#[derive(Default)]
struct DshProcess {
    child: Option<Child>,
    /// Kill-on-close Job Object：应用进程意外终止（含被强杀）时，
    /// 由系统结束 DSH 及其子进程，避免遗留持有端口的孤儿进程。
    tree: Option<crate::process_runner::ProcessTree>,
    url: Option<String>,
    /// 0.1.5 起的浏览器会话 cookie（`dsh-auth-…=…`），由启动 URL 的
    /// launch token 交换而来；Host API（session.list 等）必须携带。
    cookie: Option<String>,
    generation: u64,
}

#[derive(Default)]
pub struct DshManager {
    process: Mutex<DshProcess>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DshHostStatus {
    running: bool,
    url: Option<String>,
    error: Option<String>,
}

/// 解析 DSH 启动行。旧版输出 `dsh web: http://127.0.0.1:<port>`；
/// 0.1.5-rc.1 起额外带 token（`…:<port>/?token=…`）。整段 URL 都会返回：
/// Webview 需要 token 才能通过 DSH 的本地鉴权；API 调用会另行归一化到 origin。
fn extract_dsh_url(line: &str) -> Option<String> {
    let address = line.trim().strip_prefix("dsh web: ")?;
    let url = Url::parse(address.trim()).ok()?;
    if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") {
        return None;
    }
    let port = url.port().filter(|port| *port > 0)?;
    let _ = port;
    Some(url.to_string())
}

/// 把 DSH URL 归一化为 `http://127.0.0.1:<port>`，供 API 调用使用（丢弃 token 路径）。
pub(crate) fn dsh_api_base(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if parsed.scheme() != "http" || parsed.host_str() != Some("127.0.0.1") {
        return None;
    }
    let port = parsed.port().filter(|port| *port > 0)?;
    Some(format!("http://127.0.0.1:{port}"))
}

fn lock_error() -> String {
    "DSH process state is unavailable".into()
}

impl DshManager {
    pub(crate) fn is_running(&self) -> Result<bool, String> {
        let mut state = self.process.lock().map_err(|_| lock_error())?;
        let Some(child) = state.child.as_mut() else {
            return Ok(false);
        };
        if child
            .try_wait()
            .map_err(|error| format!("failed to inspect DSH: {error}"))?
            .is_none()
        {
            return Ok(true);
        }
        state.child = None;
        state.tree = None;
        state.url = None;
        Ok(false)
    }
}

/// DSH 启动失败诊断结果。
/// - `culprit`：导入失败型（插件与 DSH 版本不兼容）的可疑插件包名，可一键移除；
/// - `pending`：激活超时型（插件等待的服务不存在）的插件名，通常是运行时版本不匹配，
///   正确处理是更新 DSH 运行时或插件本身，而不是直接移除。
/// - `kind`：`import-failure` | `activation-pending` | `unknown`。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshDiagnosis {
    pub has_failure: bool,
    pub timestamp: Option<u64>,
    pub error: Option<String>,
    pub lines: Vec<String>,
    pub culprit: Option<String>,
    pub pending: Vec<String>,
    pub kind: Option<String>,
}

/// DSH 插件修复结果。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DshRepairReport {
    pub package: String,
    pub removed_from_dependencies: bool,
    pub removed_from_bundles: bool,
    pub node_modules_removed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DshRepairRequest {
    pub package: String,
}

fn dsh_log_dir(paths: &AppPaths) -> PathBuf {
    paths.logs.join("dsh")
}

fn dsh_failure_path(paths: &AppPaths) -> PathBuf {
    dsh_log_dir(paths).join("last-startup-failure.json")
}

fn dsh_repairs_path(paths: &AppPaths) -> PathBuf {
    dsh_log_dir(paths).join("repairs.jsonl")
}

/// 把最近一次启动失败的 stderr 行写入 logs/dsh/last-startup-failure.json（尽力而为）。
fn persist_startup_failure(paths: &AppPaths, error: &str, lines: &[String]) {
    let payload = json!({
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0),
        "error": error,
        "lines": lines,
    });
    if fs::create_dir_all(dsh_log_dir(paths)).is_err() {
        return;
    }
    let Ok(content) = serde_json::to_vec_pretty(&payload) else {
        return;
    };
    let mut file = match AtomicWriteFile::open(dsh_failure_path(paths)) {
        Ok(file) => file,
        Err(_) => return,
    };
    let _ = file.write_all(&content);
    let _ = file.commit();
}

fn is_official_dsh_package(name: &str) -> bool {
    name == "dshmarket"
        || name.starts_with("@deepseek-ai/")
        || name.starts_with("cordis")
        || name == "@cordis"
}

fn package_name_is_valid(name: &str) -> bool {
    crate::market::validate_package_name(name).is_ok()
}

/// 从启动失败的 stderr 摘要中定位肇事插件包名。
/// 优先匹配 `failed to import loader entry <id> (<pkg>)` 行；否则回退到
/// stderr 中的 node_modules 路径段（跳过官方运行时包）。
fn culprit_from_lines(lines: &[String]) -> Option<String> {
    for line in lines {
        // 同一行可能连续出现多个 `failed to import loader entry <id> (<pkg>)`
        // 嵌套错误链，需要逐个检查括号里的包名直到找到非官方的候选。
        let mut cursor = 0usize;
        while let Some(offset) = line[cursor..].find("failed to import loader entry") {
            let tail = &line[cursor + offset..];
            let open = tail.find('(');
            let close = tail.find(')');
            if let (Some(open), Some(close)) = (open, close) {
                if close > open {
                    let candidate = tail[open + 1..close].trim();
                    if !is_official_dsh_package(candidate) && package_name_is_valid(candidate) {
                        return Some(candidate.to_owned());
                    }
                }
            }
            cursor += offset + "failed to import loader entry".len();
        }
    }
    for line in lines {
        let mut cursor = 0usize;
        while cursor < line.len() {
            let Some(offset) = line[cursor..].find("node_modules") else {
                break;
            };
            let start = cursor + offset + "node_modules".len();
            let bytes = line.as_bytes();
            if start >= bytes.len() || (bytes[start] != b'/' && bytes[start] != b'\\') {
                cursor = start + 1;
                continue;
            }
            let rest = &line[start + 1..];
            let end = rest.find(['/', '\\']).unwrap_or(rest.len());
            let segment = &rest[..end];
            let candidate = if segment.starts_with('@') {
                if segment == "@deepseek-ai" || end >= rest.len() {
                    cursor = start + 1;
                    continue;
                }
                let after = &rest[end + 1..];
                let end2 = after.find(['/', '\\']).unwrap_or(after.len());
                format!("{segment}/{}", &after[..end2])
            } else {
                segment.to_owned()
            };
            if !is_official_dsh_package(&candidate) && package_name_is_valid(&candidate) {
                return Some(candidate);
            }
            cursor = start + 1;
        }
    }
    None
}

/// 从失败输出中提取“等待服务而未能激活”的插件名。
/// 该输出可能被压成一行（`pkg-a: pending (waiting for service: x)pkg-b: pending (...)`），
/// 因此这里按标记全局扫描，再向前回读包名。
fn pending_plugins_from_lines(lines: &[String]) -> Vec<String> {
    const MARKER: &str = ": pending (waiting for service:";
    let mut found: Vec<String> = Vec::new();
    for line in lines {
        let mut search_from = 0usize;
        while let Some(offset) = line[search_from..].find(MARKER) {
            let marker_at = search_from + offset;
            let name: String = line[..marker_at]
                .chars()
                .rev()
                .take_while(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '@' | '/' | '-' | '_' | '.')
                })
                .collect::<Vec<char>>()
                .into_iter()
                .rev()
                .collect();
            if !name.is_empty()
                && !is_official_dsh_package(&name)
                && package_name_is_valid(&name)
                && !found.iter().any(|existing| existing == &name)
            {
                found.push(name);
            }
            search_from = marker_at + MARKER.len();
        }
    }
    found
}

fn diagnosis_kind(
    lines: &[String],
    culprit: Option<&String>,
    pending: &[String],
) -> Option<String> {
    if !pending.is_empty() {
        return Some("activation-pending".into());
    }
    if culprit.is_some() {
        return Some("import-failure".into());
    }
    if lines.iter().any(|line| {
        line.contains("did not activate") || line.contains("plugin tree failed to load")
    }) {
        return Some("unknown".into());
    }
    None
}

fn diagnose_inner(paths: &AppPaths) -> DshDiagnosis {
    let empty = DshDiagnosis {
        has_failure: false,
        timestamp: None,
        error: None,
        lines: Vec::new(),
        culprit: None,
        pending: Vec::new(),
        kind: None,
    };
    let Ok(content) = fs::read_to_string(dsh_failure_path(paths)) else {
        return empty;
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return empty;
    };
    let lines = value
        .get("lines")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let culprit = culprit_from_lines(&lines);
    let pending = pending_plugins_from_lines(&lines);
    let kind = diagnosis_kind(&lines, culprit.as_ref(), &pending);
    DshDiagnosis {
        has_failure: true,
        timestamp: value.get("timestamp").and_then(Value::as_u64),
        error: value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_owned),
        culprit,
        pending,
        kind,
        lines,
    }
}

#[tauri::command]
pub async fn dsh_diagnose(app: AppHandle) -> Result<DshDiagnosis, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = app.state::<AppPaths>().inner().clone();
        diagnose_inner(&paths)
    })
    .await
    .map_err(|error| format!("DSH diagnosis worker failed: {error}"))
}

/// 修复插件导致的 DSH 启动失败：先快照，再从 web profile 的
/// dependencies/bundles/node_modules 中移除指定插件，失败可回滚。
fn repair_inner(app: &AppHandle, package: &str) -> Result<DshRepairReport, String> {
    let paths = app.state::<AppPaths>();
    let package = crate::market::validate_package_name(package)?;
    if is_official_dsh_package(package) {
        return Err(msg("dsh.repair.official_package"));
    }
    if app.state::<DshManager>().is_running()? {
        return Err(msg("dsh.repair.running"));
    }
    let profile = paths.dsh_profile();
    if !profile.is_dir() {
        return Err(msg("dsh.profile.missing"));
    }
    let manifest = profile.join("package.json");
    let targets = [manifest, profile.join("node_modules")];
    let snapshot = crate::snapshot::Snapshot::capture(&targets, &paths.backups)?;
    let result = remove_package_from_profile(&profile, package).and_then(|report| {
        append_repair_log(&dsh_repairs_path(&paths), package)?;
        Ok(report)
    });
    match result {
        Ok(report) => {
            snapshot.commit()?;
            Ok(report)
        }
        Err(error) => Err(snapshot.restore_error(error)),
    }
}

fn append_repair_log(path: &Path, package: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create DSH log directory: {error}"))?;
    }
    let entry = json!({
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0),
        "package": package,
        "action": "removed",
    });
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("failed to open DSH repair log: {error}"))?;
    writeln!(file, "{entry}").map_err(|error| format!("failed to write DSH repair log: {error}"))
}

#[tauri::command]
pub async fn dsh_repair(
    app: AppHandle,
    request: DshRepairRequest,
) -> Result<DshRepairReport, String> {
    tauri::async_runtime::spawn_blocking(move || repair_inner(&app, &request.package))
        .await
        .map_err(|error| format!("DSH repair worker failed: {error}"))?
}

/// 从 web profile 移除一个插件：dependencies、dsh.profile.bundles 与
/// node_modules/<pkg>（存在时）。pnpm 链接目录只移除链接本身。
fn remove_package_from_profile(profile: &Path, package: &str) -> Result<DshRepairReport, String> {
    let manifest = profile.join("package.json");
    let content = fs::read_to_string(&manifest)
        .map_err(|error| format!("failed to read DSH profile manifest: {error}"))?;
    let mut value: Value = serde_json::from_str(&content)
        .map_err(|error| format!("DSH profile manifest is not valid JSON: {error}"))?;
    let (removed_from_dependencies, removed_from_bundles) =
        strip_package_from_manifest(&mut value, package)?;
    if !removed_from_dependencies && !removed_from_bundles {
        return Err(msg_with(
            "dsh.repair.not_in_profile",
            &[("package", package)],
        ));
    }
    let serialized = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("failed to serialize DSH profile manifest: {error}"))?;
    let mut file = AtomicWriteFile::open(&manifest)
        .map_err(|error| format!("failed to open DSH profile manifest: {error}"))?;
    file.write_all(&serialized)
        .map_err(|error| format!("failed to write DSH profile manifest: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to commit DSH profile manifest: {error}"))?;

    let mut node_modules_removed = false;
    let entry = profile.join("node_modules").join(package);
    if entry.exists() {
        remove_node_modules_entry(&entry)?;
        node_modules_removed = true;
    }
    Ok(DshRepairReport {
        package: package.to_owned(),
        removed_from_dependencies,
        removed_from_bundles,
        node_modules_removed,
    })
}

fn strip_package_from_manifest(value: &mut Value, package: &str) -> Result<(bool, bool), String> {
    let mut removed_from_dependencies = false;
    let mut removed_from_bundles = false;
    if let Some(dependencies) = value.get_mut("dependencies").and_then(Value::as_object_mut) {
        removed_from_dependencies = dependencies.remove(package).is_some();
    }
    if let Some(bundles) = value
        .get_mut("dsh")
        .and_then(|dsh| dsh.get_mut("profile"))
        .and_then(|profile| profile.get_mut("bundles"))
        .and_then(Value::as_array_mut)
    {
        let before = bundles.len();
        bundles.retain(|entry| entry.as_str() != Some(package));
        removed_from_bundles = before != bundles.len();
    }
    Ok((removed_from_dependencies, removed_from_bundles))
}

fn remove_node_modules_entry(entry: &Path) -> Result<(), String> {
    // pnpm 在 Windows 上用 junction 链接包目录：绝不能递归进目标，只移除链接本身。
    let is_link = fs::symlink_metadata(entry)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false);
    let result = if is_link {
        fs::remove_dir(entry)
    } else {
        fs::remove_dir_all(entry)
    };
    result.map_err(|error| format!("failed to remove {}: {error}", entry.display()))
}

fn dsh_url_is_allowed(base_url: &str, candidate: &Url) -> bool {
    let Ok(base) = Url::parse(base_url) else {
        return false;
    };
    base.scheme() == candidate.scheme()
        && base.host_str() == candidate.host_str()
        && base.port_or_known_default() == candidate.port_or_known_default()
}

#[tauri::command]
pub async fn create_dsh_webview(
    app: AppHandle,
    base_url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let base = Url::parse(&base_url).map_err(|error| format!("invalid DSH URL: {error}"))?;
    if base.scheme() != "http" || base.host_str() != Some("127.0.0.1") || base.port().is_none() {
        return Err("DSH Webview URL must be a loopback HTTP URL with a port".into());
    }
    if !(x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite())
        || width <= 0.0
        || height <= 0.0
    {
        return Err("invalid DSH Webview bounds".into());
    }
    if app.get_webview("dsh").is_some() {
        return Ok(());
    }
    let window = app
        .get_window("main")
        .ok_or_else(|| "main window was not found".to_string())?;
    let navigation_base = base_url.clone();
    let navigation_opener = app.clone();
    let new_window_base = base_url.clone();
    let new_window_opener = app.clone();
    let builder = WebviewBuilder::new("dsh", WebviewUrl::External(base))
        .on_navigation(move |url| {
            if dsh_url_is_allowed(&navigation_base, url) {
                true
            } else {
                let _ = navigation_opener
                    .opener()
                    .open_url(url.as_str(), None::<&str>);
                false
            }
        })
        .on_new_window(move |url, _| {
            if !dsh_url_is_allowed(&new_window_base, &url) {
                let _ = new_window_opener
                    .opener()
                    .open_url(url.as_str(), None::<&str>);
            }
            NewWindowResponse::Deny
        });
    tauri::async_runtime::spawn_blocking(move || {
        window
            .add_child(
                builder,
                tauri::LogicalPosition::new(x, y),
                tauri::LogicalSize::new(width, height),
            )
            .map(|_| ())
            .map_err(|error| format!("failed to create DSH Webview: {error}"))
    })
    .await
    .map_err(|error| format!("failed to join DSH Webview creation: {error}"))?
}

fn start_dsh_inner(
    app: AppHandle,
    manager: &DshManager,
    paths: &AppPaths,
) -> Result<String, String> {
    let lifecycle = app.state::<crate::runtime::RuntimeOperationLock>();
    let _lifecycle = lifecycle.acquire()?;
    crate::snapshot::Snapshot::ensure_ready(&paths.backups)?;
    let mut state = manager.process.lock().map_err(|_| lock_error())?;
    if let Some(child) = state.child.as_mut() {
        if child
            .try_wait()
            .map_err(|error| format!("failed to inspect DSH: {error}"))?
            .is_none()
        {
            return state
                .url
                .clone()
                .ok_or_else(|| "DSH is running without a URL".into());
        }
    }
    state.child = None;
    state.tree = None;
    state.url = None;

    let runtime = paths.dsh_runtime()?;
    let cli = AppPaths::dsh_cli_path(&runtime);
    if !cli.is_file() {
        return Err("managed DSH runtime is not installed".into());
    }

    // 自愈：dshmarket 装进 profile 却没进 bundle 层列表（旧版安装路径遗留），
    // 或列表残留已卸载的包导致启动失败 —— 启动前按安装状态对齐一次。
    // 失败不阻断启动：降级为警告，DSH 仍按现状启动。
    match crate::runtime::reconcile_dshmarket_bundle(&paths.dsh_profile()) {
        Ok(true) => log::info!(
            "event=dsh_profile_reconcile component=dshmarket profile=web status=updated"
        ),
        Ok(false) => {}
        Err(error) => log::warn!(
            "event=dsh_profile_reconcile component=dshmarket profile=web status=skipped reason=\"{error}\""
        ),
    }

    let mut command = Command::new(paths.node_runtime()?);
    // 0.1.5 起 HMR 服务要求 Node 以 --expose-internals 启动，否则 DSH 无法就绪。
    command
        .arg("--expose-internals")
        .arg(cli)
        .args(["web", "--host", "127.0.0.1", "--port", "0", "--no-open"])
        .current_dir(&runtime)
        .env("DSH_HOME", &paths.dsh_home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // 由 Job Object 托管，应用异常结束时系统会一起结束 DSH。
    let (mut child, tree) = crate::process_runner::spawn_owned(&mut command)
        .map_err(|error| format!("failed to start DSH: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture DSH output".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture DSH errors".to_string())?;
    let (sender, receiver) = mpsc::sync_channel(1);
    // 启动失败信息分两路：导入/激活失败写在 stdout（插件加载器日志），
    // 崩溃栈写在 stderr。两边的末尾几行都要留作诊断依据。
    let (stderr_sender, stderr_receiver) = mpsc::sync_channel(1);
    let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ready_for_stderr = ready.clone();
    let stdout_tail = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let stdout_tail_writer = stdout_tail.clone();

    thread::spawn(move || {
        let mut sender = Some(sender);
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(mut tail) = stdout_tail_writer.lock() {
                if tail.len() >= 20 {
                    tail.remove(0);
                }
                tail.push(line.clone());
            }
            if let Some(url) = extract_dsh_url(&line) {
                if let Some(channel) = sender.take() {
                    let _ = channel.send(url);
                }
            }
        }
    });
    thread::spawn(move || {
        let mut collected: Vec<String> = Vec::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if ready_for_stderr.load(std::sync::atomic::Ordering::Relaxed) {
                continue;
            }
            // 保留最后 6 行用于失败诊断。
            if collected.len() >= 6 {
                collected.remove(0);
            }
            collected.push(line);
        }
        let _ = stderr_sender.send(collected);
    });

    let url = match receiver.recv_timeout(Duration::from_secs(30)) {
        Ok(url) => url,
        Err(error) => {
            ready.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = child.kill();
            let _ = child.wait();
            let stderr_lines: Vec<String> = stderr_receiver.try_recv().ok().unwrap_or_default();
            let stdout_lines: Vec<String> = stdout_tail
                .lock()
                .map(|tail| tail.clone())
                .unwrap_or_default();
            // stdout 在前（插件加载器日志），stderr 在后（崩溃栈）。
            let mut lines = stdout_lines;
            lines.extend(stderr_lines);
            let joined = lines.join("; ");
            let detail = (!joined.trim().is_empty()).then_some(joined);
            persist_startup_failure(paths, &error.to_string(), &lines);
            return Err(match detail {
                Some(detail) => msg_with(
                    "dsh.start.failed",
                    &[("error", &error.to_string()), ("detail", detail.as_str())],
                ),
                None => format!("DSH did not become ready: {error}"),
            });
        }
    };
    ready.store(true, std::sync::atomic::Ordering::Relaxed);
    // 启动成功即清除旧失败记录，避免诊断误报。
    let _ = fs::remove_file(dsh_failure_path(paths));

    state.generation = state.generation.wrapping_add(1);
    let generation = state.generation;
    state.child = Some(child);
    state.tree = Some(tree);
    state.url = Some(url.clone());
    // 0.1.5 起 Host API 需要 launch token 交换的签名 cookie；
    // 交换失败不阻塞启动，Webview 仍可用完整 URL 自行完成 token 换 cookie。
    state.cookie =
        dsh_api::exchange_browser_cookie(&dsh_api_base(&url).unwrap_or_else(|| url.clone()), &url)
            .ok()
            .flatten();
    drop(state);

    let _ = app.emit(
        "dsh-status",
        DshHostStatus {
            running: true,
            url: Some(url.clone()),
            error: None,
        },
    );
    monitor_dsh(app.clone(), generation, url.clone());
    Ok(url)
}

#[tauri::command]
pub async fn start_dsh(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        start_dsh_inner(app.clone(), &app.state(), &app.state())
    })
    .await
    .map_err(|error| format!("DSH start worker failed: {error}"))?
}

fn monitor_dsh(app: AppHandle, generation: u64, url: String) {
    thread::spawn(move || loop {
        let exit = app.try_state::<DshManager>().and_then(|manager| {
            let mut state = manager.process.lock().ok()?;
            if state.generation != generation {
                return Some(Ok(None));
            }
            let status = state.child.as_mut()?.try_wait();
            match status {
                Ok(None) => None,
                Ok(Some(status)) => {
                    state.child = None;
                    state.tree = None;
                    state.url = None;
                    state.cookie = None;
                    Some(Ok(Some(status.code())))
                }
                Err(error) => {
                    state.child = None;
                    state.tree = None;
                    state.url = None;
                    state.cookie = None;
                    Some(Err(error.to_string()))
                }
            }
        });
        if let Some(exit) = exit {
            let error = match exit {
                Ok(None) => return,
                Ok(Some(code)) => Some(format!("DSH exited with code {}", code.unwrap_or(-1))),
                Err(error) => Some(format!("failed to inspect DSH: {error}")),
            };
            let _ = app.emit(
                "dsh-status",
                DshHostStatus {
                    running: false,
                    url: None,
                    error,
                },
            );
            return;
        }

        let api_base = dsh_api_base(&url).unwrap_or_else(|| url.clone());
        let cookie = app
            .try_state::<DshManager>()
            .and_then(|manager| manager.process.lock().ok()?.cookie.clone());
        let sessions = match dsh_api::list_sessions(&api_base, cookie.as_deref()) {
            Ok(sessions) => Some(sessions),
            // cookie 失效（如 DSH 内部状态重置）时重新交换一次。
            Err(_) => {
                let refreshed = dsh_api::exchange_browser_cookie(&api_base, &url)
                    .ok()
                    .flatten();
                if let Some(cookie) = refreshed.as_ref() {
                    if let Some(manager) = app.try_state::<DshManager>() {
                        if let Ok(mut state) = manager.process.lock() {
                            if state.generation == generation {
                                state.cookie = Some(cookie.clone());
                            }
                        }
                    }
                }
                refreshed.and_then(|cookie| dsh_api::list_sessions(&api_base, Some(&cookie)).ok())
            }
        };
        if let (Some(sessions), Some(store)) = (sessions, app.try_state::<TaskStore>()) {
            if store.sync_dsh_sessions(&sessions).is_ok() {
                if let Ok(tasks) = store.list() {
                    let dsh_tasks = tasks
                        .into_iter()
                        .filter(|task| task.agent == "dsh")
                        .collect::<Vec<_>>();
                    let _ = app.emit("dsh-tasks", dsh_tasks);
                }
            }
        }
        thread::sleep(Duration::from_secs(1));
    });
}

#[tauri::command]
pub async fn stop_dsh(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || stop_dsh_inner(app.clone(), app.state()))
        .await
        .map_err(|error| format!("DSH stop worker failed: {error}"))?
}

fn stop_dsh_inner(app: AppHandle, manager: State<'_, DshManager>) -> Result<(), String> {
    let mut state = manager.process.lock().map_err(|_| lock_error())?;
    if let Some(child) = state.child.as_mut() {
        child
            .kill()
            .map_err(|error| format!("failed to stop DSH: {error}"))?;
        let _ = child.wait();
    }
    state.generation = state.generation.wrapping_add(1);
    state.child = None;
    state.tree = None;
    state.url = None;
    drop(state);
    let _ = app.emit(
        "dsh-status",
        DshHostStatus {
            running: false,
            url: None,
            error: None,
        },
    );
    Ok(())
}

impl Drop for DshManager {
    fn drop(&mut self) {
        if let Ok(mut state) = self.process.lock() {
            if let Some(child) = state.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tauri::Url;

    use super::{
        culprit_from_lines, dsh_api_base, dsh_url_is_allowed, extract_dsh_url,
        strip_package_from_manifest,
    };

    #[test]
    fn allows_only_the_active_loopback_origin() {
        assert!(dsh_url_is_allowed(
            "http://127.0.0.1:63358",
            &Url::parse("http://127.0.0.1:63358/settings").expect("URL should parse")
        ));
        assert!(!dsh_url_is_allowed(
            "http://127.0.0.1:63358",
            &Url::parse("https://example.com").expect("URL should parse")
        ));
        assert!(!dsh_url_is_allowed(
            "http://127.0.0.1:63358",
            &Url::parse("http://127.0.0.1:63359").expect("URL should parse")
        ));
    }

    #[test]
    fn extracts_loopback_dsh_url() {
        // 旧版：端口结尾（Url 规范化会补上根路径斜杠）。
        assert_eq!(
            extract_dsh_url("dsh web: http://127.0.0.1:63358"),
            Some("http://127.0.0.1:63358/".into())
        );
        // 0.1.5-rc.1 起：启动 URL 带进程 token，必须整段保留交给 Webview 鉴权。
        assert_eq!(
            extract_dsh_url("dsh web: http://127.0.0.1:63358/?token=abc-123_XYZ"),
            Some("http://127.0.0.1:63358/?token=abc-123_XYZ".into())
        );
    }

    #[test]
    fn api_base_drops_token_and_keeps_origin() {
        assert_eq!(
            dsh_api_base("http://127.0.0.1:63358/"),
            Some("http://127.0.0.1:63358".into())
        );
        assert_eq!(
            dsh_api_base("http://127.0.0.1:63358/?token=abc"),
            Some("http://127.0.0.1:63358".into())
        );
        assert_eq!(dsh_api_base("http://192.168.1.20:63358/"), None);
        assert_eq!(dsh_api_base("https://127.0.0.1:63358/"), None);
    }

    #[test]
    fn rejects_non_loopback_url() {
        assert_eq!(extract_dsh_url("dsh web: http://192.168.1.20:63358"), None);
    }

    fn lines(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn culprit_from_loader_entry_failure_line() {
        let stderr = lines(&[
            "[dsh-auto-approval-llm] warning",
            "Error: dsh: plugin tree failed to load: failed to import loader entry include (cordis:include): failed to import loader entry prompt-optimizer (oss-prompt-optimizer): The requested module '@deepseek-ai/dsh-llm' does not provide an export named 'deepFreeze'",
        ]);
        assert_eq!(
            culprit_from_lines(&stderr).as_deref(),
            Some("oss-prompt-optimizer")
        );
    }

    #[test]
    fn culprit_falls_back_to_node_modules_path_segment() {
        let stderr = lines(&[
            "Error: failed to import loader entry demo (demo-pkg): boom",
            "      [cause]: file:///C:/agents/dsh/profiles/web/node_modules/demo-pkg/lib/index.js:2",
        ]);
        assert_eq!(culprit_from_lines(&stderr).as_deref(), Some("demo-pkg"));
    }

    #[test]
    fn culprit_skips_official_runtime_packages() {
        let stderr = lines(&[
            "file:///F:/runtimes/dsh/node_modules/@deepseek-ai/cordis-plugin-loader/lib/index.js:97",
        ]);
        assert_eq!(culprit_from_lines(&stderr), None);
    }

    #[test]
    fn culprit_handles_scoped_paths_and_windows_separators() {
        let stderr =
            lines(&[r"C:\agents\dsh\profiles\web\node_modules\@some-scope\some-pkg\lib\index.js"]);
        assert_eq!(
            culprit_from_lines(&stderr).as_deref(),
            Some("@some-scope/some-pkg")
        );
    }

    #[test]
    fn culprit_returns_none_without_usable_evidence() {
        assert_eq!(culprit_from_lines(&lines(&["Node.js v24.13.0"])), None);
        assert_eq!(culprit_from_lines(&[]), None);
    }

    #[test]
    fn strips_package_from_dependencies_and_bundles() {
        let mut value = serde_json::json!({
            "name": "dsh-profile-web",
            "dependencies": {"oss-prompt-optimizer": "^1.0.0", "dsh-context": "^0.1.0"},
            "dsh": {"profile": {"bundles": ["@deepseek-ai/dsh-base", "dshmarket", "oss-prompt-optimizer"]}}
        });
        let (dependencies, bundles) =
            strip_package_from_manifest(&mut value, "oss-prompt-optimizer")
                .expect("strip should work");
        assert!(dependencies);
        assert!(bundles);
        assert!(value["dependencies"].get("oss-prompt-optimizer").is_none());
        assert_eq!(value["dependencies"]["dsh-context"], "^0.1.0");
        assert_eq!(
            value["dsh"]["profile"]["bundles"],
            serde_json::json!(["@deepseek-ai/dsh-base", "dshmarket"])
        );
    }

    #[test]
    fn strip_package_reports_when_absent() {
        let mut value = serde_json::json!({
            "dependencies": {"dsh-context": "^0.1.0"},
            "dsh": {"profile": {"bundles": ["dshmarket"]}}
        });
        let (dependencies, bundles) =
            strip_package_from_manifest(&mut value, "missing-pkg").expect("strip should work");
        assert!(!dependencies);
        assert!(!bundles);
    }

    #[test]
    fn remove_package_from_profile_updates_manifest_and_modules() {
        let root = std::env::temp_dir().join(format!("deeppi-dsh-repair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let profile = root.join("profiles").join("web");
        std::fs::create_dir_all(profile.join("node_modules").join("oss-prompt-optimizer"))
            .expect("fixture directories should exist");
        std::fs::create_dir_all(profile.join("node_modules").join("dsh-context"))
            .expect("fixture directories should exist");
        std::fs::write(
            profile.join("package.json"),
            br#"{"name":"dsh-profile-web","dependencies":{"oss-prompt-optimizer":"^1.0.0","dsh-context":"^0.1.0"},"dsh":{"profile":{"bundles":["dshmarket","oss-prompt-optimizer"]}}}"#,
        )
        .expect("fixture manifest should be written");
        std::fs::write(
            profile
                .join("node_modules")
                .join("oss-prompt-optimizer")
                .join("package.json"),
            br#"{"name":"oss-prompt-optimizer"}"#,
        )
        .expect("fixture package should be written");

        let report = super::remove_package_from_profile(&profile, "oss-prompt-optimizer")
            .expect("repair should succeed");
        assert!(report.removed_from_dependencies);
        assert!(report.removed_from_bundles);
        assert!(report.node_modules_removed);
        let manifest =
            std::fs::read_to_string(profile.join("package.json")).expect("manifest should read");
        assert!(!manifest.contains("oss-prompt-optimizer"));
        assert!(manifest.contains("dsh-context"));
        assert!(!profile
            .join("node_modules")
            .join("oss-prompt-optimizer")
            .exists());
        assert!(profile.join("node_modules").join("dsh-context").exists());

        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[test]
    fn remove_package_from_profile_errors_when_absent() {
        let root =
            std::env::temp_dir().join(format!("deeppi-dsh-repair-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let profile = root.join("profiles").join("web");
        std::fs::create_dir_all(&profile).expect("fixture directory should exist");
        std::fs::write(
            profile.join("package.json"),
            br#"{"name":"dsh-profile-web","dependencies":{}}"#,
        )
        .expect("fixture manifest should be written");
        let error = super::remove_package_from_profile(&profile, "missing-pkg")
            .expect_err("absent package should fail");
        assert_eq!(
            crate::message::message_code(&error),
            Some("dsh.repair.not_in_profile")
        );
        assert!(error.contains("missing-pkg"));
        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }
}

#[cfg(test)]
mod activation_tests {
    use super::{culprit_from_lines, diagnosis_kind, pending_plugins_from_lines};

    fn lines(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn pending_plugins_are_extracted_from_activation_failure() {
        let input = lines(&[
            "Failed to load plugins",
            "web boot: 3 entries did not activate",
            "@quill507/dsh-auto-approval-llm: pending (waiting for service: uiSession)",
            "dsh-better-reasoning-effort: pending (waiting for service: remote.settings)",
        ]);
        assert_eq!(
            pending_plugins_from_lines(&input),
            vec![
                "@quill507/dsh-auto-approval-llm".to_string(),
                "dsh-better-reasoning-effort".to_string()
            ]
        );
        assert_eq!(
            diagnosis_kind(&input, None, &pending_plugins_from_lines(&input)).as_deref(),
            Some("activation-pending")
        );
    }

    #[test]
    fn pending_plugins_survive_a_single_concatenated_line() {
        let input = lines(&[
            "@quill507/dsh-auto-approval-llm: pending (waiting for service: uiSession)dsh-better-reasoning-effort: pending (waiting for service: remote.settings)@banana-peeljj12/dsh-trellis: pending (waiting for service: sidebarRight)",
        ]);
        assert_eq!(
            pending_plugins_from_lines(&input),
            vec![
                "@quill507/dsh-auto-approval-llm".to_string(),
                "dsh-better-reasoning-effort".to_string(),
                "@banana-peeljj12/dsh-trellis".to_string()
            ]
        );
    }

    #[test]
    fn pending_plugins_skip_official_packages_and_deduplicate() {
        let input = lines(&[
            "@deepseek-ai/dsh-client-ui-slots: pending (waiting for service: sidebarRight)",
            "dsh-context: pending (waiting for service: uiSession)",
            "dsh-context: pending (waiting for service: uiSession)",
        ]);
        assert_eq!(
            pending_plugins_from_lines(&input),
            vec!["dsh-context".to_string()]
        );
    }

    #[test]
    fn diagnosis_kind_reports_import_failure_or_nothing() {
        let input = lines(&["Error: failed to import loader entry demo (demo-pkg): boom"]);
        let culprit = culprit_from_lines(&input);
        assert_eq!(culprit.as_deref(), Some("demo-pkg"));
        assert_eq!(
            diagnosis_kind(&input, culprit.as_ref(), &[]).as_deref(),
            Some("import-failure")
        );
        let unrelated = lines(&["Node.js v24.13.0"]);
        assert_eq!(diagnosis_kind(&unrelated, None, &[]), None);
    }
}
