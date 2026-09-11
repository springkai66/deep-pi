use crate::project_files::{excluded_directory, require_main, GuardedPath};
use notify::{EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Component, Path},
    sync::{
        atomic::{AtomicU8, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tauri::{ipc::Channel, AppHandle, Manager};

const FILES: u8 = 1;
const GIT: u8 = 2;
const FAILED: u8 = 4;
const LEASE_TIME: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum WatchStatus {
    Changed,
    RootChanged,
    Unavailable,
    Expired,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEvent {
    project_id: String,
    watch_id: String,
    files: bool,
    git: bool,
    status: WatchStatus,
}

/// `.git` 下不属于仓库可见状态的条目：
///
/// - `deeppi-index-<uuid>` / `deeppi-<uuid>.config`：DeepPi 自己的索引与配置事务临时物；
/// - `*.lock`：Git 进行中的临时锁，真正的改动会在 index/refs 落盘时上报；
/// - `objects`：内容寻址的对象库，准备提交自己就会写入对象，而面板展示的是
///   索引、引用与工作区；外部提交/拉取仍会通过 index/refs/FETCH_HEAD 上报。
///
/// 不过滤这些条目时，一次暂存或提交准备会反过来触发刷新，使正在进行的提交预览失效。
fn transient_git_entry(name: &str) -> bool {
    name.starts_with("deeppi-") || name.ends_with(".lock") || name.eq_ignore_ascii_case("objects")
}

fn path_flags(root: &Path, path: &Path) -> u8 {
    let Ok(relative) = path.strip_prefix(root) else {
        return 0;
    };
    let mut flags = FILES | GIT;
    let mut in_git = false;
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                if in_git && transient_git_entry(&name) {
                    return 0;
                }
                if name.eq_ignore_ascii_case(".git") {
                    // `.git` 目录自身的 mtime 不携带信息：真实改动总是以子路径事件
                    // （index/refs/HEAD）上报，而 DeepPi 自己的对象与临时索引写入
                    // 只该被忽略；否则一次提交准备会把自己的预览刷新掉。
                    if components.peek().is_none() {
                        return 0;
                    }
                    in_git = true;
                    flags = GIT;
                } else if components.peek().is_some() && excluded_directory(&name) {
                    flags = GIT;
                }
            }
            _ => return 0,
        }
    }
    flags
}

struct ProjectWatch {
    project: String,
    stop: mpsc::SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    touched: Arc<Mutex<Instant>>,
}

impl ProjectWatch {
    fn start(
        root: &Path,
        project: &str,
        id: &str,
        emit: impl Fn(WatchEvent) -> bool + Send + 'static,
    ) -> Result<Self, String> {
        let guard = GuardedPath::open(root, "", true)?;
        let identity = guard.identity()?;
        let flags = Arc::new(AtomicU8::new(0));
        let queued = flags.clone();
        let watched_root = root.to_path_buf();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let bits = match result {
                    Err(_) => FAILED,
                    Ok(event) if event.need_rescan() => FILES | GIT,
                    Ok(event) => {
                        if matches!(event.kind, EventKind::Access(_)) {
                            return;
                        }
                        event
                            .paths
                            .iter()
                            .fold(0, |bits, path| bits | path_flags(&watched_root, path))
                    }
                };
                queued.fetch_or(bits, Ordering::Release);
            })
            .map_err(|_| "无法创建项目文件监听")?;
        // Hold the existing reparse-safe root guards until the native watch has acknowledged registration.
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|_| "无法监听项目目录，请检查权限或使用手动刷新")?;
        drop(guard);
        let root = root.to_path_buf();
        let project_id = project.to_owned();
        let watch_id = id.to_owned();
        let (stop, stopped) = mpsc::sync_channel(1);
        let touched = Arc::new(Mutex::new(Instant::now()));
        let heartbeat = touched.clone();
        let worker = thread::Builder::new()
            .name("deeppi-project-watch".into())
            .spawn(move || {
                let _watcher = watcher;
                let mut checked = Instant::now();
                let mut reconciled = Instant::now();
                loop {
                    match stopped.recv_timeout(Duration::from_secs(1)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    let mut bits = flags.swap(0, Ordering::AcqRel);
                    // Native directory notifications can overflow or stop silently on some Windows filesystems.
                    if reconciled.elapsed() >= Duration::from_secs(30) {
                        bits |= FILES | GIT;
                        reconciled = Instant::now();
                    }
                    let expired = heartbeat
                        .lock()
                        .map_or(true, |last| last.elapsed() >= LEASE_TIME);
                    let mut status = if expired {
                        WatchStatus::Expired
                    } else if bits & FAILED != 0 {
                        WatchStatus::Unavailable
                    } else {
                        WatchStatus::Changed
                    };
                    if bits & FAILED != 0 || checked.elapsed() >= Duration::from_secs(5) {
                        checked = Instant::now();
                        if !GuardedPath::open(&root, "", true)
                            .and_then(|root| root.identity())
                            .is_ok_and(|current| current == identity)
                        {
                            status = WatchStatus::RootChanged;
                        }
                    }
                    if bits == 0 && status == WatchStatus::Changed {
                        continue;
                    }
                    let ended = status != WatchStatus::Changed;
                    if !emit(WatchEvent {
                        project_id: project_id.clone(),
                        watch_id: watch_id.clone(),
                        files: ended || bits & FILES != 0,
                        git: ended || bits & GIT != 0,
                        status,
                    }) || ended
                    {
                        break;
                    }
                }
            })
            .map_err(|_| "无法启动项目文件监听线程")?;
        Ok(Self {
            project: project.to_owned(),
            stop,
            worker: Some(worker),
            touched,
        })
    }
}

impl Drop for ProjectWatch {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
pub struct ProjectWatchManager(Mutex<HashMap<String, ProjectWatch>>);

impl ProjectWatchManager {
    fn close(&self, project: &str, id: &str) -> Result<(), String> {
        let removed = {
            let mut watches = self.0.lock().map_err(|_| "项目监听暂不可用")?;
            if watches
                .get(id)
                .is_some_and(|watch| watch.project == project)
            {
                watches.remove(id)
            } else {
                None
            }
        };
        drop(removed);
        Ok(())
    }

    fn ping(&self, project: &str, id: &str) -> Result<(), String> {
        let watches = self.0.lock().map_err(|_| "项目监听暂不可用")?;
        let watch = watches
            .get(id)
            .filter(|watch| {
                watch.project == project
                    && watch
                        .worker
                        .as_ref()
                        .is_some_and(|worker| !worker.is_finished())
            })
            .ok_or("项目监听已结束，请重新连接")?;
        *watch.touched.lock().map_err(|_| "项目监听暂不可用")? = Instant::now();
        Ok(())
    }
}

#[tauri::command]
pub async fn start_project_watch(
    webview: tauri::Webview,
    app: AppHandle,
    project_id: String,
    watch_id: String,
    channel: Channel<WatchEvent>,
) -> Result<(), String> {
    require_main(&webview)?;
    uuid::Uuid::parse_str(&watch_id).map_err(|_| "项目监听标识无效")?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = app
            .state::<crate::task::TaskStore>()
            .project_path(&project_id)?;
        let manager = app.state::<ProjectWatchManager>();
        let mut watches = manager.0.lock().map_err(|_| "项目监听暂不可用")?;
        watches.retain(|_, watch| {
            watch
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
        });
        if watches.contains_key(&watch_id) || watches.len() >= 8 {
            return Err("项目监听重复或已达容量上限".into());
        }
        let watch = ProjectWatch::start(Path::new(&root), &project_id, &watch_id, move |event| {
            channel.send(event).is_ok()
        })?;
        watches.insert(watch_id, watch);
        Ok(())
    })
    .await
    .map_err(|_| "项目监听启动任务失败")?
}

#[tauri::command]
pub async fn stop_project_watch(
    webview: tauri::Webview,
    app: AppHandle,
    project_id: String,
    watch_id: String,
) -> Result<(), String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ProjectWatchManager>()
            .close(&project_id, &watch_id)
    })
    .await
    .map_err(|_| "项目监听停止任务失败")?
}

#[tauri::command]
pub async fn ping_project_watch(
    webview: tauri::Webview,
    app: AppHandle,
    project_id: String,
    watch_id: String,
) -> Result<(), String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ProjectWatchManager>()
            .ping(&project_id, &watch_id)
    })
    .await
    .map_err(|_| "项目监听续期失败")?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_scoped_paths_without_exposing_names_or_refreshing_generated_files() {
        let root = Path::new(r"C:\project");
        assert_eq!(
            path_flags(root, &root.join("src").join("中文.txt")),
            FILES | GIT
        );
        assert_eq!(path_flags(root, &root.join(".git").join("index")), GIT);
        // 裸的 `.git` 只是目录 mtime，真实改动会带子路径。
        assert_eq!(path_flags(root, &root.join(".git")), 0);
        assert_eq!(
            path_flags(
                root,
                &root.join("node_modules").join("pkg").join("index.js")
            ),
            GIT
        );
        assert_eq!(path_flags(root, Path::new(r"C:\other\secret")), 0);
        assert_eq!(path_flags(root, root), FILES | GIT);
        assert_eq!(path_flags(root, &root.join("target")), FILES | GIT);
    }

    #[test]
    fn ignores_own_transaction_files_and_git_locks() {
        let root = Path::new(r"C:\project");
        // 事务临时物：暂存与提交准备会创建它们，不应反过来让预览失效。
        assert_eq!(
            path_flags(
                root,
                &root.join(".git").join("deeppi-index-8f14").join("index")
            ),
            0
        );
        assert_eq!(
            path_flags(root, &root.join(".git").join("deeppi-8f14.config")),
            0
        );
        // 锁文件是进行中的临时状态，真正的改动由 index/refs 上报。
        assert_eq!(path_flags(root, &root.join(".git").join("index.lock")), 0);
        // 对象库是内容寻址存储，面板不展示其中内容。
        assert_eq!(
            path_flags(
                root,
                &root
                    .join(".git")
                    .join("objects")
                    .join("76")
                    .join("27799c75485ff4bfc5b78fdecbd9d1c3f2c0f3")
            ),
            0
        );
        assert_eq!(path_flags(root, &root.join(".git").join("objects")), 0);
        assert_eq!(
            path_flags(
                root,
                &root
                    .join(".git")
                    .join("refs")
                    .join("heads")
                    .join("main.lock")
            ),
            0
        );
        // 真实的状态变更仍然上报。
        assert_eq!(path_flags(root, &root.join(".git").join("index")), GIT);
        assert_eq!(
            path_flags(
                root,
                &root.join(".git").join("refs").join("heads").join("main")
            ),
            GIT
        );
        // 项目里同名的普通文件不受影响。
        assert_eq!(
            path_flags(root, &root.join("deeppi-index-8f14.txt")),
            FILES | GIT
        );
        assert_eq!(path_flags(root, &root.join("notes.lock")), FILES | GIT);
    }

    #[cfg(windows)]
    #[test]
    fn real_watcher_coalesces_writes_and_stops_without_holding_the_root() {
        let directory = tempfile::tempdir().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let watch = ProjectWatch::start(directory.path(), "p", "w", move |event| {
            sender.send(event).is_ok()
        })
        .unwrap();
        for index in 0..25 {
            std::fs::write(directory.path().join(format!("{index}.txt")), b"test").unwrap();
        }
        let event = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(event.files && event.git);
        assert_eq!(event.project_id, "p");
        assert_eq!(event.watch_id, "w");
        drop(watch);
        std::fs::rename(
            directory.path().join("0.txt"),
            directory.path().join("renamed.txt"),
        )
        .unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn replacing_the_root_reports_invalidation_instead_of_following_the_new_directory() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let watch =
            ProjectWatch::start(&root, "p", "w", move |event| sender.send(event).is_ok()).unwrap();
        std::fs::rename(&root, directory.path().join("old")).unwrap();
        std::fs::create_dir(&root).unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let event = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if event.status == WatchStatus::RootChanged {
                break;
            }
        }
        drop(watch);
    }

    #[test]
    fn manager_close_is_scoped_and_unknown_heartbeat_is_rejected() {
        let manager = ProjectWatchManager::default();
        assert!(manager.ping("p", "unknown").is_err());
        assert!(manager.close("p", "unknown").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn abandoned_watch_expires_and_cannot_be_renewed_after_worker_exit() {
        let directory = tempfile::tempdir().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let watch = ProjectWatch::start(directory.path(), "p", "w", move |event| {
            sender.send(event).is_ok()
        })
        .unwrap();
        *watch.touched.lock().unwrap() = Instant::now() - LEASE_TIME - Duration::from_secs(1);
        let event = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(event.status == WatchStatus::Expired);
        let mut watch = watch;
        watch.worker.take().unwrap().join().unwrap();
        let manager = ProjectWatchManager::default();
        manager.0.lock().unwrap().insert("w".into(), watch);
        assert!(manager.ping("p", "w").is_err());
        manager.close("p", "w").unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn close_and_heartbeat_cannot_target_another_project_or_lease() {
        let directory = tempfile::tempdir().unwrap();
        let manager = ProjectWatchManager::default();
        manager.0.lock().unwrap().insert(
            "w".into(),
            ProjectWatch::start(directory.path(), "p", "w", |_| true).unwrap(),
        );
        manager.close("other", "w").unwrap();
        assert!(manager.ping("p", "w").is_ok());
        assert!(manager.ping("other", "w").is_err());
        manager.close("p", "old").unwrap();
        assert!(manager.ping("p", "w").is_ok());
        manager.close("p", "w").unwrap();
        assert!(manager.ping("p", "w").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn periodic_reconciliation_refreshes_even_without_native_events() {
        let directory = tempfile::tempdir().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let watch = ProjectWatch::start(directory.path(), "p", "w", move |event| {
            sender.send(event).is_ok()
        })
        .unwrap();
        let event = receiver.recv_timeout(Duration::from_secs(35)).unwrap();
        assert!(event.status == WatchStatus::Changed);
        assert!(event.files && event.git);
        drop(watch);
    }

    #[cfg(windows)]
    #[test]
    fn root_identity_guards_do_not_prevent_sibling_runtime_rollback() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["watched", "current", "candidate"] {
            std::fs::create_dir(directory.path().join(name)).unwrap();
        }
        let guard = GuardedPath::open(&directory.path().join("watched"), "", true).unwrap();
        let error = crate::recovery::activate_directory(
            &directory.path().join("current"),
            &directory.path().join("candidate"),
            &directory.path().join("backup"),
            || Err::<(), _>("verification failed".into()),
        )
        .unwrap_err();
        assert!(error.contains("verification failed"), "{error}");
        assert!(error.contains("previous state restored"), "{error}");
        drop(guard);
    }
}
