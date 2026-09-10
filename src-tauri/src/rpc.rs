use std::{
    collections::HashMap,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{ipc::Channel, AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::{
    app_paths::AppPaths,
    pty::PtyManager,
    rpc_transport::{RpcEvent, RpcTransport},
    runtime::RuntimeOperationLock,
    settings::SettingsStore,
    task::{TaskRecord, TaskStatus, TaskStore},
};

struct RpcRun {
    run_id: String,
    transport: Arc<RpcTransport>,
}

#[derive(Default)]
pub struct RpcManager {
    runs: Mutex<HashMap<String, RpcRun>>,
    pub(crate) diagnostics: Arc<crate::diagnostics::Diagnostics>,
}

impl RpcManager {
    pub(crate) fn contains(&self, task_id: &str) -> Result<bool, String> {
        Ok(self
            .runs
            .lock()
            .map_err(|_| "RPC manager lock is poisoned")?
            .contains_key(task_id))
    }
    pub fn count(&self) -> Result<usize, String> {
        Ok(self
            .runs
            .lock()
            .map_err(|_| "RPC manager lock is poisoned")?
            .len())
    }

    pub fn attach_run_ids(&self, records: &mut [TaskRecord]) -> Result<(), String> {
        let runs = self
            .runs
            .lock()
            .map_err(|_| "RPC manager lock is poisoned")?;
        for record in records {
            if let Some(run) = runs.get(&record.id) {
                record.run_id = Some(run.run_id.clone());
            }
        }
        Ok(())
    }

    fn get(&self, task_id: &str, run_id: &str) -> Result<Arc<RpcTransport>, String> {
        self.get_for_stop(task_id, run_id)?
            .ok_or_else(|| "RPC run is no longer current".into())
    }

    fn get_for_stop(
        &self,
        task_id: &str,
        run_id: &str,
    ) -> Result<Option<Arc<RpcTransport>>, String> {
        let runs = self
            .runs
            .lock()
            .map_err(|_| "RPC manager lock is poisoned")?;
        match runs.get(task_id) {
            None => Ok(None),
            Some(run) if run.run_id == run_id => Ok(Some(run.transport.clone())),
            Some(_) => Err("RPC run is no longer current".into()),
        }
    }

    pub fn stop_all(&self) -> Result<(), String> {
        let transports = self
            .runs
            .lock()
            .map_err(|_| "RPC manager lock is poisoned")?
            .values()
            .map(|run| run.transport.clone())
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        for transport in transports {
            if let Err(error) = transport.shutdown(Duration::from_secs(5)) {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

fn require_main(webview: &tauri::Webview) -> Result<(), String> {
    if webview.label() == "main" {
        Ok(())
    } else {
        Err("RPC requires the main Webview".into())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRpcRequest {
    project_id: String,
    title: String,
    task_id: Option<String>,
}

#[tauri::command]
pub async fn start_rpc_task(
    webview: tauri::Webview,
    app: AppHandle,
    request: StartRpcRequest,
) -> Result<TaskRecord, String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || start(&app, request))
        .await
        .map_err(|error| error.to_string())?
}

fn start(app: &AppHandle, request: StartRpcRequest) -> Result<TaskRecord, String> {
    let lifecycle = app.state::<RuntimeOperationLock>();
    let _permit = lifecycle.acquire()?;
    let paths = app.state::<AppPaths>();
    crate::snapshot::Snapshot::ensure_ready(&paths.backups)?;
    let manager = app.state::<RpcManager>();
    let pty = app.state::<PtyManager>();
    let maximum = app.state::<SettingsStore>().get()?.max_concurrent_tasks;
    if manager.count()? + pty.count()? >= usize::from(maximum) {
        return Err("maximum concurrent Pi tasks reached".into());
    }
    let cli = paths
        .available_pi_cli()?
        .ok_or("未找到本机或托管 Pi，请先在运行时设置中安装 Pi")?;
    let store = app.state::<TaskStore>();
    store.project_path(&request.project_id)?;
    let mut record = if let Some(task_id) = request.task_id {
        if pty.contains(&task_id)? || manager.contains(&task_id)? {
            return Err("Task is still running or stopping".into());
        }
        let existing = store.get(&task_id)?.ok_or("Task not found")?;
        if existing.agent != "pi" || existing.project_id.as_deref() != Some(&request.project_id) {
            return Err("Task does not belong to this project".into());
        }
        store.restart(&task_id)?
    } else {
        let environment =
            paths.new_pi_environment(&app.state::<SettingsStore>().get()?.pi_environment)?;
        let home = paths.pi_environment_home(environment)?;
        let mut record = store.create_pi_task_for_project(&request.project_id, &request.title)?;
        store.set_pi_environment(&record.id, environment, &home)?;
        record.pi_environment = environment.into();
        record.pi_agent_dir = Some(home.to_string_lossy().into_owned());
        record
    };
    store.set_interaction_mode(&record.id, "rpc")?;
    record.interaction_mode = "rpc".into();
    let run_id = Uuid::new_v4().to_string();
    record.run_id = Some(run_id.clone());
    let preparation = (|| {
        Ok::<_, String>((
            paths.task_pi_home(&record)?,
            crate::native_pi::session_arguments(&record)?,
        ))
    })();
    let (home, arguments) = match preparation {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = store.finish_if_active(&record.id, TaskStatus::Failed);
            return Err(error);
        }
    };
    let mut command = Command::new(paths.node_executable());
    command
        .arg(cli)
        .args(["--mode", "rpc"])
        .args(arguments)
        .current_dir(&record.project_path)
        .env("PI_CODING_AGENT_DIR", home);
    command.env_remove("PI_CODING_AGENT_SESSION_DIR");
    let task_id = record.id.clone();
    let exit_app = app.clone();
    let exit_run = run_id.clone();
    let mut runs = manager
        .runs
        .lock()
        .map_err(|_| "RPC manager lock is poisoned")?;
    let transport =
        match RpcTransport::spawn_observed(&mut command, manager.diagnostics.clone(), move |exit| {
            let manager = exit_app.state::<RpcManager>();
            if let Ok(mut runs) = manager.runs.lock() {
                if runs.get(&task_id).is_some_and(|run| run.run_id == exit_run) {
                    let status = if exit.stopped {
                        TaskStatus::Cancelled
                    } else if exit.code == Some(0) {
                        TaskStatus::Completed
                    } else {
                        TaskStatus::Failed
                    };
                    if let Err(error) = exit_app
                        .state::<TaskStore>()
                        .finish_if_active(&task_id, status)
                    {
                        log::error!("Failed to finish RPC task: {error}");
                    }
                    runs.remove(&task_id);
                    let _ = exit_app.emit_to(
                        "main",
                        "rpc-task-exit",
                        json!({
                            "taskId":task_id, "runId":exit_run, "status":status,
                        }),
                    );
                }
            };
        }) {
            Ok(transport) => Arc::new(transport),
            Err(error) => {
                let _ = store.finish_if_active(&record.id, TaskStatus::Failed);
                return Err(error);
            }
        };
    runs.insert(record.id.clone(), RpcRun { run_id, transport });
    Ok(record)
}

#[tauri::command]
pub fn subscribe_rpc(
    webview: tauri::Webview,
    manager: State<'_, RpcManager>,
    task_id: String,
    run_id: String,
    channel: Channel<RpcEvent>,
) -> Result<u64, String> {
    require_main(&webview)?;
    manager
        .get(&task_id, &run_id)?
        .subscribe(Box::new(move |event| {
            channel
                .send(event.clone())
                .map_err(|error| error.to_string())
        }))
}

fn validate_command(command: &Value) -> Result<&str, String> {
    let kind = command["type"]
        .as_str()
        .ok_or("RPC command type is required")?;
    if !matches!(
        kind,
        "prompt"
            | "steer"
            | "follow_up"
            | "get_state"
            | "get_messages"
            | "get_available_models"
            | "get_commands"
            | "set_model"
            | "set_thinking_level"
            | "clear_queue"
            | "abort"
            | "extension_ui_response"
            | "set_session_name"
    ) {
        return Err("RPC command is not supported by this host".into());
    }
    if matches!(kind, "prompt" | "steer" | "follow_up")
        && command["message"]
            .as_str()
            .is_none_or(|text| text.trim().is_empty() || text.len() > 512 * 1024)
    {
        return Err("Prompt must contain 1 to 524288 bytes".into());
    }
    Ok(kind)
}

#[tauri::command]
pub async fn rpc_command(
    webview: tauri::Webview,
    app: AppHandle,
    task_id: String,
    run_id: String,
    command: Value,
) -> Result<Value, String> {
    require_main(&webview)?;
    let kind = validate_command(&command)?.to_owned();
    let transport = app.state::<RpcManager>().get(&task_id, &run_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        if kind == "extension_ui_response" { transport.notify(command)?; return Ok(Value::Null); }
        if kind == "get_messages" {
            let reply = transport.request_with_cursor(command, Duration::from_secs(30))?;
            return Ok(json!({"messages":reply.data["messages"],"eventSequence":reply.sequence}));
        }
        let result = transport.request(command, Duration::from_secs(30)).map_err(|error| {
            if !kind.starts_with("get_") && (error.contains("timed out") || error.contains("process exited")) {
                format!("RPC_OUTCOME_UNKNOWN: 未收到请求确认，操作可能已经执行。请重新连接并检查会话，勿直接重复发送。({error})")
            } else { error }
        })?;
        if kind == "get_state" {
            app.state::<RpcManager>().get(&task_id, &run_id)?;
            if let (Some(id), Some(path)) = (result["sessionId"].as_str(), result["sessionFile"].as_str()) {
                let store = app.state::<TaskStore>();
                let task = store.get(&task_id)?.ok_or("Task not found")?;
                crate::native_pi::validate_reported_session(&app.state::<AppPaths>(), &task, id, path)?;
                store.set_session_file(&task_id, id, path)?;
            }
            let status = if result["isStreaming"] == true || result["isCompacting"] == true { TaskStatus::Running } else { TaskStatus::Waiting };
            app.state::<TaskStore>().set_active_status(&task_id, status)?;
        }
        Ok(result)
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn rpc_history_open(
    webview: tauri::Webview,
    app: AppHandle,
    task_id: String,
    run_id: String,
) -> Result<crate::rpc_history::HistoryPage, String> {
    require_main(&webview)?;
    let transport = app.state::<RpcManager>().get(&task_id, &run_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (snapshot, sequence) = transport.open_history(Duration::from_secs(60))?;
        app.state::<crate::rpc_history::HistoryStore>()
            .insert(&task_id, &run_id, snapshot, sequence)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn rpc_history_page(
    webview: tauri::Webview,
    app: AppHandle,
    task_id: String,
    run_id: String,
    snapshot_id: String,
    before: usize,
) -> Result<crate::rpc_history::HistoryPage, String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::rpc_history::HistoryStore>().page(
            &task_id,
            &run_id,
            &snapshot_id,
            before,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn rpc_history_close(
    webview: tauri::Webview,
    app: AppHandle,
    task_id: String,
    run_id: String,
    snapshot_id: String,
) -> Result<(), String> {
    require_main(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<crate::rpc_history::HistoryStore>()
            .close(&task_id, &run_id, &snapshot_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn stop_rpc_task(
    webview: tauri::Webview,
    manager: State<'_, RpcManager>,
    task_id: String,
    run_id: String,
) -> Result<(), String> {
    require_main(&webview)?;
    let Some(transport) = manager.get_for_stop(&task_id, &run_id)? else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || transport.shutdown(Duration::from_secs(5)))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_host_commands_and_rejects_empty_prompts() {
        assert!(validate_command(&json!({"type":"bash","command":"anything"})).is_err());
        assert!(validate_command(&json!({"type":"prompt","message":"  "})).is_err());
        assert!(validate_command(&json!({"type":"prompt","message":"hello"})).is_ok());
        assert!(validate_command(&json!({"type":"clear_queue"})).is_ok());
    }

    #[test]
    fn stopping_a_finished_run_is_idempotent_but_requests_still_fail() {
        let manager = RpcManager::default();
        assert!(manager
            .get_for_stop("finished", "old-run")
            .unwrap()
            .is_none());
        assert!(manager.get("finished", "old-run").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn a_stale_stop_cannot_target_a_replacement_rpc_process() {
        let manager = RpcManager::default();
        let mut command = Command::new("node.exe");
        command.args(["-e", "process.stdin.resume()"]);
        let transport = Arc::new(RpcTransport::spawn(&mut command, |_| {}).unwrap());
        manager.runs.lock().unwrap().insert(
            "task".into(),
            RpcRun {
                run_id: "new-run".into(),
                transport: transport.clone(),
            },
        );
        assert!(manager.get_for_stop("task", "old-run").is_err());
        assert!(manager.get_for_stop("task", "new-run").unwrap().is_some());
        transport.shutdown(Duration::from_secs(5)).unwrap();
    }
}
