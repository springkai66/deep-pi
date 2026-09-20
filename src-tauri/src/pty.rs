use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD, Engine};
#[cfg(not(windows))]
use portable_pty::ChildKiller;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::{
    app_paths::AppPaths,
    bridge,
    message::msg,
    runtime::RuntimeOperationLock,
    settings::SettingsStore,
    task::{TaskRecord, TaskStatus, TaskStore},
};

const MAX_TERMINAL_DIMENSION: u16 = 1_000;

struct PtyTask {
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Box<dyn Write + Send>,
    #[cfg(not(windows))]
    killer: Box<dyn ChildKiller + Send + Sync>,
    #[cfg(windows)]
    process: std::os::windows::io::OwnedHandle,
    gate: Arc<OutputGate>,
}

impl PtyTask {
    fn kill(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess};
            let handle = self.process.as_raw_handle();
            if unsafe { TerminateProcess(handle, 1) } == 0 {
                let error = std::io::Error::last_os_error();
                let mut status = 259_u32;
                if unsafe { GetExitCodeProcess(handle, &mut status) } == 0 || status == 259 {
                    return Err(format!("failed to stop Pi: {error}"));
                }
            }
            Ok(())
        }
        #[cfg(not(windows))]
        self.killer.kill().map_err(|error| error.to_string())
    }
}

#[cfg(windows)]
fn owned_process(
    child: &dyn portable_pty::Child,
) -> Result<std::os::windows::io::OwnedHandle, String> {
    use std::os::windows::io::BorrowedHandle;
    let raw = child
        .as_raw_handle()
        .ok_or("PTY child process handle is unavailable")?;
    // The child owns this handle throughout the duplication.
    unsafe { BorrowedHandle::borrow_raw(raw) }
        .try_clone_to_owned()
        .map_err(|error| error.to_string())
}

struct OutputGate {
    run_id: String,
    ready: Mutex<bool>,
    changed: Condvar,
}

impl OutputGate {
    fn new(run_id: String) -> Self {
        Self {
            run_id,
            ready: Mutex::new(false),
            changed: Condvar::new(),
        }
    }

    fn acknowledge(&self, run_id: &str) -> bool {
        if self.run_id != run_id {
            return false;
        }
        let Ok(mut ready) = self.ready.lock() else {
            return false;
        };
        *ready = true;
        self.changed.notify_all();
        true
    }

    fn wait(&self, timeout: Duration) -> bool {
        self.ready
            .lock()
            .ok()
            .and_then(|ready| {
                self.changed
                    .wait_timeout_while(ready, timeout, |ready| !*ready)
                    .ok()
            })
            .is_some_and(|(ready, _)| *ready)
    }
}

#[derive(Default)]
pub struct PtyManager {
    tasks: Mutex<HashMap<String, PtyTask>>,
    changed: Condvar,
}

impl PtyManager {
    pub(crate) fn contains(&self, task_id: &str) -> Result<bool, String> {
        Ok(self
            .tasks
            .lock()
            .map_err(|_| lock_error())?
            .contains_key(task_id))
    }

    fn stop_run_and_wait(
        &self,
        store: &TaskStore,
        task_id: &str,
        run_id: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        let mut tasks = self.tasks.lock().map_err(|_| lock_error())?;
        let Some(task) = tasks.get_mut(task_id) else {
            return Ok(());
        };
        if task.gate.run_id != run_id {
            return Err("Pi run is no longer current".into());
        }
        task.kill()?;
        task.gate.acknowledge(run_id);
        store.set_status(task_id, TaskStatus::Cancelled)?;
        let (tasks, _) = self
            .changed
            .wait_timeout_while(tasks, timeout, |tasks| {
                tasks
                    .get(task_id)
                    .is_some_and(|task| task.gate.run_id == run_id)
            })
            .map_err(|_| lock_error())?;
        if tasks
            .get(task_id)
            .is_some_and(|task| task.gate.run_id == run_id)
        {
            Err(msg("pty.terminal_busy"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn count(&self) -> Result<usize, String> {
        Ok(self.tasks.lock().map_err(|_| lock_error())?.len())
    }
    pub(crate) fn is_running(&self) -> Result<bool, String> {
        Ok(!self.tasks.lock().map_err(|_| lock_error())?.is_empty())
    }

    pub(crate) fn attach_run_ids(&self, records: &mut [TaskRecord]) -> Result<(), String> {
        let tasks = self.tasks.lock().map_err(|_| lock_error())?;
        for record in records {
            record.run_id = tasks.get(&record.id).map(|task| task.gate.run_id.clone());
        }
        Ok(())
    }

    fn finish_run(
        &self,
        store: &TaskStore,
        task_id: &str,
        run_id: &str,
        status: TaskStatus,
    ) -> Result<bool, String> {
        let mut tasks = self.tasks.lock().map_err(|_| lock_error())?;
        if !tasks
            .get(task_id)
            .is_some_and(|task| task.gate.run_id == run_id)
        {
            return Ok(false);
        }
        store.finish_if_active(task_id, status)?;
        tasks.remove(task_id);
        self.changed.notify_all();
        Ok(true)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPiTaskRequest {
    project_id: String,
    title: String,
    rows: u16,
    cols: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartPiTaskRequest {
    task_id: String,
    rows: u16,
    cols: u16,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyOutput {
    task_id: String,
    run_id: String,
    data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyExit {
    task_id: String,
    run_id: String,
    exit_code: Option<u32>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskStatusUpdate {
    task_id: String,
    run_id: String,
    status: TaskStatus,
}

fn parse_bridge_status(value: &str) -> Option<TaskStatus> {
    match value.trim() {
        "running" => Some(TaskStatus::Running),
        "waiting" => Some(TaskStatus::Waiting),
        _ => None,
    }
}

fn normalize_size(rows: u16, cols: u16) -> Result<PtySize, String> {
    if rows == 0 || cols == 0 {
        return Err("terminal dimensions must be greater than zero".into());
    }
    if rows > MAX_TERMINAL_DIMENSION || cols > MAX_TERMINAL_DIMENSION {
        return Err(format!(
            "terminal dimensions cannot exceed {MAX_TERMINAL_DIMENSION}"
        ));
    }
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}

fn lock_error() -> String {
    "PTY task state is unavailable".into()
}

pub(crate) fn resolve_default_working_directory(current: PathBuf) -> PathBuf {
    current
        .parent()
        .filter(|parent| {
            current.file_name().is_some_and(|name| name == "src-tauri")
                && parent.join("package.json").is_file()
        })
        .map_or_else(|| current.clone(), Path::to_path_buf)
}

#[tauri::command]
pub fn default_working_directory(paths: State<'_, AppPaths>) -> String {
    paths.project_root.to_string_lossy().into_owned()
}

#[tauri::command]
pub async fn start_pi_task(
    app: AppHandle,
    request: StartPiTaskRequest,
) -> Result<TaskRecord, String> {
    tauri::async_runtime::spawn_blocking(move || {
        start_pi_task_inner(
            app.clone(),
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            request,
        )
    })
    .await
    .map_err(|error| format!("Pi startup worker failed: {error}"))?
}

fn start_pi_task_inner(
    app: AppHandle,
    manager: State<'_, PtyManager>,
    store: State<'_, TaskStore>,
    paths: State<'_, AppPaths>,
    lifecycle: State<'_, RuntimeOperationLock>,
    settings: State<'_, SettingsStore>,
    request: StartPiTaskRequest,
) -> Result<TaskRecord, String> {
    let _lifecycle = lifecycle.acquire()?;
    crate::snapshot::Snapshot::ensure_ready(&paths.backups)?;
    ensure_capacity(&manager, &store, &settings)?;
    let environment = paths.new_pi_environment(&settings.get()?.pi_environment)?;
    let home = paths.pi_environment_home(environment)?;
    let mut record = store.create_pi_task_for_project(&request.project_id, &request.title)?;
    store.set_pi_environment(&record.id, environment, &home)?;
    record.pi_environment = environment.into();
    record.pi_agent_dir = Some(home.to_string_lossy().into_owned());
    record.run_id = Some(Uuid::new_v4().to_string());
    if let Err(error) = launch_pi(app, &manager, &paths, &record, request.rows, request.cols) {
        let _ = store.finish_if_active(&record.id, TaskStatus::Failed);
        return Err(error);
    }
    Ok(record)
}

#[tauri::command]
pub async fn restart_pi_task(
    app: AppHandle,
    request: RestartPiTaskRequest,
) -> Result<TaskRecord, String> {
    tauri::async_runtime::spawn_blocking(move || {
        restart_pi_task_inner(
            app.clone(),
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            app.state(),
            request,
        )
    })
    .await
    .map_err(|error| format!("Pi restart worker failed: {error}"))?
}

fn restart_pi_task_inner(
    app: AppHandle,
    manager: State<'_, PtyManager>,
    store: State<'_, TaskStore>,
    paths: State<'_, AppPaths>,
    lifecycle: State<'_, RuntimeOperationLock>,
    settings: State<'_, SettingsStore>,
    request: RestartPiTaskRequest,
) -> Result<TaskRecord, String> {
    let _lifecycle = lifecycle.acquire()?;
    crate::snapshot::Snapshot::ensure_ready(&paths.backups)?;
    ensure_capacity(&manager, &store, &settings)?;
    if manager
        .tasks
        .lock()
        .map_err(|_| lock_error())?
        .contains_key(&request.task_id)
    {
        return Err("Pi task is already running".into());
    }
    if app
        .state::<crate::rpc::RpcManager>()
        .contains(&request.task_id)?
    {
        return Err("RPC task is still running or stopping".into());
    }
    let mut record = store.restart(&request.task_id)?;
    store.set_interaction_mode(&record.id, "tui")?;
    record.interaction_mode = "tui".into();
    record.run_id = Some(Uuid::new_v4().to_string());
    if let Err(error) = launch_pi(app, &manager, &paths, &record, request.rows, request.cols) {
        let _ = store.finish_if_active(&record.id, TaskStatus::Failed);
        return Err(error);
    }
    Ok(record)
}

fn ensure_capacity(
    manager: &PtyManager,
    store: &crate::task::TaskStore,
    settings: &SettingsStore,
) -> Result<(), String> {
    // 与 rpc::start 同一口径：TUI 会话存活即占额度（宿主无法感知其内部忙闲）；
    // RPC 会话只统计 AI 正在执行（status = Running）的，空闲打开的会话不占额度。
    if manager.count()? + store.count_running_rpc()?
        >= usize::from(settings.get()?.max_concurrent_tasks)
    {
        return Err("maximum concurrent Pi tasks reached".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn acknowledge_pi_output(
    app: AppHandle,
    task_id: String,
    run_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        acknowledge_pi_output_inner(app.state(), task_id, run_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn acknowledge_pi_output_inner(
    manager: State<'_, PtyManager>,
    task_id: String,
    run_id: String,
) -> Result<(), String> {
    let tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let task = tasks.get(&task_id).ok_or("Pi task was not found")?;
    if !task.gate.acknowledge(&run_id) {
        return Err("Pi run is no longer current".into());
    }
    Ok(())
}

fn launch_pi(
    app: AppHandle,
    manager: &PtyManager,
    paths: &AppPaths,
    record: &TaskRecord,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let run_id = record.run_id.clone().ok_or("Pi run id is missing")?;
    let cwd = PathBuf::from(&record.project_path);
    if !cwd.is_dir() {
        return Err("task working directory does not exist".into());
    }
    let pair = native_pty_system()
        .openpty(normalize_size(rows, cols)?)
        .map_err(|error| format!("failed to create PTY: {error}"))?;

    // 只用托管运行时：node 与 Pi CLI 都必须在托管目录里，否则明确报错，
    // 不回落系统 PATH 上的 node，也不执行电脑上安装的 pi。
    let pi_cli = paths.required_pi_cli()?;
    let mut command = CommandBuilder::new(paths.node_runtime()?);
    command.arg(pi_cli);
    command.args(crate::native_pi::session_arguments(record)?);
    // 托管任务的 bridge 扩展位于绑定的 pi_home/extensions 下，由 Pi 自动发现；
    // 旧 native 任务已在 task_pi_home 中明确拒绝。
    command.env("PI_CODING_AGENT_DIR", paths.task_pi_home(record)?);
    command.env_remove("PI_CODING_AGENT_SESSION_DIR");
    command.env("DEEPPI_TASK_ID", &record.id);
    let state_file = bridge::state_file(paths, &run_id)?;
    let pipe_name = bridge::pipe_name(paths, &run_id)?;
    let _ = fs::remove_file(&state_file);
    command.env("DEEPPI_STATE_FILE", &state_file);
    #[cfg(windows)]
    command.env("DEEPPI_BRIDGE_PIPE", &pipe_name);
    command.cwd(cwd);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to read PTY: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("failed to write PTY: {error}"))?;

    let mut tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("failed to start Pi: {error}"))?;
    #[cfg(not(windows))]
    let killer = child.clone_killer();
    #[cfg(windows)]
    let process = match owned_process(child.as_ref()) {
        Ok(process) => process,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let gate = Arc::new(OutputGate::new(run_id.clone()));
    tasks.insert(
        record.id.clone(),
        PtyTask {
            master: Some(pair.master),
            writer,
            #[cfg(not(windows))]
            killer,
            #[cfg(windows)]
            process,
            gate: gate.clone(),
        },
    );
    drop(tasks);

    watch_bridge_status(app.clone(), record.id.clone(), run_id.clone(), state_file);
    #[cfg(windows)]
    watch_bridge_pipe(app.clone(), record.id.clone(), run_id.clone(), pipe_name);

    let output_app = app.clone();
    let output_task_id = record.id.clone();
    let output_run_id = run_id.clone();
    let output_thread = thread::spawn(move || {
        if !gate.wait(Duration::from_secs(30)) {
            if let Some(manager) = output_app.try_state::<PtyManager>() {
                if let Ok(mut tasks) = manager.tasks.lock() {
                    if let Some(task) = tasks
                        .get_mut(&output_task_id)
                        .filter(|task| task.gate.run_id == output_run_id)
                    {
                        let _ = task.kill();
                    }
                }
            }
            return Some("terminal did not acknowledge its output subscription".to_string());
        }
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let payload = PtyOutput {
                        task_id: output_task_id.clone(),
                        run_id: output_run_id.clone(),
                        data: STANDARD.encode(&buffer[..read]),
                    };
                    if output_app.emit("pty-output", payload).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    return Some(format!("PTY read failed: {error}"));
                }
            }
        }
        None
    });

    let exit_app = app;
    let exit_task_id = record.id.clone();
    thread::spawn(move || {
        let result = child.wait();
        let console = exit_app.try_state::<PtyManager>().and_then(|state| {
            state
                .tasks
                .lock()
                .ok()?
                .get_mut(&exit_task_id)
                .filter(|task| task.gate.run_id == run_id)
                .and_then(|task| task.master.take())
        });
        // ClosePseudoConsole may wait for the reader, so never hold the task-map lock here.
        drop(console);
        let output_error = output_thread
            .join()
            .unwrap_or_else(|_| Some("PTY reader panicked".into()));
        let (exit_code, mut error) = match result {
            Ok(status) => (Some(status.exit_code()), None),
            Err(error) => (None, Some(format!("failed to wait for Pi: {error}"))),
        };
        error = error.or(output_error);
        let final_status = if error.is_none() && exit_code == Some(0) {
            TaskStatus::Completed
        } else {
            TaskStatus::Failed
        };
        if let (Some(state), Some(store)) = (
            exit_app.try_state::<PtyManager>(),
            exit_app.try_state::<TaskStore>(),
        ) {
            match state.finish_run(&store, &exit_task_id, &run_id, final_status) {
                Ok(true) => {
                    let _ = exit_app.emit(
                        "pty-exit",
                        PtyExit {
                            task_id: exit_task_id,
                            run_id,
                            exit_code,
                            error,
                        },
                    );
                }
                Err(error) => log::error!("event=pty_finish task={} error={}", exit_task_id, error),
                Ok(false) => {}
            }
        }
    });
    Ok(())
}

fn task_is_active(app: &AppHandle, task_id: &str, run_id: &str) -> bool {
    app.try_state::<PtyManager>().is_some_and(|manager| {
        manager.tasks.lock().is_ok_and(|tasks| {
            tasks
                .get(task_id)
                .is_some_and(|task| task.gate.run_id == run_id)
        })
    })
}

fn apply_bridge_status(
    app: &AppHandle,
    task_id: &str,
    run_id: &str,
    status: TaskStatus,
    last_status: &mut Option<TaskStatus>,
) {
    if Some(status) == *last_status {
        return;
    }
    let Some(manager) = app.try_state::<PtyManager>() else {
        return;
    };
    let Ok(tasks) = manager.tasks.lock() else {
        return;
    };
    if !tasks
        .get(task_id)
        .is_some_and(|task| task.gate.run_id == run_id)
    {
        return;
    }
    if let Some(store) = app.try_state::<TaskStore>() {
        if store.set_active_status(task_id, status) == Ok(true) {
            let _ = app.emit(
                "task-status",
                TaskStatusUpdate {
                    task_id: task_id.to_owned(),
                    run_id: run_id.to_owned(),
                    status,
                },
            );
            *last_status = Some(status);
        }
    }
}

fn watch_bridge_status(app: AppHandle, task_id: String, run_id: String, state_file: PathBuf) {
    thread::spawn(move || {
        let mut last_status = None;
        loop {
            if !task_is_active(&app, &task_id, &run_id) {
                break;
            }
            if let Some(status) = fs::read_to_string(&state_file)
                .ok()
                .and_then(|value| parse_bridge_status(&value))
            {
                apply_bridge_status(&app, &task_id, &run_id, status, &mut last_status);
            }
            thread::sleep(Duration::from_millis(200));
        }
        let _ = fs::remove_file(state_file);
    });
}

#[cfg(windows)]
fn watch_bridge_pipe(app: AppHandle, task_id: String, run_id: String, pipe_name: String) {
    thread::spawn(move || {
        use std::{
            ffi::OsStr,
            os::windows::ffi::OsStrExt,
            ptr::{null, null_mut},
        };
        use windows_sys::Win32::{
            Foundation::{
                CloseHandle, GetLastError, ERROR_NO_DATA, ERROR_PIPE_CONNECTED,
                ERROR_PIPE_LISTENING, INVALID_HANDLE_VALUE,
            },
            Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND},
            System::Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_NOWAIT,
                PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
            },
        };

        let wide_name = OsStr::new(&pipe_name)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut last_status = None;
        while task_is_active(&app, &task_id, &run_id) {
            let pipe = unsafe {
                CreateNamedPipeW(
                    wide_name.as_ptr(),
                    PIPE_ACCESS_INBOUND,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS | PIPE_NOWAIT,
                    1,
                    256,
                    256,
                    0,
                    null(),
                )
            };
            if pipe == INVALID_HANDLE_VALUE {
                log::warn!("event=bridge_pipe status=create_failed task={task_id}");
                break;
            }

            let connected = loop {
                if !task_is_active(&app, &task_id, &run_id) {
                    unsafe { CloseHandle(pipe) };
                    return;
                }
                if unsafe { ConnectNamedPipe(pipe, null_mut()) } != 0 {
                    break true;
                }
                let error = unsafe { GetLastError() };
                if error == ERROR_PIPE_CONNECTED {
                    break true;
                }
                if error != ERROR_PIPE_LISTENING {
                    break false;
                }
                thread::sleep(Duration::from_millis(50));
            };
            if !connected {
                unsafe { CloseHandle(pipe) };
                break;
            }

            let mut buffer = [0_u8; 128];
            let mut read = 0_u32;
            let started = std::time::Instant::now();
            loop {
                if !task_is_active(&app, &task_id, &run_id)
                    || started.elapsed() > Duration::from_secs(1)
                {
                    break;
                }
                if unsafe {
                    ReadFile(
                        pipe,
                        buffer.as_mut_ptr(),
                        buffer.len() as u32,
                        &mut read,
                        null_mut(),
                    )
                } != 0
                {
                    break;
                }
                if unsafe { GetLastError() } != ERROR_NO_DATA {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            if read > 0 {
                if let Some(status) =
                    parse_bridge_status(&String::from_utf8_lossy(&buffer[..read as usize]))
                {
                    apply_bridge_status(&app, &task_id, &run_id, status, &mut last_status);
                }
            }
            unsafe {
                DisconnectNamedPipe(pipe);
                CloseHandle(pipe);
            }
        }
    });
}

fn terminal_input_bytes(data: &str) -> &[u8] {
    data.as_bytes()
}

#[tauri::command]
pub async fn write_pi_task(
    app: AppHandle,
    task_id: String,
    run_id: String,
    data: String,
) -> Result<(), String> {
    if data.len() > 1024 * 1024 {
        return Err("Terminal input exceeds 1 MiB".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        write_pi_task_inner(app.state(), task_id, run_id, data)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn write_pi_task_inner(
    manager: State<'_, PtyManager>,
    task_id: String,
    run_id: String,
    data: String,
) -> Result<(), String> {
    let mut tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| "Pi task was not found".to_string())?;
    if task.gate.run_id != run_id {
        return Err("Pi run is no longer current".into());
    }
    task.writer
        .write_all(terminal_input_bytes(&data))
        .and_then(|_| task.writer.flush())
        .map_err(|error| format!("failed to write to Pi: {error}"))
}

#[tauri::command]
pub async fn resize_pi_task(
    app: AppHandle,
    task_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        resize_pi_task_inner(app.state(), task_id, rows, cols)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn resize_pi_task_inner(
    manager: State<'_, PtyManager>,
    task_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let size = normalize_size(rows, cols)?;
    let tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let task = tasks
        .get(&task_id)
        .ok_or_else(|| "Pi task was not found".to_string())?;
    task.master
        .as_ref()
        .ok_or("Pi task is exiting")?
        .resize(size)
        .map_err(|error| format!("failed to resize Pi: {error}"))
}

#[tauri::command]
pub async fn stop_pi_task(app: AppHandle, task_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        stop_pi_task_inner(app.state(), app.state(), task_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn stop_pi_task_inner(
    manager: State<'_, PtyManager>,
    store: State<'_, TaskStore>,
    task_id: String,
) -> Result<(), String> {
    let mut tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| "Pi task was not found".to_string())?;
    task.kill()?;
    task.gate.acknowledge(&task.gate.run_id);
    store.set_status(&task_id, TaskStatus::Cancelled)
}

#[tauri::command]
pub async fn stop_pi_run(
    webview: tauri::Webview,
    app: AppHandle,
    task_id: String,
    run_id: String,
) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("Stopping a run requires the main Webview".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<PtyManager>().stop_run_and_wait(
            &app.state::<TaskStore>(),
            &task_id,
            &run_id,
            Duration::from_secs(5),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn stop_all_pi_tasks(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        stop_all_pi_tasks_inner(app.state(), app.state(), app.state())
    })
    .await
    .map_err(|error| format!("Pi shutdown worker failed: {error}"))?
}

fn stop_all_pi_tasks_inner(
    manager: State<'_, PtyManager>,
    store: State<'_, TaskStore>,
    rpc_manager: State<'_, crate::rpc::RpcManager>,
) -> Result<(), String> {
    rpc_manager.stop_all()?;
    let mut tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let mut errors = Vec::new();
    for (task_id, task) in tasks.iter_mut() {
        if let Err(error) = task.kill() {
            errors.push(format!("{task_id}: {error}"));
            continue;
        }
        task.gate.acknowledge(&task.gate.run_id);
        if let Err(error) = store.set_status(task_id, TaskStatus::Cancelled) {
            errors.push(format!("{task_id}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "failed to stop some Pi tasks: {}",
            errors.join("; ")
        ))
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        if let Ok(mut tasks) = self.tasks.lock() {
            for task in tasks.values_mut() {
                task.gate.acknowledge(&task.gate.run_id);
                let _ = task.kill();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex, thread, time::Duration};

    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    use super::{
        normalize_size, parse_bridge_status, resolve_default_working_directory,
        terminal_input_bytes, PtyManager, PtyTask,
    };
    use crate::task::TaskStatus;

    #[test]
    fn rejects_output_acknowledgement_from_an_old_run() {
        let gate = super::OutputGate::new("new-run".into());
        assert!(!gate.acknowledge("old-run"));
        assert!(gate.acknowledge("new-run"));
        assert!(gate.wait(Duration::from_millis(1)));
    }

    #[test]
    fn output_wait_is_bounded_when_frontend_never_subscribes() {
        let gate = super::OutputGate::new("run".into());
        assert!(!gate.wait(Duration::from_millis(1)));
    }

    #[cfg(windows)]
    #[test]
    fn old_exit_cannot_remove_or_finish_the_new_run() {
        let store = crate::task::TaskStore::in_memory().unwrap();
        let record = store.create_pi_task("new", ".").unwrap();
        let pair = native_pty_system()
            .openpty(normalize_size(24, 80).unwrap())
            .unwrap();
        let mut command = CommandBuilder::new("powershell.exe");
        command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
        let mut child = pair.slave.spawn_command(command).unwrap();
        let process = super::owned_process(child.as_ref()).unwrap();
        let manager = PtyManager {
            tasks: Mutex::new(HashMap::from([(
                record.id.clone(),
                PtyTask {
                    writer: pair.master.take_writer().unwrap(),
                    master: Some(pair.master),
                    process,
                    gate: std::sync::Arc::new(super::OutputGate::new("new-run".into())),
                },
            )])),
            changed: Default::default(),
        };
        assert!(!manager
            .finish_run(&store, &record.id, "old-run", TaskStatus::Completed)
            .unwrap());
        assert!(manager.is_running().unwrap());
        assert_eq!(
            store.get(&record.id).unwrap().unwrap().status,
            TaskStatus::Running
        );
        let mut records = store.list().unwrap();
        manager.attach_run_ids(&mut records).unwrap();
        assert_eq!(records[0].run_id.as_deref(), Some("new-run"));
        drop(manager);
        child.wait().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn scoped_stop_rejects_old_run_and_waits_for_cleanup() {
        use std::sync::Arc;
        let store = Arc::new(crate::task::TaskStore::in_memory().unwrap());
        let record = store.create_pi_task("switch", ".").unwrap();
        let pair = native_pty_system()
            .openpty(normalize_size(24, 80).unwrap())
            .unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut command = CommandBuilder::new("powershell.exe");
        command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let process = super::owned_process(child.as_ref()).unwrap();
        let manager = Arc::new(PtyManager::default());
        manager.tasks.lock().unwrap().insert(
            record.id.clone(),
            PtyTask {
                writer: pair.master.take_writer().unwrap(),
                master: Some(pair.master),
                process,
                gate: Arc::new(super::OutputGate::new("current".into())),
            },
        );
        assert!(manager
            .stop_run_and_wait(&store, &record.id, "old", Duration::from_secs(1))
            .is_err());
        assert_eq!(
            store.get(&record.id).unwrap().unwrap().status,
            TaskStatus::Running
        );
        assert!(manager
            .stop_run_and_wait(&store, &record.id, "current", Duration::ZERO)
            .is_err());
        assert!(manager.contains(&record.id).unwrap());
        let output = thread::spawn(move || {
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        });
        let cleanup_manager = manager.clone();
        let cleanup_store = store.clone();
        let id = record.id.clone();
        let cleanup = thread::spawn(move || {
            child.wait().unwrap();
            let console = cleanup_manager
                .tasks
                .lock()
                .unwrap()
                .get_mut(&id)
                .unwrap()
                .master
                .take();
            drop(console);
            output.join().unwrap();
            cleanup_manager
                .finish_run(&cleanup_store, &id, "current", TaskStatus::Failed)
                .unwrap();
        });
        manager
            .stop_run_and_wait(&store, &record.id, "current", Duration::from_secs(5))
            .unwrap();
        assert!(!manager.contains(&record.id).unwrap());
        assert_eq!(
            store.get(&record.id).unwrap().unwrap().status,
            TaskStatus::Cancelled
        );
        cleanup.join().unwrap();
        manager
            .stop_run_and_wait(&store, &record.id, "current", Duration::from_millis(1))
            .unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn closing_console_drains_output_after_an_immediate_exit() {
        use std::io::{Read, Write};
        let pair = native_pty_system()
            .openpty(normalize_size(24, 80).unwrap())
            .unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let mut command = CommandBuilder::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-Command",
            "[Console]::Write('ready'); exit 0",
        ]);
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let output = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 4096];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let scan_start = bytes.len().saturating_sub(3);
                bytes.extend_from_slice(&buffer[..read]);
                // ConPTY requests a cursor position before allowing the child to write.
                if bytes[scan_start..]
                    .windows(4)
                    .any(|value| value == b"\x1b[6n")
                {
                    writer.write_all(b"\x1b[1;1R").unwrap();
                    writer.flush().unwrap();
                }
            }
            sender.send(bytes).unwrap();
        });
        let exited = (0..200).any(|_| {
            if child.try_wait().unwrap().is_some() {
                true
            } else {
                thread::sleep(Duration::from_millis(25));
                false
            }
        });
        if !exited {
            let _ = child.kill();
        }
        drop(pair.master);
        let bytes = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            exited,
            "child did not complete after terminal cursor response"
        );
        assert!(String::from_utf8_lossy(&bytes).contains("ready"));
        output.join().unwrap();
    }

    #[test]
    fn preserves_unicode_terminal_input_as_utf8() {
        let input = "中文宽字符 🙂 e\u{301}";
        assert_eq!(terminal_input_bytes(input), input.as_bytes());
        assert_eq!(
            String::from_utf8(terminal_input_bytes(input).to_vec()).unwrap(),
            input
        );
    }

    #[cfg(windows)]
    #[test]
    fn dropping_manager_terminates_owned_pty_children() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("test PTY should open");
        let mut command = CommandBuilder::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-Command",
            "Start-Sleep -Seconds 30",
        ]);
        let mut child = pair
            .slave
            .spawn_command(command)
            .expect("test child should start");
        let process = super::owned_process(child.as_ref()).unwrap();
        let writer = pair
            .master
            .take_writer()
            .expect("test PTY writer should open");
        let manager = PtyManager {
            tasks: Mutex::new(HashMap::from([(
                "drop-test".to_string(),
                PtyTask {
                    master: Some(pair.master),
                    writer,
                    process,
                    gate: std::sync::Arc::new(super::OutputGate::new("test-run".into())),
                },
            )])),
            changed: Default::default(),
        };

        drop(manager);

        let terminated = (0..40).any(|_| {
            if child
                .try_wait()
                .expect("test child should be queryable")
                .is_some()
            {
                true
            } else {
                thread::sleep(Duration::from_millis(25));
                false
            }
        });
        assert!(terminated, "dropping PtyManager left the child running");
    }

    #[test]
    fn accepts_only_known_bridge_statuses() {
        assert_eq!(parse_bridge_status("running"), Some(TaskStatus::Running));
        assert_eq!(parse_bridge_status("waiting\n"), Some(TaskStatus::Waiting));
        assert_eq!(parse_bridge_status("completed"), None);
    }

    #[test]
    fn resolves_tauri_development_directory_to_project_root() {
        let root = std::env::temp_dir().join(format!("deeppi-cwd-test-{}", std::process::id()));
        let tauri_directory = root.join("src-tauri");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&tauri_directory).expect("test directory should be created");
        std::fs::write(root.join("package.json"), "{}").expect("package marker should be written");

        let resolved = resolve_default_working_directory(tauri_directory);

        assert_eq!(resolved, root);
        std::fs::remove_dir_all(&resolved).expect("test directory should be removed");
    }

    #[test]
    fn rejects_zero_dimensions() {
        assert!(normalize_size(0, 80).is_err());
        assert!(normalize_size(24, 0).is_err());
    }

    #[test]
    fn accepts_normal_terminal_dimensions() {
        let size = normalize_size(36, 120).expect("normal terminal size should be valid");
        assert_eq!(size.rows, 36);
        assert_eq!(size.cols, 120);
    }

    #[test]
    fn rejects_unreasonably_large_dimensions() {
        assert!(normalize_size(1_001, 80).is_err());
        assert!(normalize_size(24, 1_001).is_err());
    }
}
