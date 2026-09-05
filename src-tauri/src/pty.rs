use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    app_paths::AppPaths,
    bridge,
    task::{TaskRecord, TaskStatus, TaskStore},
};

const MAX_TERMINAL_DIMENSION: u16 = 1_000;

struct PtyTask {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct PtyManager {
    tasks: Mutex<HashMap<String, PtyTask>>,
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
    data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PtyExit {
    task_id: String,
    exit_code: Option<u32>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskStatusUpdate {
    task_id: String,
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
pub fn start_pi_task(
    app: AppHandle,
    manager: State<'_, PtyManager>,
    store: State<'_, TaskStore>,
    paths: State<'_, AppPaths>,
    request: StartPiTaskRequest,
) -> Result<TaskRecord, String> {
    let record = store.create_pi_task_for_project(&request.project_id, &request.title)?;
    if let Err(error) = launch_pi(app, &manager, &paths, &record, request.rows, request.cols) {
        let _ = store.finish_if_active(&record.id, TaskStatus::Failed);
        return Err(error);
    }
    Ok(record)
}

#[tauri::command]
pub fn restart_pi_task(
    app: AppHandle,
    manager: State<'_, PtyManager>,
    store: State<'_, TaskStore>,
    paths: State<'_, AppPaths>,
    request: RestartPiTaskRequest,
) -> Result<TaskRecord, String> {
    if manager
        .tasks
        .lock()
        .map_err(|_| lock_error())?
        .contains_key(&request.task_id)
    {
        return Err("Pi task is already running".into());
    }
    let record = store.restart(&request.task_id)?;
    if let Err(error) = launch_pi(app, &manager, &paths, &record, request.rows, request.cols) {
        let _ = store.finish_if_active(&record.id, TaskStatus::Failed);
        return Err(error);
    }
    Ok(record)
}

fn launch_pi(
    app: AppHandle,
    manager: &PtyManager,
    paths: &AppPaths,
    record: &TaskRecord,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let cwd = PathBuf::from(&record.project_path);
    if !cwd.is_dir() {
        return Err("task working directory does not exist".into());
    }
    let pair = native_pty_system()
        .openpty(normalize_size(rows, cols)?)
        .map_err(|error| format!("failed to create PTY: {error}"))?;

    let mut command = if let Some(pi_cli) = paths.pi_cli() {
        let mut command = CommandBuilder::new(paths.node_executable());
        command.arg(pi_cli);
        command
    } else {
        let mut command = CommandBuilder::new("powershell.exe");
        command.args(["-NoLogo", "-NoProfile", "-Command", "pi"]);
        command
    };
    command.args(["--session-id", &record.session_id, "--name", &record.title]);
    command.env("PI_CODING_AGENT_DIR", &paths.pi_home);
    command.env("DEEPPI_TASK_ID", &record.id);
    let state_file = bridge::state_file(paths, &record.id)?;
    let pipe_name = bridge::pipe_name(paths, &record.id)?;
    let _ = fs::remove_file(&state_file);
    command.env("DEEPPI_STATE_FILE", &state_file);
    #[cfg(windows)]
    command.env("DEEPPI_BRIDGE_PIPE", &pipe_name);
    command.cwd(cwd);

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("failed to start Pi: {error}"))?;
    let killer = child.clone_killer();
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to read PTY: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("failed to write PTY: {error}"))?;

    manager.tasks.lock().map_err(|_| lock_error())?.insert(
        record.id.clone(),
        PtyTask {
            master: pair.master,
            writer,
            killer,
        },
    );

    watch_bridge_status(app.clone(), record.id.clone(), state_file);
    #[cfg(windows)]
    watch_bridge_pipe(app.clone(), record.id.clone(), pipe_name);

    let output_app = app.clone();
    let output_task_id = record.id.clone();
    thread::spawn(move || {
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let payload = PtyOutput {
                        task_id: output_task_id.clone(),
                        data: STANDARD.encode(&buffer[..read]),
                    };
                    if output_app.emit("pty-output", payload).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = output_app.emit(
                        "pty-exit",
                        PtyExit {
                            task_id: output_task_id.clone(),
                            exit_code: None,
                            error: Some(format!("PTY read failed: {error}")),
                        },
                    );
                    break;
                }
            }
        }
    });

    let exit_app = app;
    let exit_task_id = record.id.clone();
    thread::spawn(move || {
        let result = child.wait();
        if let Some(state) = exit_app.try_state::<PtyManager>() {
            if let Ok(mut tasks) = state.tasks.lock() {
                tasks.remove(&exit_task_id);
            }
        }
        let (exit_code, error) = match result {
            Ok(status) => (Some(status.exit_code()), None),
            Err(error) => (None, Some(format!("failed to wait for Pi: {error}"))),
        };
        let final_status = if error.is_none() && exit_code == Some(0) {
            TaskStatus::Completed
        } else {
            TaskStatus::Failed
        };
        if let Some(store) = exit_app.try_state::<TaskStore>() {
            let _ = store.finish_if_active(&exit_task_id, final_status);
        }
        let _ = exit_app.emit(
            "pty-exit",
            PtyExit {
                task_id: exit_task_id,
                exit_code,
                error,
            },
        );
    });
    Ok(())
}

fn task_is_active(app: &AppHandle, task_id: &str) -> bool {
    app.try_state::<PtyManager>().is_some_and(|manager| {
        manager
            .tasks
            .lock()
            .is_ok_and(|tasks| tasks.contains_key(task_id))
    })
}

fn apply_bridge_status(
    app: &AppHandle,
    task_id: &str,
    status: TaskStatus,
    last_status: &mut Option<TaskStatus>,
) {
    if Some(status) == *last_status {
        return;
    }
    if let Some(store) = app.try_state::<TaskStore>() {
        if store.set_active_status(task_id, status).is_ok() {
            let _ = app.emit(
                "task-status",
                TaskStatusUpdate {
                    task_id: task_id.to_owned(),
                    status,
                },
            );
            *last_status = Some(status);
        }
    }
}

fn watch_bridge_status(app: AppHandle, task_id: String, state_file: PathBuf) {
    thread::spawn(move || {
        let mut last_status = None;
        loop {
            if !task_is_active(&app, &task_id) {
                break;
            }
            if let Some(status) = fs::read_to_string(&state_file)
                .ok()
                .and_then(|value| parse_bridge_status(&value))
            {
                apply_bridge_status(&app, &task_id, status, &mut last_status);
            }
            thread::sleep(Duration::from_millis(200));
        }
        let _ = fs::remove_file(state_file);
    });
}

#[cfg(windows)]
fn watch_bridge_pipe(app: AppHandle, task_id: String, pipe_name: String) {
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
        while task_is_active(&app, &task_id) {
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
                if !task_is_active(&app, &task_id) {
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
            loop {
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
                    apply_bridge_status(&app, &task_id, status, &mut last_status);
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
pub fn write_pi_task(
    manager: State<'_, PtyManager>,
    task_id: String,
    data: String,
) -> Result<(), String> {
    let mut tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| "Pi task was not found".to_string())?;
    task.writer
        .write_all(terminal_input_bytes(&data))
        .and_then(|_| task.writer.flush())
        .map_err(|error| format!("failed to write to Pi: {error}"))
}

#[tauri::command]
pub fn resize_pi_task(
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
        .resize(size)
        .map_err(|error| format!("failed to resize Pi: {error}"))
}

#[tauri::command]
pub fn stop_pi_task(
    manager: State<'_, PtyManager>,
    store: State<'_, TaskStore>,
    task_id: String,
) -> Result<(), String> {
    let mut tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let task = tasks
        .get_mut(&task_id)
        .ok_or_else(|| "Pi task was not found".to_string())?;
    task.killer
        .kill()
        .map_err(|error| format!("failed to stop Pi: {error}"))?;
    tasks.remove(&task_id);
    store.set_status(&task_id, TaskStatus::Cancelled)
}

#[tauri::command]
pub fn stop_all_pi_tasks(
    manager: State<'_, PtyManager>,
    store: State<'_, TaskStore>,
) -> Result<(), String> {
    let mut tasks = manager.tasks.lock().map_err(|_| lock_error())?;
    let running = tasks.drain().collect::<Vec<_>>();
    drop(tasks);
    let mut errors = Vec::new();
    for (task_id, mut task) in running {
        if let Err(error) = task.killer.kill() {
            errors.push(format!("{task_id}: {error}"));
            continue;
        }
        if let Err(error) = store.set_status(&task_id, TaskStatus::Cancelled) {
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
                let _ = task.killer.kill();
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
        let killer = child.clone_killer();
        let writer = pair
            .master
            .take_writer()
            .expect("test PTY writer should open");
        let manager = PtyManager {
            tasks: Mutex::new(HashMap::from([(
                "drop-test".to_string(),
                PtyTask {
                    master: pair.master,
                    writer,
                    killer,
                },
            )])),
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
