use crate::message::msg;
use serde::Serialize;
use std::{
    collections::VecDeque,
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

const MAX_EVENTS: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    ProcessStarting,
    SpawnFailed,
    ProcessExit,
    StopRequested,
    StopTimeout,
    InputWriteFailed,
    OutputInvalid,
    StderrObserved,
    StderrReadFailed,
    RequestTimeout,
    ResponseFailed,
    InputQueueFull,
    ReplayGap,
    SpawnNotFound,
    SpawnDenied,
    ProcessOwnershipFailed,
    ProcessResumeFailed,
    FrameTooLarge,
    InvalidJson,
    IncompleteFrame,
    OutputReadFailed,
    HistoryFailed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEvent {
    sequence: u64,
    elapsed_ms: u64,
    run: u64,
    code: DiagnosticCode,
    count: u64,
    exit_code: Option<i32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    schema_version: u8,
    snapshot_id: String,
    app_version: &'static str,
    os: &'static str,
    arch: &'static str,
    elapsed_ms: u64,
    dropped_events: u64,
    events: Vec<DiagnosticEvent>,
}

#[derive(Default)]
struct Journal {
    events: VecDeque<DiagnosticEvent>,
    sequence: u64,
    next_run: u64,
    dropped: u64,
    preview: Option<(DiagnosticReport, Instant)>,
}

pub struct Diagnostics {
    inner: Mutex<Journal>,
    started: Instant,
    export_gate: Mutex<()>,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Journal::default()),
            started: Instant::now(),
            export_gate: Mutex::new(()),
        }
    }
}

impl Diagnostics {
    pub fn begin_run(&self) -> u64 {
        let run = if let Ok(mut journal) = self.inner.lock() {
            journal.next_run += 1;
            journal.next_run
        } else {
            return 0;
        };
        self.record(run, DiagnosticCode::ProcessStarting, 1, None);
        run
    }

    // No free-form input is accepted here: child output, paths and identifiers never enter the journal.
    pub fn record(&self, run: u64, code: DiagnosticCode, count: u64, exit_code: Option<i32>) {
        let Ok(mut journal) = self.inner.lock() else {
            return;
        };
        let elapsed_ms = self.elapsed_ms();
        if let Some(last) = journal.events.back_mut() {
            if last.run == run && last.code == code && last.exit_code == exit_code {
                last.count = last.count.saturating_add(count);
                last.elapsed_ms = elapsed_ms;
                return;
            }
        }
        journal.sequence += 1;
        let sequence = journal.sequence;
        journal.events.push_back(DiagnosticEvent {
            sequence,
            elapsed_ms,
            run,
            code,
            count,
            exit_code,
        });
        while journal.events.len() > MAX_EVENTS {
            journal.events.pop_front();
            journal.dropped = journal.dropped.saturating_add(1);
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    fn snapshot(&self) -> Result<DiagnosticReport, String> {
        let _gate = self
            .export_gate
            .try_lock()
            .map_err(|_| msg("diagnostics.export_in_progress"))?;
        let mut journal = self
            .inner
            .lock()
            .map_err(|_| msg("diagnostics.journal_unavailable"))?;
        let report = DiagnosticReport {
            schema_version: 1,
            snapshot_id: uuid::Uuid::new_v4().to_string(),
            app_version: env!("CARGO_PKG_VERSION"),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            elapsed_ms: self.elapsed_ms(),
            dropped_events: journal.dropped,
            events: journal.events.iter().cloned().collect(),
        };
        journal.preview = Some((report.clone(), Instant::now()));
        Ok(report)
    }

    fn export_bytes(&self, id: &str) -> Result<Vec<u8>, String> {
        let journal = self
            .inner
            .lock()
            .map_err(|_| msg("diagnostics.journal_unavailable"))?;
        let (report, _) = journal
            .preview
            .as_ref()
            .filter(|(report, created)| {
                report.snapshot_id == id && created.elapsed() < Duration::from_secs(600)
            })
            .ok_or(msg("diagnostics.preview_expired"))?;
        serde_json::to_vec_pretty(report).map_err(|_| msg("diagnostics.report_generate_failed"))
    }

    fn clear(&self) -> Result<(), String> {
        let _gate = self
            .export_gate
            .try_lock()
            .map_err(|_| msg("diagnostics.export_in_progress"))?;
        let mut journal = self
            .inner
            .lock()
            .map_err(|_| msg("diagnostics.journal_unavailable"))?;
        journal.events.clear();
        journal.dropped = 0;
        journal.preview = None;
        Ok(())
    }
}

fn write_export(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .file_name()
            .is_none_or(|name| name.to_string_lossy().contains(':'))
    {
        return Err(msg("diagnostics.report_path_required"));
    }
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        if !matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(
            prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_) | Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _)
        )) {
            return Err(msg("diagnostics.report_device_path"));
        }
        for component in path.components() {
            if let Component::Normal(name) = component {
                let name = name.to_string_lossy();
                let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
                if name.contains(':')
                    || name.ends_with(['.', ' '])
                    || matches!(
                        stem.as_str(),
                        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                    )
                    || (stem.len() == 4
                        && (stem.starts_with("COM") || stem.starts_with("LPT"))
                        && stem.as_bytes()[3].is_ascii_digit())
                {
                    return Err(msg("diagnostics.report_file_name_invalid"));
                }
            }
        }
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let mut file = options
        .open(path)
        .map_err(|_| msg("diagnostics.report_create_failed"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| msg("diagnostics.report_write_incomplete"))
}

fn require_main(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() == "main" {
        Ok(())
    } else {
        Err(msg("diagnostics.main_window_only"))
    }
}

#[tauri::command]
pub fn diagnostics_snapshot(
    webview: tauri::Webview,
    app: AppHandle,
) -> Result<DiagnosticReport, String> {
    require_main(&webview)?;
    app.state::<crate::rpc::RpcManager>().diagnostics.snapshot()
}

#[tauri::command]
pub fn diagnostics_clear(webview: tauri::Webview, app: AppHandle) -> Result<(), String> {
    require_main(&webview)?;
    app.state::<crate::rpc::RpcManager>().diagnostics.clear()
}

#[tauri::command]
pub async fn diagnostics_export(
    webview: tauri::Webview,
    app: AppHandle,
    snapshot_id: String,
    // 对话框标题由前端传入（前端掌握当前语言），后端不持有任何文案。
    dialog_title: String,
) -> Result<bool, String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<crate::rpc::RpcManager>();
        let _gate = manager
            .diagnostics
            .export_gate
            .try_lock()
            .map_err(|_| msg("diagnostics.export_in_progress"))?;
        let bytes = manager.diagnostics.export_bytes(&snapshot_id)?;
        let window = app
            .get_webview_window("main")
            .ok_or(msg("app.main_window_closed"))?;
        let Some(file) = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(dialog_title)
            .set_file_name(format!("deeppi-diagnostics-{}.json", uuid::Uuid::new_v4()))
            .add_filter("JSON", &["json"])
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file
            .into_path()
            .map_err(|_| msg("diagnostics.export_location_unsupported"))?;
        write_export(&path, &bytes)?;
        Ok(true)
    })
    .await
    .map_err(|_| msg("diagnostics.export_task_failed"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_events_coalesce_counts_without_storing_child_output() {
        let store = Diagnostics::default();
        let run = store.begin_run();
        store.record(run, DiagnosticCode::StderrObserved, 123, None);
        store.record(run, DiagnosticCode::StderrObserved, 7, None);
        let report = store.snapshot().unwrap();
        assert_eq!(report.events.last().unwrap().count, 130);
        for _ in 0..300 {
            store.record(run, DiagnosticCode::RequestTimeout, 1, None);
            store.record(run, DiagnosticCode::ResponseFailed, 1, None);
        }
        let report = store.snapshot().unwrap();
        assert_eq!(report.events.len(), MAX_EVENTS);
        assert!(report.dropped_events > 0);
        let json = serde_json::to_value(report).unwrap();
        assert!(json.get("stderr").is_none());
        assert!(json.get("taskId").is_none());
    }

    #[test]
    fn export_is_the_preview_snapshot_and_clear_invalidates_it() {
        let store = Diagnostics::default();
        let run = store.begin_run();
        let preview = store.snapshot().unwrap();
        store.record(run, DiagnosticCode::ProcessExit, 1, Some(23));
        let exported = store.export_bytes(&preview.snapshot_id).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&exported).unwrap(),
            serde_json::to_value(&preview).unwrap()
        );
        store.clear().unwrap();
        assert!(store.export_bytes(&preview.snapshot_id).is_err());
        assert!(store.snapshot().unwrap().events.is_empty());
        assert!(store.begin_run() > run);
    }

    #[test]
    fn replacement_preview_and_expiration_reject_stale_exports() {
        let store = Diagnostics::default();
        let old = store.snapshot().unwrap();
        let new = store.snapshot().unwrap();
        assert!(store.export_bytes(&old.snapshot_id).is_err());
        assert!(store.export_bytes(&new.snapshot_id).is_ok());
        store.inner.lock().unwrap().preview.as_mut().unwrap().1 =
            Instant::now() - Duration::from_secs(601);
        assert!(store.export_bytes(&new.snapshot_id).is_err());
    }

    #[test]
    fn export_creates_only_new_files_and_preserves_existing_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.json");
        write_export(&path, b"first").unwrap();
        assert!(write_export(&path, b"second").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        assert!(write_export(Path::new("relative.json"), b"no").is_err());
        assert!(write_export(&directory.path().join("report.json:stream"), b"no").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn real_transport_records_failures_and_exit_without_exporting_sensitive_stderr() {
        use std::{
            process::Command,
            sync::{mpsc, Arc},
        };
        let store = Arc::new(Diagnostics::default());
        let mut command = Command::new("node.exe");
        command.args(["-e", r#"
            process.stderr.write('Authorization: Bearer SECRET_TEST_ONLY\nC:\\private\\prompt.txt\n' + 'x'.repeat(300000));
            let pending = '';
            process.stdin.on('data', bytes => {
                pending += bytes;
                for (;;) {
                    const end = pending.indexOf('\n');
                    if (end < 0) break;
                    const request = JSON.parse(pending.slice(0, end));
                    pending = pending.slice(end + 1);
                    if (request.type === 'never') continue;
                    process.stdout.write(JSON.stringify({id: request.id, type: 'response', success: false, error: 'SECRET_TEST_ONLY'}) + '\n');
                }
            });
        "#]);
        let (sender, receiver) = mpsc::channel();
        let transport = crate::rpc_transport::RpcTransport::spawn_observed(
            &mut command,
            store.clone(),
            move |exit| {
                let _ = sender.send(exit);
            },
        )
        .unwrap();
        assert!(transport
            .request(serde_json::json!({"type":"fail"}), Duration::from_secs(5))
            .is_err());
        assert!(transport
            .request(
                serde_json::json!({"type":"never"}),
                Duration::from_millis(30)
            )
            .is_err());
        transport.shutdown(Duration::from_secs(5)).unwrap();
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(transport);
        let report = store.snapshot().unwrap();
        for expected in [
            DiagnosticCode::StderrObserved,
            DiagnosticCode::RequestTimeout,
            DiagnosticCode::ResponseFailed,
            DiagnosticCode::ProcessExit,
        ] {
            assert!(report.events.iter().any(|event| event.code == expected));
        }
        let bytes: u64 = report
            .events
            .iter()
            .filter(|event| event.code == DiagnosticCode::StderrObserved)
            .map(|event| event.count)
            .sum();
        assert!(bytes >= 300000);
        let exported = String::from_utf8(store.export_bytes(&report.snapshot_id).unwrap()).unwrap();
        for sensitive in [
            "SECRET_TEST_ONLY",
            "Authorization",
            "private",
            "prompt.txt",
            "Bearer",
        ] {
            assert!(!exported.contains(sensitive));
        }
        assert!(exported.len() < 64 * 1024);
    }

    #[test]
    fn failed_spawn_remains_diagnosable_without_executable_path() {
        let store = std::sync::Arc::new(Diagnostics::default());
        let root = tempfile::tempdir().unwrap();
        let mut command = std::process::Command::new(root.path().join("missing-secret-executable"));
        assert!(crate::rpc_transport::RpcTransport::spawn_observed(
            &mut command,
            store.clone(),
            |_| {}
        )
        .is_err());
        let report = store.snapshot().unwrap();
        assert!(report
            .events
            .iter()
            .any(|event| event.code == DiagnosticCode::SpawnNotFound));
        assert!(
            !String::from_utf8(store.export_bytes(&report.snapshot_id).unwrap())
                .unwrap()
                .contains("missing-secret-executable")
        );
    }

    #[test]
    fn clearing_is_rejected_during_export_and_preserves_the_preview() {
        let store = Diagnostics::default();
        let preview = store.snapshot().unwrap();
        let gate = store.export_gate.lock().unwrap();
        assert!(store.clear().is_err());
        assert!(store.snapshot().is_err());
        assert!(store.export_bytes(&preview.snapshot_id).is_ok());
        drop(gate);
        store.clear().unwrap();
        assert!(store.export_bytes(&preview.snapshot_id).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn export_rejects_device_names_before_attempting_io() {
        for path in [
            r"\\.\pipe\deeppi-diagnostics",
            r"\\?\GLOBALROOT\Device\Null",
            r"C:\CON.json",
            r"C:\NUL",
            r"C:\COM1.json",
        ] {
            assert!(write_export(Path::new(path), b"no").is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn real_malformed_output_distinguishes_json_errors_from_incomplete_frames() {
        use std::{
            process::Command,
            sync::{mpsc, Arc},
        };
        for (script, expected) in [
            (
                "process.stdout.write('SECRET_TEST_ONLY\\n')",
                DiagnosticCode::InvalidJson,
            ),
            (
                "process.stdout.write('{\"secret\":\"SECRET_TEST_ONLY\"}')",
                DiagnosticCode::IncompleteFrame,
            ),
        ] {
            let store = Arc::new(Diagnostics::default());
            let mut command = Command::new("node.exe");
            command.args(["-e", script]);
            let (sender, receiver) = mpsc::channel();
            let transport = crate::rpc_transport::RpcTransport::spawn_observed(
                &mut command,
                store.clone(),
                move |_| {
                    let _ = sender.send(());
                },
            )
            .unwrap();
            receiver.recv_timeout(Duration::from_secs(5)).unwrap();
            let report = store.snapshot().unwrap();
            assert!(report.events.iter().any(|event| event.code == expected));
            assert!(
                !String::from_utf8(store.export_bytes(&report.snapshot_id).unwrap())
                    .unwrap()
                    .contains("SECRET_TEST_ONLY")
            );
            transport.shutdown(Duration::from_secs(1)).unwrap();
        }
    }
}
