use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
    thread,
};

use base64::{engine::general_purpose::STANDARD, Engine};
#[cfg(not(windows))]
use portable_pty::ChildKiller;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::{settings::SettingsStore, task::TaskStore};

const MAX_TERMINAL_DIMENSION: u16 = 1_000;

struct ShellSession {
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Box<dyn Write + Send>,
    #[cfg(not(windows))]
    killer: Box<dyn ChildKiller + Send + Sync>,
    #[cfg(windows)]
    process: std::os::windows::io::OwnedHandle,
    run_id: String,
}

impl ShellSession {
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
                    return Err(format!("failed to stop shell: {error}"));
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
    unsafe { BorrowedHandle::borrow_raw(raw) }
        .try_clone_to_owned()
        .map_err(|error| error.to_string())
}

#[derive(Default)]
pub struct ShellManager {
    sessions: Mutex<HashMap<String, ShellSession>>,
}

impl Drop for ShellManager {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for session in sessions.values_mut() {
                let _ = session.kill();
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartShellRequest {
    project_id: String,
    rows: u16,
    cols: u16,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellSessionInfo {
    session_id: String,
    run_id: String,
    shell: String,
    cwd: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShellOutput {
    session_id: String,
    run_id: String,
    data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShellExit {
    session_id: String,
    run_id: String,
    exit_code: Option<u32>,
    error: Option<String>,
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

fn shell_command(shell: &str) -> Result<(&'static str, &'static [&'static str]), String> {
    match shell {
        "powershell" => {
            if cfg!(windows) {
                Ok(("powershell.exe", &["-NoLogo", "-NoProfile"]))
            } else {
                Ok(("powershell", &["-NoLogo", "-NoProfile"]))
            }
        }
        "pwsh" => Ok((
            if cfg!(windows) { "pwsh.exe" } else { "pwsh" },
            &["-NoLogo", "-NoProfile"],
        )),
        "bash" => Ok((
            if cfg!(windows) { "bash.exe" } else { "bash" },
            &["--noprofile", "--norc"],
        )),
        "cmd" if cfg!(windows) => Ok(("cmd.exe", &["/Q"])),
        "cmd" => Err("cmd shell is only supported on Windows".into()),
        _ => Err("unsupported terminal shell".into()),
    }
}

fn start_shell_inner(
    app: AppHandle,
    manager: &ShellManager,
    store: &TaskStore,
    settings: &SettingsStore,
    request: StartShellRequest,
) -> Result<ShellSessionInfo, String> {
    let cwd = PathBuf::from(store.project_path(&request.project_id)?);
    if !cwd.is_dir() {
        return Err("shell working directory does not exist".into());
    }
    let shell = settings.get()?.terminal_shell;
    let (program, args) = shell_command(&shell)?;
    let pair = native_pty_system()
        .openpty(normalize_size(request.rows, request.cols)?)
        .map_err(|error| format!("failed to create shell PTY: {error}"))?;
    let mut command = CommandBuilder::new(program);
    command.args(args.iter().copied());
    command.cwd(&cwd);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("failed to read shell PTY: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("failed to write shell PTY: {error}"))?;
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("failed to start {shell}: {error}"))?;
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

    let session_id = format!("shell-{}", Uuid::new_v4());
    let run_id = Uuid::new_v4().to_string();
    let mut sessions = manager
        .sessions
        .lock()
        .map_err(|_| "shell session state is unavailable".to_string())?;
    sessions.insert(
        session_id.clone(),
        ShellSession {
            master: Some(pair.master),
            writer,
            #[cfg(not(windows))]
            killer,
            #[cfg(windows)]
            process,
            run_id: run_id.clone(),
        },
    );
    drop(sessions);

    let output_app = app.clone();
    let output_session_id = session_id.clone();
    let output_run_id = run_id.clone();
    let output_thread = thread::spawn(move || {
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let payload = ShellOutput {
                        session_id: output_session_id.clone(),
                        run_id: output_run_id.clone(),
                        data: STANDARD.encode(&buffer[..read]),
                    };
                    if output_app.emit("shell-output", payload).is_err() {
                        break;
                    }
                }
                Err(error) => return Some(format!("shell PTY read failed: {error}")),
            }
        }
        None
    });

    let exit_app = app;
    let exit_session_id = session_id.clone();
    let exit_run_id = run_id.clone();
    thread::spawn(move || {
        let result = child.wait();
        let console = exit_app.try_state::<ShellManager>().and_then(|state| {
            state
                .sessions
                .lock()
                .ok()?
                .get_mut(&exit_session_id)
                .filter(|session| session.run_id == exit_run_id)
                .and_then(|session| session.master.take())
        });
        drop(console);
        let output_error = output_thread
            .join()
            .unwrap_or_else(|_| Some("shell PTY reader panicked".into()));
        let (exit_code, error) = match result {
            Ok(status) => (Some(status.exit_code()), output_error),
            Err(error) => (None, Some(format!("failed to wait for shell: {error}"))),
        };
        let finished = exit_app.try_state::<ShellManager>().is_some_and(|state| {
            state.sessions.lock().is_ok_and(|mut sessions| {
                if sessions
                    .get(&exit_session_id)
                    .is_some_and(|session| session.run_id == exit_run_id)
                {
                    sessions.remove(&exit_session_id);
                    true
                } else {
                    false
                }
            })
        });
        if finished {
            let _ = exit_app.emit(
                "shell-exit",
                ShellExit {
                    session_id: exit_session_id,
                    run_id: exit_run_id,
                    exit_code,
                    error,
                },
            );
        }
    });

    Ok(ShellSessionInfo {
        session_id,
        run_id,
        shell,
        cwd: cwd.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn start_shell(
    app: AppHandle,
    request: StartShellRequest,
) -> Result<ShellSessionInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        start_shell_inner(
            app.clone(),
            &app.state::<ShellManager>(),
            &app.state::<TaskStore>(),
            &app.state::<SettingsStore>(),
            request,
        )
    })
    .await
    .map_err(|error| format!("shell startup worker failed: {error}"))?
}

fn write_shell_inner(
    manager: &ShellManager,
    session_id: String,
    run_id: String,
    data: String,
) -> Result<(), String> {
    let mut sessions = manager
        .sessions
        .lock()
        .map_err(|_| "shell session state is unavailable".to_string())?;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| "shell session was not found".to_string())?;
    if session.run_id != run_id {
        return Err("shell run is no longer current".into());
    }
    session
        .writer
        .write_all(data.as_bytes())
        .and_then(|_| session.writer.flush())
        .map_err(|error| format!("failed to write to shell: {error}"))
}

#[tauri::command]
pub async fn write_shell(
    app: AppHandle,
    session_id: String,
    run_id: String,
    data: String,
) -> Result<(), String> {
    if data.len() > 1024 * 1024 {
        return Err("Terminal input exceeds 1 MiB".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        write_shell_inner(&app.state::<ShellManager>(), session_id, run_id, data)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn resize_shell(
    app: AppHandle,
    session_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let size = normalize_size(rows, cols)?;
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<ShellManager>();
        let sessions = manager
            .sessions
            .lock()
            .map_err(|_| "shell session state is unavailable".to_string())?;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| "shell session was not found".to_string())?;
        session
            .master
            .as_ref()
            .ok_or("shell session is exiting")?
            .resize(size)
            .map_err(|error| format!("failed to resize shell: {error}"))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn stop_shell(app: AppHandle, session_id: String, run_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<ShellManager>();
        let mut sessions = manager
            .sessions
            .lock()
            .map_err(|_| "shell session state is unavailable".to_string())?;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| "shell session was not found".to_string())?;
        if session.run_id != run_id {
            return Err("shell run is no longer current".into());
        }
        session.kill()
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::{normalize_size, shell_command};

    #[test]
    fn maps_supported_shells_to_fixed_commands() {
        assert!(shell_command("powershell").is_ok());
        assert!(shell_command("pwsh").is_ok());
        assert!(shell_command("bash").is_ok());
        assert!(cfg!(windows) == shell_command("cmd").is_ok());
    }

    #[test]
    fn rejects_unknown_shells_and_invalid_sizes() {
        assert!(shell_command("zsh").is_err());
        assert!(normalize_size(0, 80).is_err());
        assert!(normalize_size(24, 1_001).is_err());
        assert!(normalize_size(24, 120).is_ok());
    }
}
