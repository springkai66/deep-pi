use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{mpsc, Mutex},
    thread,
    time::Duration,
};

use serde::Serialize;
use tauri::{
    webview::{NewWindowResponse, WebviewBuilder},
    AppHandle, Emitter, Manager, State, Url, WebviewUrl,
};
use tauri_plugin_opener::OpenerExt;

use crate::{app_paths::AppPaths, dsh_api, task::TaskStore};

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

    thread::spawn(move || {
        let mut sender = Some(sender);
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(url) = extract_dsh_url(&line) {
                if let Some(channel) = sender.take() {
                    let _ = channel.send(url);
                }
            }
        }
    });
    thread::spawn(move || for _ in BufReader::new(stderr).lines() {});

    let url = match receiver.recv_timeout(Duration::from_secs(30)) {
        Ok(url) => url,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("DSH did not become ready: {error}"));
        }
    };

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

    use super::{dsh_api_base, dsh_url_is_allowed, extract_dsh_url};

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
}
