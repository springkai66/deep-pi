use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{Read, Seek, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread,
    time::Duration,
};

use crate::diagnostics::{DiagnosticCode, Diagnostics};
use crate::rpc_history::{history_response_id, HistorySnapshot, MAX_HISTORY_BYTES};
use serde::Serialize;
use serde_json::{json, Value};

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_COMMAND_BYTES: usize = 1024 * 1024;
const MAX_PENDING: usize = 32;
struct Reply {
    sender: mpsc::SyncSender<Result<RpcReply, String>>,
    history: bool,
}
type Subscriber = Box<dyn Fn(&RpcEvent) -> Result<(), String> + Send + Sync>;

struct JsonLines {
    pending: Vec<u8>,
    limit: usize,
    spill: Option<File>,
    bytes: usize,
    maximum: usize,
    discard: bool,
}

enum RpcFrame {
    Json(Value),
    History(File),
}

impl JsonLines {
    fn new(limit: usize) -> Self {
        Self {
            pending: Vec::new(),
            limit,
            spill: None,
            bytes: 0,
            maximum: limit,
            discard: false,
        }
    }

    #[cfg(test)]
    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<RpcFrame>, String> {
        self.feed_retained(bytes, |_| true)
    }

    fn feed_retained(
        &mut self,
        bytes: &[u8],
        retain: impl Fn(&[u8]) -> bool,
    ) -> Result<Vec<RpcFrame>, String> {
        let mut frames = Vec::new();
        for part in bytes.split_inclusive(|byte| *byte == b'\n') {
            let complete = part.last() == Some(&b'\n');
            let content = if complete {
                &part[..part.len() - 1]
            } else {
                part
            };
            if self.bytes + content.len() > self.maximum {
                return Err("RPC frame exceeded the size limit".into());
            }
            if !self.discard && self.spill.is_none() && self.bytes + content.len() > self.limit {
                if retain(&self.pending) {
                    let mut file = tempfile::tempfile().map_err(|_| "Cannot spool RPC history")?;
                    file.write_all(&self.pending)
                        .map_err(|_| "Cannot spool RPC history")?;
                    self.spill = Some(file);
                } else {
                    self.discard = true;
                }
                self.pending = Vec::new();
            }
            if let Some(file) = &mut self.spill {
                file.write_all(content)
                    .map_err(|_| "Cannot spool RPC history")?;
            } else if !self.discard {
                self.pending.extend_from_slice(content);
            }
            self.bytes += content.len();
            if complete {
                if let Some(mut file) = self.spill.take() {
                    file.rewind()
                        .map_err(|_| "Cannot read spooled RPC history")?;
                    frames.push(RpcFrame::History(file));
                } else if !self.pending.iter().all(u8::is_ascii_whitespace) {
                    let value: Value = serde_json::from_slice(&self.pending)
                        .map_err(|_| "Invalid RPC JSON frame")?;
                    if !value.is_object() {
                        return Err("RPC frame must be an object".into());
                    }
                    frames.push(RpcFrame::Json(value));
                }
                self.pending.clear();
                self.bytes = 0;
                self.discard = false;
            }
        }
        Ok(frames)
    }

    fn finish(&self) -> Result<(), String> {
        if self.bytes == 0 {
            Ok(())
        } else {
            Err("RPC exited with an incomplete frame".into())
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcEvent {
    pub sequence: u64,
    pub payload: Value,
}

pub struct RpcReply {
    pub data: Value,
    pub sequence: u64,
    pub history: Option<HistorySnapshot>,
}

struct EventJournal {
    events: VecDeque<(RpcEvent, usize)>,
    sequence: u64,
    dropped: u64,
    bytes: usize,
    max_events: usize,
    max_bytes: usize,
    subscriber: Option<Subscriber>,
}

impl EventJournal {
    fn new(max_events: usize, max_bytes: usize) -> Self {
        Self {
            events: VecDeque::new(),
            sequence: 0,
            dropped: 0,
            bytes: 0,
            max_events,
            max_bytes,
            subscriber: None,
        }
    }

    fn push(&mut self, payload: Value) -> Result<(), String> {
        let size = serde_json::to_vec(&payload)
            .map_err(|error| error.to_string())?
            .len();
        self.sequence += 1;
        let event = RpcEvent {
            sequence: self.sequence,
            payload,
        };
        if let Some(subscriber) = &self.subscriber {
            if subscriber(&event).is_err() {
                self.subscriber = None;
            }
        }
        self.events.push_back((event, size));
        self.bytes += size;
        while self.events.len() > self.max_events || self.bytes > self.max_bytes {
            if let Some((_, bytes)) = self.events.pop_front() {
                self.bytes -= bytes;
                self.dropped += 1;
            }
        }
        Ok(())
    }

    fn forget_ui_response(&mut self, id: &str) {
        self.events.retain(|(event, _)| {
            !(event.payload["type"] == "extension_ui_request" && event.payload["id"] == id)
        });
        self.bytes = self.events.iter().map(|(_, bytes)| bytes).sum();
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcExit {
    pub code: Option<i32>,
    pub stopped: bool,
}

struct Shared {
    pending: Mutex<HashMap<String, Reply>>,
    events: Mutex<EventJournal>,
    stop: AtomicBool,
    requested_stop: AtomicBool,
    exit: Mutex<Option<RpcExit>>,
    diagnostics: Arc<Diagnostics>,
    diagnostic_run: u64,
    exited: Condvar,
}

impl Shared {
    fn publish(&self, event: Value) {
        if let Ok(mut journal) = self.events.lock() {
            let _ = journal.push(event);
        }
    }

    fn receive(&self, mut value: Value) {
        if value["type"] == "response" {
            if let Some(id) = value["id"].as_str() {
                let reply = self
                    .pending
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.remove(id));
                if let Some(reply) = reply {
                    let result = if value["success"] == true {
                        self.events
                            .lock()
                            .map_err(|_| "RPC event lock is poisoned".to_owned())
                            .map(|journal| journal.sequence)
                            .and_then(|sequence| {
                                if reply.history {
                                    let messages = value["data"]["messages"].take();
                                    let Value::Array(messages) = messages else {
                                        return Err("Invalid history messages".into());
                                    };
                                    let history = HistorySnapshot::from_messages(messages)?;
                                    Ok(RpcReply {
                                        data: Value::Null,
                                        sequence,
                                        history: Some(history),
                                    })
                                } else {
                                    Ok(RpcReply {
                                        data: value
                                            .get_mut("data")
                                            .map(Value::take)
                                            .unwrap_or(Value::Null),
                                        sequence,
                                        history: None,
                                    })
                                }
                            })
                    } else {
                        self.diagnostics.record(
                            self.diagnostic_run,
                            DiagnosticCode::ResponseFailed,
                            1,
                            None,
                        );
                        Err(value["error"]
                            .as_str()
                            .unwrap_or("RPC command failed")
                            .to_owned())
                    };
                    let _ = reply.sender.send(result);
                }
            }
        } else {
            self.publish(value);
        }
    }

    fn retain_history(&self, prefix: &[u8]) -> bool {
        history_response_id(prefix).is_none_or(|id| {
            self.pending
                .lock()
                .is_ok_and(|pending| pending.contains_key(&id))
        })
    }

    fn receive_history(&self, mut file: File) -> Result<(), String> {
        if let Some(id) = history_response_id(&mut file) {
            if !self
                .pending
                .lock()
                .map_err(|_| "RPC pending lock is poisoned")?
                .contains_key(&id)
            {
                return Ok(());
            }
        }
        file.rewind().map_err(|_| "Cannot read RPC history")?;
        let sequence = self
            .events
            .lock()
            .map_err(|_| "RPC event lock is poisoned")?
            .sequence;
        let (id, history) = HistorySnapshot::read_response(file)?;
        let reply = self
            .pending
            .lock()
            .map_err(|_| "RPC pending lock is poisoned")?
            .remove(&id);
        if let Some(reply) = reply {
            let result = if reply.history {
                Ok(RpcReply {
                    data: Value::Null,
                    sequence,
                    history: Some(history),
                })
            } else {
                Err("RPC response exceeded the normal frame limit".into())
            };
            let _ = reply.sender.send(result);
        }
        Ok(())
    }
}

pub struct RpcTransport {
    shared: Arc<Shared>,
    input: mpsc::SyncSender<Vec<u8>>,
    history_gate: Mutex<()>,
}

impl RpcTransport {
    #[cfg(test)]
    pub fn spawn(
        command: &mut Command,
        on_exit: impl FnOnce(RpcExit) + Send + 'static,
    ) -> Result<Self, String> {
        Self::spawn_observed(command, Arc::new(Diagnostics::default()), on_exit)
    }

    pub fn spawn_observed(
        command: &mut Command,
        diagnostics: Arc<Diagnostics>,
        on_exit: impl FnOnce(RpcExit) + Send + 'static,
    ) -> Result<Self, String> {
        let diagnostic_run = diagnostics.begin_run();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let (mut child, tree) =
            crate::process_runner::spawn_owned_detailed(command).map_err(|failure| {
                use crate::process_runner::SpawnFailureKind;
                let code = match failure.kind {
                    SpawnFailureKind::NotFound => DiagnosticCode::SpawnNotFound,
                    SpawnFailureKind::PermissionDenied => DiagnosticCode::SpawnDenied,
                    SpawnFailureKind::Ownership => DiagnosticCode::ProcessOwnershipFailed,
                    SpawnFailureKind::Resume => DiagnosticCode::ProcessResumeFailed,
                    SpawnFailureKind::Spawn | SpawnFailureKind::Unsupported => {
                        DiagnosticCode::SpawnFailed
                    }
                };
                diagnostics.record(diagnostic_run, code, 1, None);
                failure.message
            })?;
        let mut stdin = child.stdin.take().ok_or("RPC stdin unavailable")?;
        let mut stdout = child.stdout.take().ok_or("RPC stdout unavailable")?;
        let mut stderr = child.stderr.take().ok_or("RPC stderr unavailable")?;
        let shared = Arc::new(Shared {
            pending: Mutex::new(HashMap::new()),
            events: Mutex::new(EventJournal::new(1024, MAX_FRAME_BYTES)),
            stop: AtomicBool::new(false),
            requested_stop: AtomicBool::new(false),
            exit: Mutex::new(None),
            diagnostics,
            diagnostic_run,
            exited: Condvar::new(),
        });
        let (input, receiver) = mpsc::sync_channel::<Vec<u8>>(16);
        let writer_state = shared.clone();
        thread::spawn(move || {
            while !writer_state.stop.load(Ordering::Acquire) {
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(bytes) => {
                        if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                            writer_state.diagnostics.record(
                                writer_state.diagnostic_run,
                                DiagnosticCode::InputWriteFailed,
                                1,
                                None,
                            );
                            writer_state.stop.store(true, Ordering::Release);
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        let reader_state = shared.clone();
        let (reader_done, drained) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut decoder = JsonLines::new(MAX_FRAME_BYTES);
            decoder.maximum = MAX_HISTORY_BYTES;
            let mut buffer = [0; 8192];
            let result = (|| -> Result<(), String> {
                loop {
                    let count = stdout
                        .read(&mut buffer)
                        .map_err(|_| "Cannot read RPC output")?;
                    if count == 0 {
                        return decoder.finish();
                    }
                    for frame in decoder.feed_retained(&buffer[..count], |prefix| {
                        reader_state.retain_history(prefix)
                    })? {
                        match frame {
                            RpcFrame::Json(value) => reader_state.receive(value),
                            RpcFrame::History(file) => reader_state.receive_history(file)?,
                        }
                    }
                }
            })();
            if let Err(error) = result {
                if !reader_state.requested_stop.load(Ordering::Acquire) {
                    let code = match error.as_str() {
                        "Cannot read RPC output" => DiagnosticCode::OutputReadFailed,
                        "RPC frame exceeded the size limit" => DiagnosticCode::FrameTooLarge,
                        "Invalid RPC JSON frame" | "RPC frame must be an object" => {
                            DiagnosticCode::InvalidJson
                        }
                        "RPC exited with an incomplete frame" => DiagnosticCode::IncompleteFrame,
                        "Cannot spool RPC history"
                        | "Cannot read spooled RPC history"
                        | "Cannot read RPC history"
                        | "历史响应格式无效或单条消息超过上限"
                        | "历史响应存在多余内容"
                        | "无法创建临时历史快照" => DiagnosticCode::HistoryFailed,
                        _ => DiagnosticCode::OutputInvalid,
                    };
                    reader_state
                        .diagnostics
                        .record(reader_state.diagnostic_run, code, 1, None);
                    reader_state.publish(json!({"type":"rpc_error","error":error}));
                }
            }
            reader_state.stop.store(true, Ordering::Release);
            let _ = reader_done.send(());
        });
        let diagnostic_state = shared.clone();
        // Drain stderr without retaining raw text, which may contain prompts or credentials.
        let (diagnostic_done, diagnostics_drained) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut buffer = [0; 8192];
            loop {
                match stderr.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        diagnostic_state.diagnostics.record(
                            diagnostic_state.diagnostic_run,
                            DiagnosticCode::StderrObserved,
                            count as u64,
                            None,
                        );
                    }
                    Err(_) => {
                        diagnostic_state.diagnostics.record(
                            diagnostic_state.diagnostic_run,
                            DiagnosticCode::StderrReadFailed,
                            1,
                            None,
                        );
                        break;
                    }
                }
            }
            let _ = diagnostic_done.send(());
        });
        let exit_state = shared.clone();
        thread::spawn(move || {
            let mut status = None;
            loop {
                match child.try_wait() {
                    Ok(Some(result)) => {
                        status = Some(result);
                        break;
                    }
                    Err(_) => break,
                    Ok(None) if exit_state.stop.load(Ordering::Acquire) => break,
                    Ok(None) => thread::sleep(Duration::from_millis(20)),
                }
            }
            exit_state.stop.store(true, Ordering::Release);
            let stopped = exit_state.requested_stop.load(Ordering::Acquire);
            drop(tree);
            if status.is_none() {
                let _ = child.kill();
                status = child.wait().ok();
            }
            let _ = drained.recv_timeout(Duration::from_secs(2));
            let _ = diagnostics_drained.recv_timeout(Duration::from_secs(1));
            let exit = RpcExit {
                code: status.and_then(|status| status.code()),
                stopped,
            };
            exit_state.diagnostics.record(
                exit_state.diagnostic_run,
                DiagnosticCode::ProcessExit,
                1,
                exit.code,
            );
            if let Ok(mut pending) = exit_state.pending.lock() {
                for (_, reply) in pending.drain() {
                    let _ = reply.sender.send(Err("RPC process exited".into()));
                }
            }
            exit_state.publish(json!({"type":"rpc_exit","code":exit.code,"stopped":exit.stopped}));
            on_exit(exit.clone());
            if let Ok(mut state) = exit_state.exit.lock() {
                *state = Some(exit);
                exit_state.exited.notify_all();
            }
        });
        Ok(Self {
            shared,
            input,
            history_gate: Mutex::new(()),
        })
    }

    pub fn request(&self, command: Value, timeout: Duration) -> Result<Value, String> {
        self.request_with_cursor(command, timeout)
            .map(|reply| reply.data)
    }

    pub fn request_with_cursor(
        &self,
        command: Value,
        timeout: Duration,
    ) -> Result<RpcReply, String> {
        self.request_mode(command, timeout, false)
    }

    pub fn open_history(&self, timeout: Duration) -> Result<(HistorySnapshot, u64), String> {
        let _permit = self
            .history_gate
            .try_lock()
            .map_err(|_| "历史快照正在读取，请稍后重试")?;
        let reply = self.request_mode(json!({"type":"get_messages"}), timeout, true)?;
        Ok((reply.history.ok_or("历史快照响应缺失")?, reply.sequence))
    }

    fn request_mode(
        &self,
        mut command: Value,
        timeout: Duration,
        history: bool,
    ) -> Result<RpcReply, String> {
        let id = uuid::Uuid::new_v4().to_string();
        command
            .as_object_mut()
            .ok_or("RPC command must be an object")?
            .insert("id".into(), Value::String(id.clone()));
        let (sender, receiver) = mpsc::sync_channel(1);
        {
            let mut pending = self
                .shared
                .pending
                .lock()
                .map_err(|_| "RPC pending lock is poisoned")?;
            if self.shared.stop.load(Ordering::Acquire) {
                return Err("RPC process is closed".into());
            }
            if pending.len() >= MAX_PENDING {
                return Err("Too many pending RPC requests".into());
            }
            pending.insert(id.clone(), Reply { sender, history });
        }
        let result = self.notify(command).and_then(|_| {
            receiver.recv_timeout(timeout).map_err(|_| {
                self.shared.diagnostics.record(
                    self.shared.diagnostic_run,
                    DiagnosticCode::RequestTimeout,
                    1,
                    None,
                );
                "RPC request timed out".to_owned()
            })?
        });
        if let Ok(mut pending) = self.shared.pending.lock() {
            pending.remove(&id);
        }
        result
    }

    pub fn notify(&self, command: Value) -> Result<(), String> {
        if self.shared.stop.load(Ordering::Acquire) {
            return Err("RPC process is closed".into());
        }
        let mut bytes = serde_json::to_vec(&command).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_COMMAND_BYTES {
            return Err("RPC command exceeded the size limit".into());
        }
        bytes.push(b'\n');
        self.input.try_send(bytes).map_err(|_| {
            self.shared.diagnostics.record(
                self.shared.diagnostic_run,
                DiagnosticCode::InputQueueFull,
                1,
                None,
            );
            "RPC input queue is full or closed".to_owned()
        })?;
        if command["type"] == "extension_ui_response" {
            if let Some(id) = command["id"].as_str() {
                self.shared
                    .events
                    .lock()
                    .map_err(|_| "RPC event lock is poisoned")?
                    .forget_ui_response(id);
            }
        }
        Ok(())
    }

    pub fn subscribe(&self, subscriber: Subscriber) -> Result<u64, String> {
        let mut journal = self
            .shared
            .events
            .lock()
            .map_err(|_| "RPC event lock is poisoned")?;
        for (event, _) in &journal.events {
            subscriber(event)?;
        }
        journal.subscriber = Some(subscriber);
        if journal.dropped > 0 {
            self.shared.diagnostics.record(
                self.shared.diagnostic_run,
                DiagnosticCode::ReplayGap,
                journal.dropped,
                None,
            );
        }
        Ok(journal.dropped)
    }

    pub fn shutdown(&self, timeout: Duration) -> Result<(), String> {
        self.shared.diagnostics.record(
            self.shared.diagnostic_run,
            DiagnosticCode::StopRequested,
            1,
            None,
        );
        self.shared.requested_stop.store(true, Ordering::Release);
        self.shared.stop.store(true, Ordering::Release);
        let exit = self
            .shared
            .exit
            .lock()
            .map_err(|_| "RPC exit lock is poisoned")?;
        let (exit, _) = self
            .shared
            .exited
            .wait_timeout_while(exit, timeout, |exit| exit.is_none())
            .map_err(|_| "RPC exit lock is poisoned")?;
        if exit.is_some() {
            Ok(())
        } else {
            self.shared.diagnostics.record(
                self.shared.diagnostic_run,
                DiagnosticCode::StopTimeout,
                1,
                None,
            );
            Err("RPC process did not stop in time".into())
        }
    }
}

impl Drop for RpcTransport {
    fn drop(&mut self) {
        self.shared.requested_stop.store(true, Ordering::Release);
        self.shared.stop.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn spooled_frames_keep_exact_utf8_and_still_enforce_the_total_byte_limit() {
        let source = "{\"type\":\"response\",\"content\":\"中文\u{2028}继续\"}\r\n";
        let mut decoder = JsonLines::new(10);
        decoder.maximum = 100;
        let mut frames = Vec::new();
        for byte in source.as_bytes() {
            frames.extend(decoder.feed(&[*byte]).unwrap());
        }
        assert_eq!(frames.len(), 1);
        let RpcFrame::History(mut file) = frames.remove(0) else {
            panic!("expected a spooled frame")
        };
        let mut actual = String::new();
        file.read_to_string(&mut actual).unwrap();
        assert_eq!(actual, source.strip_suffix('\n').unwrap());
        assert!(decoder.finish().is_ok());
        assert!(decoder.feed(&[b'x'; 101]).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn large_history_is_paged_and_following_rpc_events_still_arrive() {
        let mut command = Command::new("node.exe");
        command.args(["-e", r#"
const readline = require('node:readline');
readline.createInterface({input:process.stdin}).on('line', line => {
  const request = JSON.parse(line);
  if (request.type === 'get_messages') {
    console.log(JSON.stringify({type:'agent_start'}));
    const messages = Array.from({length:150}, (_,i) => ({role:'user',timestamp:i,content:'x'.repeat(70000)}));
    console.log(JSON.stringify({id:request.id,type:'response',command:request.type,success:true,data:{messages}}));
    console.log(JSON.stringify({type:'agent_settled'}));
  } else console.log(JSON.stringify({id:request.id,type:'response',command:request.type,success:true,data:{continued:true}}));
});"#]);
        let transport = RpcTransport::spawn(&mut command, |_| {}).unwrap();
        let (mut snapshot, sequence) = transport.open_history(Duration::from_secs(20)).unwrap();
        let page = snapshot.page(None, sequence).unwrap();
        assert_eq!(page.total, 150);
        assert_eq!(page.event_sequence, 1);
        assert!(page.messages.len() < 100);
        let older = snapshot.page(Some(page.start), sequence).unwrap();
        assert_eq!(older.end, page.start);
        assert_eq!(
            transport
                .request(json!({"type":"get_state"}), Duration::from_secs(5))
                .unwrap()["continued"],
            true
        );
        transport.shutdown(Duration::from_secs(5)).unwrap();
        assert!(snapshot.page(Some(1), sequence).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn expired_large_history_is_drained_without_parsing_its_body() {
        let mut command = Command::new("node.exe");
        command.args(["-e", r#"
let old;
require('node:readline').createInterface({input:process.stdin}).on('line', line => {
  const request = JSON.parse(line);
  if (request.type === 'get_messages') { old = request.id; return; }
  console.log(JSON.stringify({id:old,type:'response',command:'get_messages',success:true,data:{messages:'x'.repeat(9*1024*1024)}}));
  console.log(JSON.stringify({id:request.id,type:'response',command:request.type,success:true,data:{continued:true}}));
});"#]);
        let transport = RpcTransport::spawn(&mut command, |_| {}).unwrap();
        assert!(transport.open_history(Duration::from_millis(10)).is_err());
        let state = transport
            .request(json!({"type":"get_state"}), Duration::from_secs(20))
            .unwrap();
        assert_eq!(state["continued"], true);
        transport.shutdown(Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn frames_only_on_lf_and_handles_split_utf8() {
        let bytes = "{\"type\":\"message\",\"text\":\"模型\u{2028}继续\u{2029}\"}\r\n".as_bytes();
        let mut decoder = JsonLines::new(1024);
        let mut output = Vec::new();
        for byte in bytes {
            output.extend(
                decoder
                    .feed(&[*byte])
                    .unwrap()
                    .into_iter()
                    .map(|frame| match frame {
                        RpcFrame::Json(value) => value,
                        RpcFrame::History(_) => panic!("small frame unexpectedly spooled"),
                    }),
            );
        }
        assert_eq!(
            output,
            vec![json!({"type":"message","text":"模型\u{2028}继续\u{2029}"})]
        );
        assert!(decoder.finish().is_ok());
    }

    #[test]
    fn rejects_unbounded_invalid_and_incomplete_frames() {
        assert!(JsonLines::new(5).feed(b"123456").is_err());
        assert!(JsonLines::new(100).feed(b"not-json\n").is_err());
        let mut decoder = JsonLines::new(100);
        decoder.feed(b"{\"type\":").unwrap();
        assert!(decoder.finish().is_err());
        assert!(JsonLines::new(100).feed(b"\r\n\n").unwrap().is_empty());
    }

    #[test]
    fn bounded_event_history_reports_gaps_and_replays_in_order() {
        let mut journal = EventJournal::new(2, 1024);
        journal.push(json!({"type":"a"})).unwrap();
        journal.push(json!({"type":"b"})).unwrap();
        journal.push(json!({"type":"c"})).unwrap();
        assert_eq!(journal.dropped, 1);
        assert_eq!(
            journal
                .events
                .iter()
                .map(|event| event.0.sequence)
                .collect::<Vec<_>>(),
            [2, 3]
        );
    }

    #[test]
    fn answered_extension_dialogs_are_not_replayed_on_reconnect() {
        let mut journal = EventJournal::new(10, 1024);
        journal
            .push(json!({"type":"extension_ui_request","id":"dialog-1","method":"confirm"}))
            .unwrap();
        journal.push(json!({"type":"agent_start"})).unwrap();
        journal.forget_ui_response("dialog-1");
        assert_eq!(journal.events.len(), 1);
        assert_eq!(journal.events[0].0.payload["type"], "agent_start");
        assert_eq!(journal.sequence, 2);
        assert_eq!(journal.bytes, journal.events[0].1);
    }

    #[cfg(windows)]
    #[test]
    fn correlates_requests_and_stops_a_real_process() {
        let mut command = std::process::Command::new("node.exe");
        command.args(["-e", r#"
          let input = '';
          process.stdin.on('data', chunk => {
            input += chunk;
            for (;;) {
              const end = input.indexOf('\n');
              if (end < 0) break;
              const msg = JSON.parse(input.slice(0, end)); input = input.slice(end + 1);
              process.stdout.write(JSON.stringify({type:'event', value:msg.value}) + '\n');
              process.stdout.write(JSON.stringify({type:'response', id:msg.id, success:true, data:{value:msg.value}}) + '\n');
            }
          });
        "#]);
        let transport = RpcTransport::spawn(&mut command, |_| {}).unwrap();
        assert_eq!(
            transport
                .request(
                    json!({"type":"ping","value":"真实进程"}),
                    Duration::from_secs(5)
                )
                .unwrap(),
            json!({"value":"真实进程"})
        );
        let seen = Arc::new(Mutex::new(Vec::new()));
        let output = seen.clone();
        transport
            .subscribe(Box::new(move |event| {
                output.lock().unwrap().push(event.payload.clone());
                Ok(())
            }))
            .unwrap();
        assert_eq!(seen.lock().unwrap()[0]["type"], "event");
        transport.shutdown(Duration::from_secs(5)).unwrap();
        assert!(transport
            .request(json!({"type":"ping"}), Duration::from_millis(100))
            .is_err());
    }

    #[cfg(windows)]
    #[test]
    fn times_out_and_clears_pending_requests() {
        let mut command = std::process::Command::new("node.exe");
        command.args(["-e", "process.stdin.resume();"]);
        let transport = RpcTransport::spawn(&mut command, |_| {}).unwrap();
        assert!(transport
            .request(json!({"type":"ping"}), Duration::from_millis(80))
            .is_err());
        assert!(transport.shared.pending.lock().unwrap().is_empty());
        transport.shutdown(Duration::from_secs(5)).unwrap();
    }
}
