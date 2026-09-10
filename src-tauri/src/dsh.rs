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
    url: Option<String>,
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

fn extract_dsh_url(line: &str) -> Option<String> {
    let address = line.trim().strip_prefix("dsh web: http://127.0.0.1:")?;
    let port = address.parse::<u16>().ok().filter(|port| *port > 0)?;
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
    state.url = None;

    let runtime = paths.dsh_runtime()?;
    let cli = AppPaths::dsh_cli_path(&runtime);
    if !cli.is_file() {
        return Err("managed DSH runtime is not installed".into());
    }

    let mut command = Command::new(paths.node_executable());
    command
        .arg(cli)
        .args(["web", "--host", "127.0.0.1", "--port", "0", "--no-open"])
        .current_dir(&runtime)
        .env("DSH_HOME", &paths.dsh_home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }

    let mut child = command
        .spawn()
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
    state.url = Some(url.clone());
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
                    state.url = None;
                    Some(Ok(Some(status.code())))
                }
                Err(error) => {
                    state.child = None;
                    state.url = None;
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

        if let (Ok(sessions), Some(store)) =
            (dsh_api::list_sessions(&url), app.try_state::<TaskStore>())
        {
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

    use super::{dsh_url_is_allowed, extract_dsh_url};

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
        assert_eq!(
            extract_dsh_url("dsh web: http://127.0.0.1:63358"),
            Some("http://127.0.0.1:63358".into())
        );
    }

    #[test]
    fn rejects_non_loopback_url() {
        assert_eq!(extract_dsh_url("dsh web: http://192.168.1.20:63358"), None);
    }
}
