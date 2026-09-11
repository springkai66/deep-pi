use std::{
    io::Read,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const MAX_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
}

impl ProcessOutput {
    pub fn text(&self) -> String {
        let stdout = String::from_utf8_lossy(&self.stdout);
        let stderr = String::from_utf8_lossy(&self.stderr);
        let mut text = match (stdout.is_empty(), stderr.is_empty()) {
            (true, true) => String::new(),
            (false, true) => stdout.into_owned(),
            (true, false) => stderr.into_owned(),
            (false, false) => format!("{stdout}\n{stderr}"),
        };
        if self.truncated {
            text.push_str("\n[output truncated]");
        }
        text
    }
}

fn capture(
    mut reader: impl Read + Send + 'static,
) -> mpsc::Receiver<Result<(Vec<u8>, bool), String>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let result = (|| {
            let mut data = Vec::new();
            let mut buffer = [0_u8; 8192];
            let mut truncated = false;
            loop {
                let read = reader
                    .read(&mut buffer)
                    .map_err(|error| error.to_string())?;
                if read == 0 {
                    break;
                }
                let keep = read.min(MAX_CAPTURE_BYTES.saturating_sub(data.len()));
                data.extend_from_slice(&buffer[..keep]);
                truncated |= keep < read;
            }
            Ok((data, truncated))
        })();
        let _ = sender.send(result);
    });
    receiver
}

#[cfg(test)]
pub fn run(command: &mut Command, timeout: Duration) -> Result<ProcessOutput, String> {
    run_cancellable(command, timeout, None)
}

pub fn run_cancellable(
    command: &mut Command,
    timeout: Duration,
    cancellation: Option<&crate::operation::Cancellation>,
) -> Result<ProcessOutput, String> {
    if let Some(token) = cancellation {
        token.check()?;
    }
    if !cfg!(windows) {
        return Err("bounded command execution currently requires Windows".into());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (mut child, tree) = spawn_owned(command)?;
    let stdout = capture(child.stdout.take().ok_or("command stdout is unavailable")?);
    let stderr = capture(child.stderr.take().ok_or("command stderr is unavailable")?);
    let started = Instant::now();
    let result = loop {
        if let Some(token) = cancellation {
            if let Err(error) = token.check() {
                break Err(error);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(error) => break Err(format!("failed to wait for command: {error}")),
            Ok(None) if started.elapsed() >= timeout => break Err("command timed out".into()),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
        }
    };
    // Closing the job also ends descendants that inherited stdout/stderr handles.
    drop(tree);
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let status = result?;
    let (stdout, out_truncated) = stdout
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "command stdout did not close".to_string())??;
    let (stderr, err_truncated) = stderr
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "command stderr did not close".to_string())??;
    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
        truncated: out_truncated || err_truncated,
    })
}

pub(crate) fn spawn_owned(command: &mut Command) -> Result<(Child, ProcessTree), String> {
    spawn_owned_detailed(command).map_err(|failure| failure.message)
}

pub(crate) enum SpawnFailureKind {
    NotFound,
    PermissionDenied,
    Spawn,
    Ownership,
    Resume,
    Unsupported,
}

pub(crate) struct SpawnFailure {
    pub kind: SpawnFailureKind,
    pub message: String,
}

pub(crate) fn spawn_owned_detailed(
    command: &mut Command,
) -> Result<(Child, ProcessTree), SpawnFailure> {
    if !cfg!(windows) {
        return Err(SpawnFailure {
            kind: SpawnFailureKind::Unsupported,
            message: "owned command execution currently requires Windows".into(),
        });
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0004);
    }
    let mut child = command.spawn().map_err(|error| SpawnFailure {
        kind: match error.kind() {
            std::io::ErrorKind::NotFound => SpawnFailureKind::NotFound,
            std::io::ErrorKind::PermissionDenied => SpawnFailureKind::PermissionDenied,
            _ => SpawnFailureKind::Spawn,
        },
        message: format!("failed to start command: {error}"),
    })?;
    let tree = match ProcessTree::attach(&child) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SpawnFailure {
                kind: SpawnFailureKind::Ownership,
                message: error,
            });
        }
    };
    #[cfg(windows)]
    if let Err(error) = resume_child(&child) {
        drop(tree);
        let _ = child.kill();
        let _ = child.wait();
        return Err(SpawnFailure {
            kind: SpawnFailureKind::Resume,
            message: error,
        });
    }
    Ok((child, tree))
}

#[cfg(windows)]
fn resume_child(child: &Child) -> Result<(), String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };
    // Spawn suspended, assign the job, then resume: no child can escape before job assignment.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of_val(&entry) as u32;
    let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
    while found != 0 {
        if entry.th32OwnerProcessID == child.id() {
            let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if raw.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
            if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                return Err(std::io::Error::last_os_error().to_string());
            }
            return Ok(());
        }
        found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
    }
    Err("suspended command thread was not found".into())
}

#[cfg(windows)]
pub(crate) struct ProcessTree(std::os::windows::io::OwnedHandle);

#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &Child) -> Result<Self, String> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        // The handle is owned exclusively by this guard and never inherited.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let tree = Self(unsafe { OwnedHandle::from_raw_handle(job) });
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                tree.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(tree.0.as_raw_handle(), child.as_raw_handle()) }
                == 0
        {
            return Err(format!(
                "failed to own command process tree: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(tree)
    }
}

#[cfg(not(windows))]
pub(crate) struct ProcessTree;

#[cfg(not(windows))]
impl ProcessTree {
    fn attach(_: &Child) -> Result<Self, String> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn cancellation_stops_a_running_command() {
        let token = crate::operation::Cancellation::default();
        let worker_token = token.clone();
        let worker = thread::spawn(move || {
            run_cancellable(
                &mut shell("Start-Sleep -Seconds 30"),
                Duration::from_secs(60),
                Some(&worker_token),
            )
        });
        thread::sleep(Duration::from_millis(200));
        token.cancel();
        let started = Instant::now();
        assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(windows)]
    fn shell(script: &str) -> Command {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoLogo", "-NoProfile", "-Command", script]);
        command
    }

    #[cfg(windows)]
    #[test]
    fn bounds_captured_output_without_blocking_the_child() {
        let output = run(
            &mut shell("[Console]::Out.Write('x' * 200000); [Console]::Error.Write('y' * 200000)"),
            Duration::from_secs(60),
        )
        .unwrap();
        assert!(output.status.success());
        assert!(output.stdout.len() <= MAX_CAPTURE_BYTES);
        assert!(output.stderr.len() <= MAX_CAPTURE_BYTES);
        assert!(output.truncated);
    }

    #[cfg(windows)]
    #[test]
    fn terminates_a_command_at_its_deadline() {
        let started = Instant::now();
        let error = run(
            &mut shell("Start-Sleep -Seconds 30"),
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(error.contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(windows)]
    #[test]
    fn closes_descendant_processes_when_the_parent_exits() {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        };
        let output = run(&mut shell(
            "$child = Start-Process powershell.exe -WindowStyle Hidden -ArgumentList '-NoProfile -Command Start-Sleep -Seconds 30' -PassThru; [Console]::Write($child.Id)"
        ), Duration::from_secs(60)).unwrap();
        assert!(output.status.success());
        let pid = String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if !handle.is_null() {
            let wait = unsafe { WaitForSingleObject(handle, 10_000) };
            unsafe {
                CloseHandle(handle);
            }
            assert_eq!(wait, 0, "descendant process survived its owning command");
        }
    }
}
