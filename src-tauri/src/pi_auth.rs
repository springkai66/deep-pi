//! 官方账号 OAuth 登录桥（DeepPi → pi SDK）。
//!
//! 「设置 → 模型设置」里的官方登录走这里：Rust 侧拉起托管 Node 执行
//! `resources/pi-auth-bridge.mjs`，桥接用 pi 自己的 SDK（`ModelRuntime.login/
//! logout`、`AuthStorage`）完成 OAuth 流程，并把凭据写进 DeepPi 托管 pi 目录的
//! `auth.json`。DeepPi 只负责：进程管理、把登录过程的事件转发给界面、把界面
//! 的输入转回桥接，以及所有对外文本的脱敏。
//!
//! 边界（与目标契约一致）：不读取/不改动本机已安装 CLI 的凭据；不修改 pi
//! 运行时；令牌只写托管目录 auth.json，不出现在命令返回与事件负载中。

use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::message::{msg, msg_with};
use crate::{app_paths::AppPaths, provider::validate_provider_id};

/// include_str! 的桥接脚本源码，与 `bridge.rs` 的 pi 扩展同一打包模式：
/// 随应用携带、使用时按内容比对安装到托管目录，不依赖打包资源解析。
const BRIDGE_SOURCE: &str = include_str!("../resources/pi-auth-bridge.mjs");
const EVENT_NAME: &str = "pi-auth-event";
/// 冷启动含托管 Node 启动 + pi SDK import，放宽到 90 秒。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
/// respond/cancel/logout 等控制操作的超时。
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
/// 桥接输出单行上限：协议应答/事件都远小于该值，超限视为桥接异常，直接断开。
const MAX_LINE_BYTES: usize = 1024 * 1024;

/// 与桥接脚本一致的文本脱敏：任何 ≥32 字符的 `[A-Za-z0-9_-]` 连续串整体替换。
/// API Key / OAuth 令牌远长于普通单词，URL 的路径与短参数不受影响。
pub(crate) fn redact(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut run = String::new();
    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            run.push(character);
        } else {
            flush_run(&mut output, &mut run);
            output.push(character);
        }
    }
    flush_run(&mut output, &mut run);
    output
}

fn flush_run(output: &mut String, run: &mut String) {
    if run.chars().count() >= 32 {
        output.push_str("[redacted]");
    } else {
        output.push_str(run);
    }
    run.clear();
}

/// 事件下发前的兜底脱敏：只处理自由文本字段（error/message），URL 保留原文，
/// 否则授权链接里的 PKCE 参数会被破坏、无法回贴。
fn sanitize_event(event: &Value) -> Value {
    let mut value = event.clone();
    sanitize_fields(&mut value);
    value
}

fn sanitize_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, entry) in map.iter_mut() {
                if (key == "error" || key == "message") && entry.is_string() {
                    if let Some(text) = entry.as_str() {
                        *entry = Value::String(redact(text));
                    }
                } else {
                    sanitize_fields(entry);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                sanitize_fields(item);
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthProviderInfo {
    pub id: String,
    pub name: String,
    pub oauth: bool,
    pub oauth_name: Option<String>,
    pub is_subscription: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthCredentialSummary {
    pub provider: String,
    pub auth_type: String,
    pub expires: Option<u64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthStatusResponse {
    pub credentials: Vec<PiAuthCredentialSummary>,
    pub active_login: Option<String>,
    pub active_login_provider: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthLoginAck {
    pub login_id: String,
    pub provider: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthLoginRequest {
    provider_id: String,
    /// 界面预先选定的登录方式（如 openai-codex 的 `browser` / `device_code`）；
    /// 为 None 时由 pi 的默认流程决定，选择提示照常转发给界面。
    #[serde(default)]
    login_method: Option<String>,
}

/// 官方供应商可用模型（pi 运行时自带目录，官方登录后免 API Key）。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PiOfficialModel {
    pub id: String,
    pub name: String,
    pub context_window: Option<u64>,
    pub max_tokens: Option<u64>,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthProviderRef {
    provider_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthLoginRef {
    login_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthRespondRequest {
    login_id: String,
    prompt_id: String,
    value: String,
}

struct ActiveLogin {
    id: String,
    provider: String,
}

/// 一次桥接会话内共享的状态：等待应答的请求、活动登录标记与存活标记。
struct BridgeShared {
    pending: Mutex<HashMap<u64, mpsc::SyncSender<Value>>>,
    next_request_id: AtomicU64,
    dead: AtomicBool,
    active_login: Mutex<Option<ActiveLogin>>,
}

struct BridgeProcess {
    shared: Arc<BridgeShared>,
    stdin: Mutex<Option<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    _tree: crate::process_runner::ProcessTree,
}

impl Drop for BridgeProcess {
    fn drop(&mut self) {
        // 先关 stdin 触发桥接 readline close 自行退出，再兜底 kill。
        if let Ok(mut slot) = self.stdin.lock() {
            slot.take();
        }
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl BridgeProcess {
    fn request(&self, mut payload: Value, timeout: Duration) -> Result<Value, String> {
        if self.shared.dead.load(Ordering::Acquire) {
            return Err(msg("pi.auth.bridge_unavailable"));
        }
        let id = self.shared.next_request_id.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(object) = payload.as_object_mut() {
            object.insert("id".into(), json!(id));
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        self.shared
            .pending
            .lock()
            .expect("pi auth pending map poisoned")
            .insert(id, sender);
        let write_result = (|| -> Result<(), String> {
            let mut slot = self
                .stdin
                .lock()
                .map_err(|_| msg("pi.auth.internal_unavailable"))?;
            let stdin = slot
                .as_mut()
                .ok_or_else(|| msg("pi.auth.bridge_unavailable"))?;
            let body = serde_json::to_vec(&payload).map_err(|error| {
                msg_with("pi.auth.request_invalid", &[("error", &error.to_string())])
            })?;
            stdin
                .write_all(&body)
                .and_then(|()| stdin.write_all(b"\n"))
                .and_then(|()| stdin.flush())
                .map_err(|error| {
                    msg_with(
                        "pi.auth.request_write_failed",
                        &[("error", &error.to_string())],
                    )
                })
        })();
        if let Err(error) = write_result {
            self.shared
                .pending
                .lock()
                .map_err(|_| msg("pi.auth.internal_unavailable"))?
                .remove(&id);
            return Err(error);
        }
        match receiver.recv_timeout(timeout) {
            Ok(value) => {
                if value.get("ok").and_then(Value::as_bool) == Some(true) {
                    Ok(value)
                } else {
                    let message = value
                        .get("error")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| msg("pi.auth.bridge_request_failed"));
                    Err(redact(&message))
                }
            }
            Err(_) => {
                self.shared
                    .pending
                    .lock()
                    .map_err(|_| msg("pi.auth.internal_unavailable"))?
                    .remove(&id);
                Err(msg("pi.auth.bridge_timeout"))
            }
        }
    }

    fn active_login(&self) -> Option<(String, String)> {
        self.shared.active_login.lock().ok().and_then(|guard| {
            guard
                .as_ref()
                .map(|active| (active.id.clone(), active.provider.clone()))
        })
    }
}

fn fail_pending(shared: &BridgeShared, message: &str) {
    if let Ok(mut pending) = shared.pending.lock() {
        for (_, sender) in pending.drain() {
            let _ = sender.send(json!({ "ok": false, "error": message }));
        }
    }
}

/// 逐行读取桥接输出，行内不再有换行；单行超限返回错误（调用方断开桥接）。
/// 返回 `Ok(false)` 表示流已结束。
fn read_line_bounded<R: BufRead>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    maximum: usize,
) -> std::io::Result<bool> {
    buffer.clear();
    loop {
        let (take, done) = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                return Ok(false);
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let (take, done) = match newline {
                Some(index) => (index + 1, true),
                None => (available.len(), false),
            };
            if buffer.len() + take > maximum {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    msg("pi.auth.bridge_output_invalid"),
                ));
            }
            buffer.extend_from_slice(&available[..take]);
            (take, done)
        };
        reader.consume(take);
        if done {
            return Ok(true);
        }
    }
}

/// 桥接事件回调（测试收集器与 Tauri emit 共用）。
type EventEmitter = Box<dyn Fn(&Value) + Send + Sync>;

fn read_loop(
    stdout: ChildStdout,
    shared: Arc<BridgeShared>,
    emit: EventEmitter,
    child: Arc<Mutex<Child>>,
) {
    let mut reader = BufReader::new(stdout);
    let mut buffer: Vec<u8> = Vec::new();
    while let Ok(true) = read_line_bounded(&mut reader, &mut buffer, MAX_LINE_BYTES) {
        let Some(text) = std::str::from_utf8(&buffer).ok().map(str::trim) else {
            break;
        };
        if text.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        if value.get("reply").is_some() {
            if let Some(id) = value.get("reply").and_then(Value::as_u64) {
                if let Ok(mut pending) = shared.pending.lock() {
                    if let Some(sender) = pending.remove(&id) {
                        let _ = sender.send(value);
                    }
                }
            }
        } else if value.get("event").and_then(Value::as_str).is_some() {
            if value.get("event").and_then(Value::as_str) == Some("done") {
                if let Ok(mut active) = shared.active_login.lock() {
                    *active = None;
                }
            }
            emit(&value);
        }
    }
    shared.dead.store(true, Ordering::Release);
    fail_pending(&shared, &msg("pi.auth.bridge_exited"));
    if let Ok(mut active) = shared.active_login.lock() {
        *active = None;
    }
    if let Ok(mut child) = child.lock() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// pi SDK 入口（dist/index.js），来自托管 pi 运行时；从不回落到系统安装的 pi。
fn pi_sdk_index(paths: &AppPaths) -> Result<PathBuf, String> {
    let sdk = paths
        .managed_pi_runtime()?
        .join("node_modules")
        .join("@earendil-works")
        .join("pi-coding-agent")
        .join("dist")
        .join("index.js");
    if sdk.is_file() {
        Ok(sdk)
    } else {
        Err(msg("pi.auth.runtime_missing"))
    }
}

/// 把桥接脚本安装到托管缓存目录（内容不变则跳过写盘）。
fn install_bridge(paths: &AppPaths) -> Result<PathBuf, String> {
    let directory = paths.cache.join("auth-bridge");
    fs::create_dir_all(&directory).map_err(|error| {
        msg_with(
            "pi.auth.bridge_install_failed",
            &[("error", &error.to_string())],
        )
    })?;
    let destination = directory.join("pi-auth-bridge.mjs");
    if fs::read_to_string(&destination).ok().as_deref() == Some(BRIDGE_SOURCE) {
        return Ok(destination);
    }
    let mut file = atomic_write_file::AtomicWriteFile::open(&destination).map_err(|error| {
        msg_with(
            "pi.auth.bridge_install_failed",
            &[("error", &error.to_string())],
        )
    })?;
    file.write_all(BRIDGE_SOURCE.as_bytes()).map_err(|error| {
        msg_with(
            "pi.auth.bridge_install_failed",
            &[("error", &error.to_string())],
        )
    })?;
    file.commit().map_err(|error| {
        msg_with(
            "pi.auth.bridge_install_failed",
            &[("error", &error.to_string())],
        )
    })?;
    Ok(destination)
}

fn spawn_bridge(
    app: Option<&AppHandle>,
    paths: &AppPaths,
    inject: Option<&Path>,
    emit_override: Option<EventEmitter>,
) -> Result<BridgeProcess, String> {
    let script = install_bridge(paths)?;
    let sdk = pi_sdk_index(paths)?;
    let mut command = Command::new(paths.node_runtime()?);
    // 登录/刷新 token 的请求也要走代理。
    crate::proxy::apply_to_command(&mut command);
    command
        .arg(&script)
        .env("PI_AUTH_PI_SDK", &sdk)
        .env("PI_AUTH_AUTH_JSON", paths.pi_auth_file())
        .env("PI_CODING_AGENT_DIR", &paths.pi_home)
        .env("PI_TELEMETRY", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // OAuth 桥接也统一走常驻回环中继。系统代理关闭时不能读取
    // ProxyEnable=0 的旧 ProxyServer（否则登录仍会钉在过期端口上）；
    // 子进程环境由 apply_to_command 依据当前系统/手动模式注入。
    if let Some(inject) = inject {
        command.env("PI_AUTH_BRIDGE_TEST_INJECT", inject);
    }
    let (mut child, tree) = crate::process_runner::spawn_owned(&mut command)
        .map_err(|error| msg_with("pi.auth.bridge_launch_failed", &[("error", &error)]))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| msg("pi.auth.bridge_pipe_unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| msg("pi.auth.bridge_pipe_unavailable"))?;
    let shared = Arc::new(BridgeShared {
        pending: Mutex::new(HashMap::new()),
        next_request_id: AtomicU64::new(0),
        dead: AtomicBool::new(false),
        active_login: Mutex::new(None),
    });
    let emit: EventEmitter = match (emit_override, app) {
        (Some(override_emit), _) => override_emit,
        (None, Some(handle)) => {
            let handle = handle.clone();
            Box::new(move |event| {
                let _ = handle.emit(EVENT_NAME, sanitize_event(event));
            })
        }
        (None, None) => Box::new(|_| {}),
    };
    let reader_shared = shared.clone();
    let child = Arc::new(Mutex::new(child));
    let reader_child = child.clone();
    thread::spawn(move || read_loop(stdout, reader_shared, emit, reader_child));
    Ok(BridgeProcess {
        shared,
        stdin: Mutex::new(Some(stdin)),
        child,
        _tree: tree,
    })
}

/// 桥接进程管理：懒启动 + 按需重建（进程死亡后下一次调用自动拉起）。
#[derive(Default)]
pub struct PiAuthManager {
    bridge: Mutex<Option<Arc<BridgeProcess>>>,
    /// 串行化 start_login：先登记活动登录再放行下一次，避免并发登录。
    start_lock: Mutex<()>,
}

impl PiAuthManager {
    fn ensure(
        &self,
        app: Option<&AppHandle>,
        paths: &AppPaths,
    ) -> Result<Arc<BridgeProcess>, String> {
        let mut guard = self
            .bridge
            .lock()
            .map_err(|_| msg("pi.auth.internal_unavailable"))?;
        if let Some(bridge) = guard.as_ref() {
            if !bridge.shared.dead.load(Ordering::Acquire) {
                return Ok(bridge.clone());
            }
        }
        let bridge = Arc::new(spawn_bridge(app, paths, None, None)?);
        *guard = Some(bridge.clone());
        Ok(bridge)
    }

    /// 回收当前桥接进程（下一命令按需重建）。取消登录后调用，确保被中断
    /// 会话的 active_login 标记随之消失，不会卡住后续的重新登录。
    fn reset(&self) {
        if let Ok(mut guard) = self.bridge.lock() {
            *guard = None;
        }
    }
}

#[tauri::command]
pub async fn pi_auth_providers(app: AppHandle) -> Result<Vec<PiAuthProviderInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<PiAuthManager>();
        let paths = app.state::<AppPaths>();
        let bridge = manager.ensure(Some(&app), &paths)?;
        let value = bridge.request(json!({ "op": "providers" }), REQUEST_TIMEOUT)?;
        serde_json::from_value::<Vec<PiAuthProviderInfo>>(
            value.get("providers").cloned().unwrap_or(Value::Null),
        )
        .map_err(|error| {
            msg_with(
                "pi.auth.providers_invalid",
                &[("error", &error.to_string())],
            )
        })
    })
    .await
    .map_err(|error| msg_with("pi.auth.worker_failed", &[("error", &error.to_string())]))?
}

#[tauri::command]
pub async fn pi_auth_status(app: AppHandle) -> Result<PiAuthStatusResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<PiAuthManager>();
        let paths = app.state::<AppPaths>();
        let bridge = manager.ensure(Some(&app), &paths)?;
        let value = bridge.request(json!({ "op": "status" }), REQUEST_TIMEOUT)?;
        let credentials = serde_json::from_value::<Vec<PiAuthCredentialSummary>>(
            value.get("credentials").cloned().unwrap_or(Value::Null),
        )
        .map_err(|error| msg_with("pi.auth.status_invalid", &[("error", &error.to_string())]))?;
        // Rust 侧的活动登录标记优先；桥接自报值作兜底（覆盖标记滞后或进程
        // 重建等极端时序），保证界面总能看到进行中的登录并重新接上面板。
        let (active_login, active_login_provider) = match bridge.active_login() {
            Some((id, provider)) => (Some(id), Some(provider)),
            None => {
                let login = value
                    .get("activeLogin")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let provider = value
                    .get("activeLoginProvider")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                (login.clone(), provider.or(login))
            }
        };
        Ok(PiAuthStatusResponse {
            credentials,
            active_login,
            active_login_provider,
        })
    })
    .await
    .map_err(|error| msg_with("pi.auth.worker_failed", &[("error", &error.to_string())]))?
}

#[tauri::command]
pub async fn pi_auth_provider_models(
    app: AppHandle,
    request: PiAuthProviderRef,
) -> Result<Vec<PiOfficialModel>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_provider_id(&request.provider_id)?;
        let manager = app.state::<PiAuthManager>();
        let paths = app.state::<AppPaths>();
        let bridge = manager.ensure(Some(&app), &paths)?;
        let value = bridge.request(
            json!({ "op": "models", "provider": request.provider_id }),
            REQUEST_TIMEOUT,
        )?;
        serde_json::from_value::<Vec<PiOfficialModel>>(
            value.get("models").cloned().unwrap_or(Value::Null),
        )
        .map_err(|error| msg_with("pi.auth.models_invalid", &[("error", &error.to_string())]))
    })
    .await
    .map_err(|error| msg_with("pi.auth.worker_failed", &[("error", &error.to_string())]))?
}

#[tauri::command]
pub async fn pi_auth_start_login(
    app: AppHandle,
    request: PiAuthLoginRequest,
) -> Result<PiAuthLoginAck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_provider_id(&request.provider_id)?;
        let manager = app.state::<PiAuthManager>();
        let paths = app.state::<AppPaths>();
        let _serial = manager
            .start_lock
            .lock()
            .map_err(|_| msg("pi.auth.internal_unavailable"))?;
        let bridge = manager.ensure(Some(&app), &paths)?;
        if let Some((_, provider)) = bridge.active_login() {
            return Err(msg_with("pi.auth.login_busy", &[("provider", &provider)]));
        }
        let ack = bridge.request(
            json!({
                "op": "login",
                "provider": request.provider_id,
                "method": request.login_method,
            }),
            REQUEST_TIMEOUT,
        )?;
        let login_id = ack
            .get("login")
            .and_then(Value::as_str)
            .ok_or_else(|| msg("pi.auth.login_not_acknowledged"))?
            .to_owned();
        *bridge
            .shared
            .active_login
            .lock()
            .map_err(|_| "pi auth active login poisoned")? = Some(ActiveLogin {
            id: login_id.clone(),
            provider: request.provider_id.clone(),
        });
        Ok(PiAuthLoginAck {
            login_id,
            provider: request.provider_id,
        })
    })
    .await
    .map_err(|error| msg_with("pi.auth.worker_failed", &[("error", &error.to_string())]))?
}

fn assert_active_login(bridge: &BridgeProcess, login_id: &str) -> Result<(), String> {
    match bridge.active_login() {
        Some((id, _)) if id == login_id => Ok(()),
        Some((_, provider)) => Err(msg_with(
            "pi.auth.login_not_owned",
            &[("provider", &provider)],
        )),
        None => Err(msg("pi.auth.login_missing")),
    }
}

#[tauri::command]
pub async fn pi_auth_respond(app: AppHandle, request: PiAuthRespondRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<PiAuthManager>();
        let paths = app.state::<AppPaths>();
        let bridge = manager.ensure(Some(&app), &paths)?;
        assert_active_login(&bridge, &request.login_id)?;
        bridge
            .request(
                json!({
                    "op": "respond",
                    "login": request.login_id,
                    "promptId": request.prompt_id,
                    "value": request.value,
                }),
                CONTROL_TIMEOUT,
            )
            .map(|_| ())
    })
    .await
    .map_err(|error| msg_with("pi.auth.worker_failed", &[("error", &error.to_string())]))?
}

#[tauri::command]
pub async fn pi_auth_cancel(app: AppHandle, request: PiAuthLoginRef) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let manager = app.state::<PiAuthManager>();
        let paths = app.state::<AppPaths>();
        let bridge = manager.ensure(Some(&app), &paths)?;
        assert_active_login(&bridge, &request.login_id)?;
        let result = bridge
            .request(
                json!({ "op": "cancel", "login": request.login_id }),
                CONTROL_TIMEOUT,
            )
            .map(|_| ());
        if result.is_ok() {
            // 取消成功即回收桥接进程：被中断的 pi 登录可能残留回调服务器与
            // 轮询任务；下一次命令按需重建进程，active_login 标记随之消失，
            // 保证「取消 → 立即重新登录」总能成功。
            manager.reset();
        }
        result
    })
    .await
    .map_err(|error| msg_with("pi.auth.worker_failed", &[("error", &error.to_string())]))?
}

#[tauri::command]
pub async fn pi_auth_logout(app: AppHandle, request: PiAuthProviderRef) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        validate_provider_id(&request.provider_id)?;
        let manager = app.state::<PiAuthManager>();
        let paths = app.state::<AppPaths>();
        let bridge = manager.ensure(Some(&app), &paths)?;
        bridge
            .request(
                json!({ "op": "logout", "provider": request.provider_id }),
                CONTROL_TIMEOUT,
            )
            .map(|_| ())
    })
    .await
    .map_err(|error| msg_with("pi.auth.worker_failed", &[("error", &error.to_string())]))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_long_token_runs_only() {
        // 用明显非真实的前缀（避免 gitleaks 等扫描器把测试常量误报为真实密钥）。
        let token = format!("fake-oauth-access-{}", "x".repeat(64));
        let redacted = redact(&format!("refresh failed for {token} (grant invalid)"));
        assert_eq!(redacted, "refresh failed for [redacted] (grant invalid)");
        // 短词、路径与查询参数不受影响。
        assert_eq!(
            redact("HTTP 401 at /v1/oauth/token?id=12345"),
            "HTTP 401 at /v1/oauth/token?id=12345"
        );
        // URL 中的 43 字符 PKCE challenge 同样被收敛——错误文本不允许携带它。
        let challenge = "a".repeat(43);
        assert!(redact(&challenge).contains("[redacted]"));
    }

    #[test]
    fn sanitizes_error_and_message_fields_only() {
        let event = json!({
            "event": "done",
            "ok": false,
            "provider": "anthropic",
            "error": format!("token exchange failed: {}", "t".repeat(64)),
            "nested": { "message": format!("m{}", "u".repeat(40)) },
        });
        let sanitized = sanitize_event(&event);
        assert!(sanitized["error"].as_str().unwrap().contains("[redacted]"));
        assert!(sanitized["nested"]["message"]
            .as_str()
            .unwrap()
            .contains("[redacted]"));
        assert_eq!(sanitized["provider"], json!("anthropic"));
    }

    #[test]
    fn parses_official_model_list() {
        let models: Vec<PiOfficialModel> = serde_json::from_value(json!([
            {
                "id": "gpt-5.3-codex-spark",
                "name": "GPT-5.3 Codex Spark",
                "contextWindow": 128000,
                "maxTokens": 128000,
                "reasoning": true,
                "input": ["text"],
                "inputCost": 1.75,
                "outputCost": 14.0
            },
            {
                "id": "k3",
                "name": "Kimi K3",
                "contextWindow": null,
                "maxTokens": null,
                "reasoning": false,
                "input": [],
                "inputCost": null,
                "outputCost": null
            }
        ]))
        .unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].context_window, Some(128_000));
        assert_eq!(models[0].input_cost, Some(1.75));
        assert!(models[0].reasoning);
        assert_eq!(models[1].context_window, None);
        assert_eq!(models[1].output_cost, None);
    }

    #[test]
    fn parses_provider_list_and_status_replies() {
        let providers: Vec<PiAuthProviderInfo> = serde_json::from_value(json!([
            { "id": "anthropic", "name": "Anthropic", "oauth": true, "oauthName": "Anthropic (Claude Pro/Max)", "isSubscription": true },
            { "id": "deepseek", "name": "DeepSeek", "oauth": false, "oauthName": null, "isSubscription": false },
        ]))
        .unwrap();
        assert_eq!(providers.len(), 2);
        assert_eq!(
            providers[0].oauth_name.as_deref(),
            Some("Anthropic (Claude Pro/Max)")
        );
        assert!(!providers[1].oauth);

        let status: Vec<PiAuthCredentialSummary> = serde_json::from_value(json!([
            { "provider": "anthropic", "authType": "oauth", "expires": 1_900_000_000_000u64 },
            { "provider": "openai", "authType": "api_key", "expires": null },
        ]))
        .unwrap();
        assert_eq!(status[0].auth_type, "oauth");
        assert_eq!(status[1].expires, None);
    }
}

/// 需要真实托管 Node + pi SDK 的桥接链路测试（与 rpc_integration_tests 同模式，
/// 默认跳过，`cargo test -- --include-ignored` 时执行）。
#[cfg(all(test, windows))]
mod bridge_tests {
    use super::*;

    const EVENT_TIMEOUT: Duration = Duration::from_secs(60);

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// 开发机的托管运行时（仓库内 `.deeppi-runtime/runtimes`）；缺失则跳过测试。
    fn fixture_with_dev_runtime() -> Option<(AppPaths, Fixture)> {
        let runtimes = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()?
            .join(".deeppi-runtime")
            .join("runtimes");
        if !runtimes.join("node/active.json").is_file()
            || !runtimes.join("pi/active.json").is_file()
        {
            return None;
        }
        let fixture =
            Fixture(std::env::temp_dir().join(format!("deeppi-pi-auth-{}", uuid::Uuid::new_v4())));
        let paths = AppPaths::from_roots_with_runtimes(
            fixture.0.join("roaming"),
            fixture.0.join("local"),
            runtimes,
            fixture.0.join("project"),
        )
        .ok()?;
        Some((paths, fixture))
    }

    type EventSink = Box<dyn Fn(&Value) + Send + Sync>;

    fn collector() -> (EventSink, mpsc::Receiver<Value>) {
        let (sender, receiver) = mpsc::channel();
        let sink: EventSink = Box::new(move |event: &Value| {
            let _ = sender.send(event.clone());
        });
        (sink, receiver)
    }

    fn wait_event(receiver: &mpsc::Receiver<Value>, event: &str) -> Result<Value, String> {
        let deadline = std::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(format!("timed out waiting for event: {event}"));
            }
            match receiver.recv_timeout(remaining.min(Duration::from_secs(1))) {
                Ok(value) => {
                    if value.get("event").and_then(Value::as_str) == Some(event) {
                        return Ok(value);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("pi auth bridge event channel closed".into());
                }
            }
        }
    }

    fn drain_until_done(receiver: &mpsc::Receiver<Value>) -> Result<Value, String> {
        let deadline = std::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err("timed out waiting for the done event".into());
            }
            match receiver.recv_timeout(remaining) {
                Ok(value) => {
                    if value.get("event").and_then(Value::as_str) == Some("done") {
                        return Ok(value);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err("timed out waiting for the done event".into());
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("pi auth bridge event channel closed".into());
                }
            }
        }
    }

    /// 登录 provider 列表必须覆盖 pi 内置的全部 OAuth 通道。
    #[test]
    #[ignore = "requires the managed Pi SDK; exercised with --include-ignored"]
    fn bridge_lists_official_oauth_providers() {
        let Some((paths, _fixture)) = fixture_with_dev_runtime() else {
            return;
        };
        let (emit, _events) = collector();
        let bridge = spawn_bridge(None, &paths, None, Some(emit)).expect("bridge should spawn");
        let reply = bridge
            .request(json!({ "op": "providers" }), REQUEST_TIMEOUT)
            .expect("providers request should succeed");
        let providers: Vec<PiAuthProviderInfo> =
            serde_json::from_value(reply["providers"].clone()).unwrap();
        for id in [
            "anthropic",
            "openai-codex",
            "openrouter",
            "github-copilot",
            "kimi-coding",
            "xai",
        ] {
            let provider = providers
                .iter()
                .find(|provider| provider.id == id)
                .unwrap_or_else(|| panic!("provider {id} should support official login"));
            assert!(provider.oauth, "provider {id} should expose OAuth login");
            assert!(provider.oauth_name.is_some());
        }
        assert!(
            providers
                .iter()
                .all(|provider| provider.id != "deepseek" || !provider.oauth),
            "API-key providers must not offer official login"
        );
    }

    /// 模拟 OAuth 登录：替身运行时驱动完整交互（auth_url → manual_code 提示 →
    /// 用户回贴 → 凭据落盘），auth.json 必须出现 pi 标准格式的 oauth 凭据；
    /// logout 删除条目；所有事件与应答不得携带令牌明文。
    #[test]
    #[ignore = "requires the managed Pi SDK; exercised with --include-ignored"]
    fn simulated_login_persists_oauth_credential_and_logout_removes_it() {
        let Some((paths, fixture)) = fixture_with_dev_runtime() else {
            return;
        };
        let access_token = format!("access-token-{}", "A".repeat(48));
        let refresh_token = format!("refresh-token-{}", "B".repeat(48));
        let inject = fixture.0.join("inject.mjs");
        fs::write(
            &inject,
            format!(
                r#"import {{ pathToFileURL }} from "node:url";

export default {{
  async createRuntime(pi) {{
    const sdk = pathToFileURL(process.env.PI_AUTH_PI_SDK).href;
    const storage = await import(new URL("core/auth-storage.js", sdk).href);
    const authPath = process.env.PI_AUTH_AUTH_JSON;
    return {{
      async getProviders() {{
        return [{{ id: "fixture-oauth", name: "Fixture", auth: {{ oauth: {{ name: "Fixture (Pro)", isSubscription: true }} }} }}];
      }},
      async listCredentials() {{
        const stored = storage.readStoredCredential("fixture-oauth", authPath);
        return stored ? [{{ providerId: "fixture-oauth", type: stored.type }}] : [];
      }},
      async login(providerId, type, interaction) {{
        interaction.notify({{ type: "auth_url", url: "https://fixture.example/authorize?state=abc" }});
        await interaction.prompt({{ type: "manual_code", message: "Paste the authorization code" }});
        const credential = {{ type: "oauth", access: "{access_token}", refresh: "{refresh_token}", expires: Date.now() + 3600_000 }};
        await storage.AuthStorage.create(authPath).modify(providerId, async () => credential);
        return credential;
      }},
    }};
  }},
}};"#
            ),
        )
        .unwrap();
        let (emit, events) = collector();
        let bridge =
            spawn_bridge(None, &paths, Some(&inject), Some(emit)).expect("bridge should spawn");

        let ack = bridge
            .request(
                json!({ "op": "login", "provider": "fixture-oauth" }),
                REQUEST_TIMEOUT,
            )
            .expect("login should be acknowledged");
        let login_id = ack["login"].as_str().unwrap().to_owned();

        let prompt = wait_event(&events, "prompt").unwrap();
        assert_eq!(prompt["kind"], json!("manual_code"));
        let prompt_id = prompt["promptId"].as_str().unwrap().to_owned();

        bridge
            .request(
                json!({ "op": "respond", "login": login_id, "promptId": prompt_id, "value": "manual-code" }),
                CONTROL_TIMEOUT,
            )
            .expect("respond should succeed");

        let done = drain_until_done(&events).unwrap();
        assert_eq!(done["ok"], json!(true));
        assert_eq!(done["provider"], json!("fixture-oauth"));

        let auth: Value = serde_json::from_str(
            &fs::read_to_string(paths.pi_auth_file()).expect("auth.json should exist"),
        )
        .unwrap();
        assert_eq!(auth["fixture-oauth"]["type"], json!("oauth"));

        let status_reply = bridge
            .request(json!({ "op": "status" }), REQUEST_TIMEOUT)
            .expect("status request should succeed");
        let status_text = status_reply.to_string();
        assert!(!status_text.contains(&access_token));
        assert!(!status_text.contains(&refresh_token));
        let credentials: Vec<PiAuthCredentialSummary> =
            serde_json::from_value(status_reply["credentials"].clone()).unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].provider, "fixture-oauth");
        assert_eq!(credentials[0].auth_type, "oauth");
        assert!(credentials[0].expires.is_some());

        bridge
            .request(
                json!({ "op": "logout", "provider": "fixture-oauth" }),
                CONTROL_TIMEOUT,
            )
            .expect("logout should succeed");
        let auth: Value = serde_json::from_str(
            &fs::read_to_string(paths.pi_auth_file()).expect("auth.json should still exist"),
        )
        .unwrap();
        assert!(
            auth.get("fixture-oauth").is_none(),
            "logout must remove the entry"
        );
    }

    /// 真实 pi 登录流程（无替身）：openai-codex 的浏览器登录先弹出登录方式选择，
    /// 取消后整个流程干净结束，不产生任何凭据，也不泄漏任何令牌。
    #[test]
    #[ignore = "requires the managed Pi SDK; exercised with --include-ignored"]
    fn real_pi_login_flow_prompts_select_then_cancels_cleanly() {
        let Some((paths, _fixture)) = fixture_with_dev_runtime() else {
            return;
        };
        let (emit, events) = collector();
        let bridge = spawn_bridge(None, &paths, None, Some(emit)).expect("bridge should spawn");
        let ack = bridge
            .request(
                json!({ "op": "login", "provider": "openai-codex" }),
                REQUEST_TIMEOUT,
            )
            .expect("login should be acknowledged");
        let login_id = ack["login"].as_str().unwrap().to_owned();

        let prompt = wait_event(&events, "prompt").expect("select prompt should arrive");
        assert_eq!(prompt["kind"], json!("select"));
        assert!(!prompt["options"].as_array().unwrap().is_empty());

        bridge
            .request(
                json!({ "op": "cancel", "login": login_id }),
                CONTROL_TIMEOUT,
            )
            .expect("cancel should succeed");

        // 取消后桥接先关闭等待中的 prompt，再结束登录；事件顺序：prompt_closed → done。
        let prompt_closed = wait_event(&events, "prompt_closed").unwrap();
        assert_eq!(prompt_closed["promptId"], prompt["promptId"]);
        let done = drain_until_done(&events).unwrap();
        assert_eq!(done["ok"], json!(false));
        assert!(done["error"].as_str().unwrap().len() < 512);
    }

    /// 取消后立即重新登录：桥内会话标记已释放，第二次 login 直接得到 ack。
    #[test]
    #[ignore = "requires the managed Pi SDK; exercised with --include-ignored"]
    fn cancel_then_second_login_starts_cleanly() {
        let Some((paths, _fixture)) = fixture_with_dev_runtime() else {
            return;
        };
        let (emit, events) = collector();
        let bridge = spawn_bridge(None, &paths, None, Some(emit)).expect("bridge should spawn");
        let ack = bridge
            .request(
                json!({ "op": "login", "provider": "openai-codex" }),
                REQUEST_TIMEOUT,
            )
            .expect("first login should be acknowledged");
        let login_id = ack["login"].as_str().unwrap().to_owned();
        wait_event(&events, "prompt").expect("select prompt should arrive");
        bridge
            .request(
                json!({ "op": "cancel", "login": login_id }),
                CONTROL_TIMEOUT,
            )
            .expect("cancel should succeed");
        let done = drain_until_done(&events).unwrap();
        assert_eq!(done["ok"], json!(false));
        // 取消后立即重登：桥内 activeLogins 已随 done 清空。
        let ack2 = bridge
            .request(
                json!({ "op": "login", "provider": "openai-codex" }),
                REQUEST_TIMEOUT,
            )
            .expect("second login should start right after cancel");
        let second = ack2["login"].as_str().unwrap().to_owned();
        assert_ne!(second, login_id);
        bridge
            .request(json!({ "op": "cancel", "login": second }), CONTROL_TIMEOUT)
            .expect("cleanup cancel should succeed");
    }

    /// status 应答之后重发进行中登录的等待 prompt，供界面重连恢复面板。
    #[test]
    #[ignore = "requires the managed Pi SDK; exercised with --include-ignored"]
    fn status_replays_pending_prompt_of_active_login() {
        let Some((paths, fixture)) = fixture_with_dev_runtime() else {
            return;
        };
        let inject = fixture.0.join("inject-status.mjs");
        fs::write(
            &inject,
            r#"import { pathToFileURL } from "node:url";
export default {
  async createRuntime(pi) {
    const sdk = pathToFileURL(process.env.PI_AUTH_PI_SDK).href;
    return {
      async getProviders() {
        return [{ id: "fixture-oauth", name: "Fixture", auth: { oauth: { name: "Fixture (Pro)", isSubscription: true } } }];
      },
      async listCredentials() {
        return [];
      },
      async login(providerId, type, interaction) {
        interaction.notify({ type: "auth_url", url: "https://fixture.example/authorize" });
        await interaction.prompt({ type: "manual_code", message: "Paste the code" });
        return { type: "oauth" };
      },
    };
  },
};"#,
        )
        .unwrap();
        let (emit, events) = collector();
        let bridge =
            spawn_bridge(None, &paths, Some(&inject), Some(emit)).expect("bridge should spawn");
        let ack = bridge
            .request(
                json!({ "op": "login", "provider": "fixture-oauth" }),
                REQUEST_TIMEOUT,
            )
            .expect("login should be acknowledged");
        let login_id = ack["login"].as_str().unwrap().to_owned();
        wait_event(&events, "prompt").expect("prompt should arrive");

        let status = bridge
            .request(json!({ "op": "status" }), REQUEST_TIMEOUT)
            .expect("status should succeed");
        assert_eq!(status["activeLogin"], json!(login_id));
        assert_eq!(status["activeLoginProvider"], json!("fixture-oauth"));

        // 原始 prompt 已被上面的 wait_event 消费；这里应能等到重放的那条
        //（status 应答之后桥接会重发仍待输入的 prompt）。
        let deadline = std::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                panic!("timed out waiting for the replayed prompt event");
            }
            match events.recv_timeout(remaining.min(Duration::from_secs(1))) {
                Ok(value) => {
                    if value.get("event").and_then(Value::as_str) == Some("prompt")
                        && value.get("login").and_then(Value::as_str) == Some(login_id.as_str())
                    {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("pi auth bridge event channel closed");
                }
            }
        }
        bridge
            .request(
                json!({ "op": "cancel", "login": login_id }),
                CONTROL_TIMEOUT,
            )
            .expect("cleanup cancel should succeed");
    }

    /// 界面预选登录方式：选择提示由桥接直接回答，不再转发给界面。
    #[test]
    #[ignore = "requires the managed Pi SDK; exercised with --include-ignored"]
    fn preferred_login_method_answers_the_select_prompt() {
        let Some((paths, fixture)) = fixture_with_dev_runtime() else {
            return;
        };
        let inject = fixture.0.join("inject-method.mjs");
        fs::write(
            &inject,
            r#"import { pathToFileURL } from "node:url";
export default {
  async createRuntime(pi) {
    const sdk = pathToFileURL(process.env.PI_AUTH_PI_SDK).href;
    const storage = await import(new URL("core/auth-storage.js", sdk).href);
    const authPath = process.env.PI_AUTH_AUTH_JSON;
    return {
      async getProviders() {
        return [{ id: "fixture-oauth", name: "Fixture", auth: { oauth: { name: "Fixture (Pro)", isSubscription: true } } }];
      },
      async listCredentials() {
        return [];
      },
      async login(providerId, type, interaction) {
        const choice = await interaction.prompt({
          type: "select",
          message: "Select login method:",
          options: [
            { id: "browser", label: "Browser login" },
            { id: "device_code", label: "Device code login" },
          ],
        });
        interaction.notify({ type: "info", message: `chosen:${choice}` });
        const credential = { type: "oauth", access: "fixture-access", refresh: "fixture-refresh", expires: Date.now() + 3600_000 };
        await storage.AuthStorage.create(authPath).modify(providerId, async () => credential);
        return credential;
      },
    };
  },
};"#,
        )
        .unwrap();
        let (emit, events) = collector();
        let bridge =
            spawn_bridge(None, &paths, Some(&inject), Some(emit)).expect("bridge should spawn");
        let ack = bridge
            .request(
                json!({ "op": "login", "provider": "fixture-oauth", "method": "device_code" }),
                REQUEST_TIMEOUT,
            )
            .expect("login should be acknowledged");
        assert!(ack["login"].is_string());

        let mut chosen = false;
        let mut saw_prompt = false;
        let deadline = std::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                panic!("timed out waiting for the login result");
            }
            match events.recv_timeout(remaining.min(Duration::from_secs(1))) {
                Ok(value) => {
                    let event = value.get("event").and_then(Value::as_str);
                    if event == Some("prompt") {
                        saw_prompt = true;
                    }
                    if event == Some("notify")
                        && value.get("message").and_then(Value::as_str)
                            == Some("chosen:device_code")
                    {
                        chosen = true;
                    }
                    if event == Some("done") {
                        assert_eq!(value["ok"], json!(true));
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("pi auth bridge event channel closed");
                }
            }
        }
        assert!(chosen, "fixture 应收到预选的登录方式");
        assert!(!saw_prompt, "预选命中时选择提示不得转发给界面");
    }

    /// 模型目录 op：返回 pi 运行时自带的供应商模型（官方登录后免 API Key）。
    #[test]
    #[ignore = "requires the managed Pi SDK; exercised with --include-ignored"]
    fn lists_provider_models_from_pi_runtime() {
        let Some((paths, _fixture)) = fixture_with_dev_runtime() else {
            return;
        };
        let (emit, _events) = collector();
        let bridge = spawn_bridge(None, &paths, None, Some(emit)).expect("bridge should spawn");
        let reply = bridge
            .request(
                json!({ "op": "models", "provider": "openai-codex" }),
                REQUEST_TIMEOUT,
            )
            .expect("models request should succeed");
        let models: Vec<PiOfficialModel> = serde_json::from_value(reply["models"].clone()).unwrap();
        assert!(!models.is_empty(), "pi 运行时应提供 codex 模型");
        assert!(models.iter().all(|model| !model.id.is_empty()));
    }
}
