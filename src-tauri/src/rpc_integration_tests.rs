#![cfg(windows)]

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::rpc_transport::RpcTransport;
use serde_json::{json, Value};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct MockModel {
    port: u16,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Result<usize, String>>>,
}

impl MockModel {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let token = stop.clone();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut requests = 0;
            while !token.load(Ordering::Acquire) && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => return Err(error.to_string()),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 8192];
                let header_end = loop {
                    let count = stream
                        .read(&mut buffer)
                        .map_err(|error| error.to_string())?;
                    if count == 0 || request.len() > 1024 * 1024 {
                        return Err("Invalid fixture HTTP request".into());
                    }
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(offset) =
                        request.windows(4).position(|window| window == b"\r\n\r\n")
                    {
                        break offset + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let length: usize = headers
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                    .ok_or("Missing content-length")?
                    .1
                    .trim()
                    .parse()
                    .map_err(|_| "Invalid content-length")?;
                if length > 1024 * 1024 {
                    return Err("Fixture body too large".into());
                }
                while request.len() < header_end + length {
                    let count = stream
                        .read(&mut buffer)
                        .map_err(|error| error.to_string())?;
                    if count == 0 {
                        return Err("Incomplete fixture request".into());
                    }
                    request.extend_from_slice(&buffer[..count]);
                }
                let body: Value = serde_json::from_slice(&request[header_end..header_end + length])
                    .map_err(|error| error.to_string())?;
                if requests > 0
                    && !body["messages"].as_array().is_some_and(|messages| {
                        messages.iter().any(|message| message["role"] == "tool")
                    })
                {
                    return Err("Pi did not include the real tool result".into());
                }
                let (delta, reason) = if requests == 0 {
                    (
                        json!({"role":"assistant","tool_calls":[{"index":0,"id":"fixture-read","type":"function",
                        "function":{"name":"read","arguments":"{\"path\":\"fixture.txt\"}"}}]}),
                        "tool_calls",
                    )
                } else {
                    (json!({"role":"assistant","content":"RPC_TOOL_OK"}), "stop")
                };
                let chunk = |delta: Value, finish: Value| {
                    json!({
                    "id":"fixture-completion", "object":"chat.completion.chunk", "created":0, "model":"fixture",
                    "choices":[{"index":0,"delta":delta,"finish_reason":finish}],
                }).to_string()
                };
                let response = format!(
                    "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
                    chunk(delta, Value::Null),
                    chunk(json!({}), json!(reason))
                );
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response)
                    .map_err(|error| error.to_string())?;
                requests += 1;
            }
            Ok(requests)
        });
        Self {
            port,
            stop,
            worker: Some(worker),
        }
    }

    fn finish(&mut self) -> Result<usize, String> {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| "Mock model thread panicked".to_owned())?
    }
}

impl Drop for MockModel {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
#[ignore = "requires the Pi SDK; exercised with --include-ignored"]
fn real_pi_pages_a_large_native_session_without_replaying_prompts() {
    let cli = std::env::var_os("PI_RPC_TEST_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA is required"))
                .join("npm/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js")
        });
    let directory = cli.parent().unwrap();
    let sdk = if directory.file_name().is_some_and(|name| name == "bundle") {
        directory.parent().unwrap().join("index.js")
    } else {
        directory.join("index.js")
    };
    assert!(
        sdk.is_file(),
        "The Pi history integration test requires the matching SDK index.js"
    );
    let fixture =
        Fixture(std::env::temp_dir().join(format!("deeppi-rpc-history-{}", uuid::Uuid::new_v4())));
    let project = fixture.0.join("project");
    let home = fixture.0.join("home");
    let sessions = fixture.0.join("sessions");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let mut seed = Command::new("node.exe");
    seed.args(["--input-type=module", "-e", r#"
import { pathToFileURL } from 'node:url';
const { SessionManager } = await import(pathToFileURL(process.argv[1]).href);
const session = SessionManager.create(process.argv[2], process.argv[3]);
session.appendMessage({role:'assistant',content:[{type:'text',text:'seed'}],api:'openai-completions',
provider:'local-fixture',model:'fixture',timestamp:1,stopReason:'stop',
usage:{input:0,output:0,cacheRead:0,cacheWrite:0,totalTokens:0,cost:{input:0,output:0,cacheRead:0,cacheWrite:0,total:0}}});
for (let i=0;i<150;i++) session.appendMessage({role:'user',timestamp:i+2,content:'x'.repeat(70000)});
process.stdout.write(JSON.stringify({id:session.getSessionId(),file:session.getSessionFile()}));
"#]).arg(&sdk).arg(&project).arg(&sessions).current_dir(&project)
        .env("PI_CODING_AGENT_DIR", &home).env("PI_TELEMETRY", "0");
    let output = crate::process_runner::run(&mut seed, Duration::from_secs(30)).unwrap();
    assert!(output.status.success(), "{}", output.text());
    let seeded: Value = serde_json::from_slice(&output.stdout).unwrap();
    let path = PathBuf::from(seeded["file"].as_str().unwrap());
    assert!(path.starts_with(&sessions));
    assert!(fs::metadata(&path).unwrap().len() > 8 * 1024 * 1024);
    let store = crate::task::TaskStore::in_memory().unwrap();
    store
        .import_native_sessions(
            &[crate::native_pi::NativeSession {
                id: seeded["id"].as_str().unwrap().into(),
                title: "Imported history".into(),
                cwd: project.to_string_lossy().into_owned(),
                file: path.to_string_lossy().into_owned(),
                timestamp: 1,
            }],
            &home,
        )
        .unwrap();
    let task = store.list().unwrap().remove(0);
    let arguments = crate::native_pi::session_arguments(&task).unwrap();
    fs::write(home.join("models.json"), serde_json::to_vec(&json!({
        "providers":{"local-fixture":{"baseUrl":"http://127.0.0.1:9/v1","api":"openai-completions","apiKey":"fixture-only",
        "models":[{"id":"fixture","name":"Fixture","reasoning":false,"input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000000,"maxTokens":1000}]}}
    })).unwrap()).unwrap();
    let mut command = Command::new("node.exe");
    command
        .arg(cli)
        .args([
            "--mode",
            "rpc",
            "--offline",
            "--no-extensions",
            "--no-skills",
            "--no-prompt-templates",
            "--no-context-files",
            "--no-themes",
        ])
        .args(arguments)
        .args(["--provider", "local-fixture", "--model", "fixture"])
        .current_dir(&project)
        .env("PI_CODING_AGENT_DIR", &home)
        .env("PI_TELEMETRY", "0");
    let transport = RpcTransport::spawn(&mut command, |_| {}).unwrap();
    let state = transport
        .request(json!({"type":"get_state"}), Duration::from_secs(30))
        .unwrap();
    assert_eq!(state["sessionId"], seeded["id"]);
    let original = fs::read(&path).unwrap();
    let (mut snapshot, sequence) = transport.open_history(Duration::from_secs(30)).unwrap();
    let mut page = snapshot.page(None, sequence).unwrap();
    assert_eq!(page.total, 151);
    loop {
        for (index, message) in page.messages.iter().enumerate() {
            assert_eq!(message["timestamp"], page.start + index + 1);
        }
        if page.start == 0 {
            break;
        }
        page = snapshot.page(Some(page.start), sequence).unwrap();
    }
    assert_eq!(
        transport
            .request(json!({"type":"get_state"}), Duration::from_secs(5))
            .unwrap()["isStreaming"],
        false
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    transport.shutdown(Duration::from_secs(5)).unwrap();
}

#[test]
#[ignore = "requires an installed Pi CLI; exercised with --include-ignored"]
fn real_pi_executes_a_tool_and_restores_the_same_session() {
    let cli = std::env::var_os("PI_RPC_TEST_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA is required"))
                .join("npm/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js")
        });
    assert!(cli.is_file(), "Set PI_RPC_TEST_CLI to an installed Pi CLI");
    let fixture =
        Fixture(std::env::temp_dir().join(format!("deeppi-rpc-{}", uuid::Uuid::new_v4())));
    let project = fixture.0.join("project");
    let home = fixture.0.join("pi");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(project.join("fixture.txt"), "REAL_LOCAL_TOOL_RESULT").unwrap();
    let mut model = MockModel::start();
    fs::write(home.join("models.json"), serde_json::to_vec(&json!({
        "providers":{"local-fixture":{
            "baseUrl":format!("http://127.0.0.1:{}/v1", model.port),
            "api":"openai-completions","apiKey":"local-test-only",
            "models":[{"id":"fixture","name":"Fixture","reasoning":false,"input":["text"],
                "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":32000,"maxTokens":1000}]
        }}
    })).unwrap()).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut command = Command::new("node.exe");
    command
        .arg(&cli)
        .args([
            "--mode",
            "rpc",
            "--offline",
            "--no-extensions",
            "--no-skills",
            "--no-prompt-templates",
            "--no-context-files",
            "--no-themes",
            "--session-id",
            &id,
            "--provider",
            "local-fixture",
            "--model",
            "fixture",
        ])
        .current_dir(&project)
        .env("PI_CODING_AGENT_DIR", &home)
        .env("PI_TELEMETRY", "0");
    let transport = RpcTransport::spawn(&mut command, |_| {}).unwrap();
    let (sender, events) = mpsc::sync_channel(256);
    transport
        .subscribe(Box::new(move |event| {
            sender
                .try_send(event.payload.clone())
                .map_err(|error| error.to_string())
        }))
        .unwrap();
    let state = transport
        .request(json!({"type":"get_state"}), Duration::from_secs(20))
        .unwrap();
    assert_eq!(state["sessionId"], id);
    transport
        .request(
            json!({"type":"prompt","message":"Read fixture.txt and confirm."}),
            Duration::from_secs(20),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut tool_finished = false;
    let mut final_text = false;
    let mut diagnostics = Vec::new();
    loop {
        let event = events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("Pi did not settle");
        diagnostics.push(json!({"type":event["type"],"error":event["message"]["errorMessage"],"stop":event["message"]["stopReason"]}));
        if event["type"] == "tool_execution_end" {
            assert_eq!(event["isError"], false, "{event}");
            assert!(event["result"]
                .to_string()
                .contains("REAL_LOCAL_TOOL_RESULT"));
            tool_finished = true;
        }
        if event["type"] == "message_end" && event["message"].to_string().contains("RPC_TOOL_OK") {
            final_text = true;
        }
        if event["type"] == "agent_settled" {
            break;
        }
        assert_ne!(
            event["type"], "rpc_exit",
            "Pi exited before completing the test"
        );
    }
    let served = model.finish();
    assert!(
        tool_finished && final_text,
        "Model fixture: {served:?}; Pi events: {diagnostics:?}"
    );
    let before = transport
        .request(json!({"type":"get_messages"}), Duration::from_secs(5))
        .unwrap();
    assert!(before["messages"].as_array().unwrap().len() >= 4);
    transport.shutdown(Duration::from_secs(5)).unwrap();
    let resumed = RpcTransport::spawn(&mut command, |_| {}).unwrap();
    let after = resumed
        .request(json!({"type":"get_messages"}), Duration::from_secs(20))
        .unwrap();
    assert_eq!(before, after);
    resumed.shutdown(Duration::from_secs(5)).unwrap();
    assert_eq!(served.unwrap(), 2);
}

#[test]
#[ignore = "requires explicit local subscription credentials and live network"]
fn live_codex_recovery_does_not_replay_prompts() {
    let cli = PathBuf::from(std::env::var_os("PI_RPC_LIVE_CLI").expect("live CLI path required"));
    let home = PathBuf::from(std::env::var_os("PI_RPC_LIVE_HOME").expect("live Pi home required"));
    let scratch =
        PathBuf::from(std::env::var_os("PI_RPC_LIVE_SCRATCH").expect("scratch directory required"));
    assert!(cli.is_file() && home.join("auth.json").is_file());
    assert!(scratch.starts_with(std::env::var_os("PI_SCRATCH_DIR").expect("scratch root required")));
    let project = scratch.join("project");
    let sessions = scratch.join("sessions");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut command = Command::new("node.exe");
    command
        .arg(cli)
        .args([
            "--mode",
            "rpc",
            "--no-extensions",
            "--no-skills",
            "--no-prompt-templates",
            "--no-context-files",
            "--no-themes",
            "--session-id",
            &session_id,
            "--session-dir",
        ])
        .arg(&sessions)
        .args([
            "--provider",
            "openai-codex",
            "--model",
            "gpt-6-sol",
            "--thinking",
            "high",
        ])
        .current_dir(&project)
        .env("PI_CODING_AGENT_DIR", &home)
        .env_remove("PI_CODING_AGENT_SESSION_DIR")
        .env("PI_TELEMETRY", "0");
    let proxy = std::env::var("PI_RPC_LIVE_PROXY").expect("system proxy required");
    assert!(proxy.starts_with("http://127.0.0.1:") && !proxy.contains('@'));
    command
        .env("HTTP_PROXY", &proxy)
        .env("HTTPS_PROXY", &proxy)
        .env("NODE_USE_ENV_PROXY", "1")
        .env("NO_PROXY", "localhost,127.0.0.1,::1,[::1]")
        .env_remove("ALL_PROXY");
    let transport = RpcTransport::spawn(&mut command, |_| {}).expect("Pi RPC startup failed");
    let state = transport
        .request(json!({"type":"get_state"}), Duration::from_secs(20))
        .unwrap();
    assert_eq!(state["model"]["provider"], "openai-codex");
    assert_eq!(state["model"]["id"], "gpt-6-sol");
    assert_eq!(state["thinkingLevel"], "high");
    assert_eq!(state["sessionId"], session_id);
    println!("LIVE_MODEL=openai-codex/gpt-6-sol THINKING=high");
    let (events_sender, events) = mpsc::channel::<String>();
    transport
        .subscribe(Box::new(move |event| {
            events_sender
                .send(event.payload["type"].as_str().unwrap_or("").to_owned())
                .map_err(|_| "subscriber dropped".to_owned())
        }))
        .unwrap();
    let first = "LIVE_RECOVERY_ONCE";
    transport
        .request(
            json!({"type":"prompt","message":format!("Reply with exactly {first}. No tools.")}),
            Duration::from_secs(30),
        )
        .expect("first prompt acknowledgement missing");
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let next = events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("first prompt did not settle");
        assert_ne!(next, "rpc_exit", "Pi exited during first prompt");
        if next == "agent_settled" {
            break;
        }
    }
    let messages = transport
        .request(json!({"type":"get_messages"}), Duration::from_secs(20))
        .unwrap();
    let first_user_count = messages["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "user" && message.to_string().contains(first))
        .count();
    assert_eq!(first_user_count, 1, "first prompt was replayed");
    let first_ok = messages["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| {
            message["role"] == "assistant"
                && message.to_string().contains(first)
                && message["stopReason"] == "stop"
        });
    assert!(
        first_ok,
        "first live subscription response did not finish successfully"
    );
    println!("LIVE_FIRST=completed USER_MESSAGES=1");

    // Simulate a vanished Webview channel; the process remains open while events are journaled.
    drop(events);
    let second = "LIVE_UNKNOWN_ONCE";
    let ambiguous = transport.request(
        json!({"type":"prompt","message":format!("Reply with exactly {second}. No tools.")}),
        Duration::ZERO,
    );
    assert!(
        ambiguous.is_err(),
        "zero-wait prompt should have an unknown outcome"
    );
    assert!(transport.is_open(), "the old RPC run should still be live");
    let (recovered_sender, recovered) = mpsc::channel::<String>();
    let dropped = transport
        .subscribe(Box::new(move |event| {
            recovered_sender
                .send(event.payload["type"].as_str().unwrap_or("").to_owned())
                .map_err(|_| "reconnected subscriber dropped".to_owned())
        }))
        .unwrap();
    assert_eq!(dropped, 0, "journal replay lost events");
    println!("LIVE_PROBE=open RESUBSCRIBED=true UNKNOWN_ACK=true");
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let next = recovered
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("ambiguous prompt did not settle");
        assert_ne!(next, "rpc_exit", "Pi exited during ambiguous prompt");
        if next == "agent_settled" {
            break;
        }
    }
    let before = transport
        .request(json!({"type":"get_messages"}), Duration::from_secs(20))
        .unwrap();
    let count = |data: &Value, marker: &str| {
        data["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "user" && message.to_string().contains(marker))
            .count()
    };
    assert_eq!(
        count(&before, second),
        1,
        "uncertain prompt was not accepted exactly once"
    );
    println!("LIVE_UNKNOWN=completed USER_MESSAGES=1 NO_RESEND=true");
    transport.shutdown(Duration::from_secs(5)).unwrap();
    assert!(!transport.is_open(), "stopped run must fail liveness probe");
    let resumed = RpcTransport::spawn(&mut command, |_| {}).expect("session restart failed");
    let resumed_state = resumed
        .request(json!({"type":"get_state"}), Duration::from_secs(20))
        .unwrap();
    assert_eq!(resumed_state["sessionId"], session_id);
    let after = resumed
        .request(json!({"type":"get_messages"}), Duration::from_secs(20))
        .unwrap();
    assert_eq!(count(&after, first), 1);
    assert_eq!(count(&after, second), 1);
    resumed.shutdown(Duration::from_secs(5)).unwrap();
    println!("LIVE_RESTART=same_session USER_MESSAGES=1+1");
}
