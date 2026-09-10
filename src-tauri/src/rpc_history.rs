use serde::{
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Serialize,
};
use serde_json::Value;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    fmt,
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    rc::Rc,
    sync::Mutex,
    time::{Duration, Instant},
};

pub const MAX_HISTORY_BYTES: usize = 128 * 1024 * 1024;
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const PAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_MESSAGES: usize = 100_000;

pub fn history_response_id(reader: impl Read) -> Option<String> {
    let mut found = None;
    let mut decoder = serde_json::Deserializer::from_reader(BufReader::new(reader.take(64 * 1024)));
    // The bounded probe deliberately stops before the body; full parsing happens only for live requests.
    let _ = serde::Deserializer::deserialize_map(&mut decoder, HeaderProbe(&mut found));
    found
}

struct HeaderProbe<'a>(&'a mut Option<String>);
impl<'de> Visitor<'de> for HeaderProbe<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("RPC history header")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        let mut keys = HashSet::new();
        let (mut id, mut kind, mut command, mut success) = (None, None, None, false);
        while let Some(key) = map.next_key::<String>()? {
            if keys.len() >= 64 || !keys.insert(key.clone()) {
                return Err(de::Error::custom("invalid header"));
            }
            match key.as_str() {
                "id" => id = Some(map.next_value::<String>()?),
                "type" => kind = Some(map.next_value::<String>()?),
                "command" => command = Some(map.next_value::<String>()?),
                "success" => success = map.next_value::<bool>()?,
                "data" => {
                    if kind.as_deref() == Some("response")
                        && command.as_deref() == Some("get_messages")
                        && success
                    {
                        *self.0 = id.filter(|value| !value.is_empty() && value.len() <= 128);
                    }
                    return Err(de::Error::custom("header probe complete"));
                }
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub snapshot_id: String,
    pub event_sequence: u64,
    pub messages: Vec<Value>,
    pub start: usize,
    pub end: usize,
    pub total: usize,
}

pub struct HistorySnapshot {
    id: String,
    file: File,
    entries: Vec<(u64, usize)>,
    bytes: usize,
}

impl HistorySnapshot {
    fn new() -> Result<Self, String> {
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            file: tempfile::tempfile().map_err(|_| "无法创建临时历史快照")?,
            entries: Vec::new(),
            bytes: 0,
        })
    }

    fn append(&mut self, message: Value) -> Result<(), String> {
        if !message.is_object() || !message["role"].is_string() {
            return Err("历史消息格式无效".into());
        }
        let bytes = serde_json::to_vec(&message).map_err(|_| "无法序列化历史消息")?;
        if bytes.len() > MAX_MESSAGE_BYTES
            || self.bytes + bytes.len() > MAX_HISTORY_BYTES
            || self.entries.len() >= MAX_MESSAGES
        {
            return Err("历史快照超过大小或消息数量上限".into());
        }
        self.file
            .write_all(&bytes)
            .map_err(|_| "无法写入临时历史快照")?;
        self.entries.push((self.bytes as u64, bytes.len()));
        self.bytes += bytes.len();
        Ok(())
    }

    pub fn from_messages(messages: Vec<Value>) -> Result<Self, String> {
        let mut snapshot = Self::new()?;
        for message in messages {
            snapshot.append(message)?;
        }
        Ok(snapshot)
    }

    pub fn read_response(reader: impl Read) -> Result<(String, Self), String> {
        let mut snapshot = Self::new()?;
        let budget = Rc::new(Cell::new(MAX_MESSAGE_BYTES));
        let reader = BudgetReader {
            inner: BufReader::new(reader),
            remaining: budget.clone(),
        };
        let mut decoder = serde_json::Deserializer::from_reader(reader);
        let id = EnvelopeSeed {
            snapshot: &mut snapshot,
            budget,
        }
        .deserialize(&mut decoder)
        .map_err(|_| "历史响应格式无效或单条消息超过上限")?;
        decoder.end().map_err(|_| "历史响应存在多余内容")?;
        Ok((id, snapshot))
    }

    pub fn page(&mut self, before: Option<usize>, sequence: u64) -> Result<HistoryPage, String> {
        let end = before.unwrap_or(self.entries.len());
        if end > self.entries.len() {
            return Err("历史分页位置无效".into());
        }
        let mut start = end;
        let mut bytes = 0;
        while start > 0 && end - start < 100 {
            let size = self.entries[start - 1].1;
            if start < end && bytes + size > PAGE_BYTES {
                break;
            }
            bytes += size;
            start -= 1;
        }
        let mut messages = Vec::with_capacity(end - start);
        for &(offset, length) in &self.entries[start..end] {
            self.file
                .seek(SeekFrom::Start(offset))
                .map_err(|_| "无法定位历史快照")?;
            let mut bytes = vec![0; length];
            self.file
                .read_exact(&mut bytes)
                .map_err(|_| "无法读取历史快照")?;
            messages.push(serde_json::from_slice(&bytes).map_err(|_| "历史快照内容无效")?);
        }
        Ok(HistoryPage {
            snapshot_id: self.id.clone(),
            event_sequence: sequence,
            messages,
            start,
            end,
            total: self.entries.len(),
        })
    }
}

struct StoredHistory {
    task: String,
    run: String,
    snapshot: HistorySnapshot,
    sequence: u64,
    last_access: Instant,
}

#[derive(Default)]
pub struct HistoryStore(Mutex<HashMap<String, StoredHistory>>);
impl HistoryStore {
    pub fn insert(
        &self,
        task: &str,
        run: &str,
        mut snapshot: HistorySnapshot,
        sequence: u64,
    ) -> Result<HistoryPage, String> {
        let mut entries = self.0.lock().map_err(|_| "历史快照锁不可用")?;
        entries.retain(|_, entry| entry.last_access.elapsed() < Duration::from_secs(30 * 60));
        let bytes: usize = entries.values().map(|entry| entry.snapshot.bytes).sum();
        if entries.len() >= 64 || bytes + snapshot.bytes > 512 * 1024 * 1024 {
            return Err("历史快照占用已达上限，请关闭不用的任务后重试".into());
        }
        let page = snapshot.page(None, sequence)?;
        entries.insert(
            snapshot.id.clone(),
            StoredHistory {
                task: task.into(),
                run: run.into(),
                snapshot,
                sequence,
                last_access: Instant::now(),
            },
        );
        Ok(page)
    }

    pub fn page(
        &self,
        task: &str,
        run: &str,
        id: &str,
        before: usize,
    ) -> Result<HistoryPage, String> {
        let mut entries = self.0.lock().map_err(|_| "历史快照锁不可用")?;
        entries.retain(|_, entry| entry.last_access.elapsed() < Duration::from_secs(30 * 60));
        let entry = entries
            .get_mut(id)
            .filter(|entry| entry.task == task && entry.run == run)
            .ok_or("历史快照已失效或空闲过期，请重新打开任务")?;
        entry.last_access = Instant::now();
        entry.snapshot.page(Some(before), entry.sequence)
    }

    pub fn close(&self, task: &str, run: &str, id: &str) -> Result<(), String> {
        let mut entries = self.0.lock().map_err(|_| "历史快照锁不可用")?;
        if entries
            .get(id)
            .is_some_and(|entry| entry.task == task && entry.run == run)
        {
            entries.remove(id);
        }
        Ok(())
    }
}

struct BudgetReader<R> {
    inner: R,
    remaining: Rc<Cell<usize>>,
}
impl<R: Read> Read for BudgetReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let available = self.remaining.get();
        if available == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "history message limit",
            ));
        }
        let limit = buffer.len().min(available);
        let count = self.inner.read(&mut buffer[..limit])?;
        self.remaining.set(available - count);
        Ok(count)
    }
}

struct EnvelopeSeed<'a> {
    snapshot: &'a mut HistorySnapshot,
    budget: Rc<Cell<usize>>,
}
impl<'de> DeserializeSeed<'de> for EnvelopeSeed<'_> {
    type Value = String;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<String, D::Error> {
        decoder.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for EnvelopeSeed<'_> {
    type Value = String;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("RPC history response")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<String, M::Error> {
        let mut keys = HashSet::new();
        let (mut id, mut kind, mut command, mut success, mut data) =
            (None, None, None, false, false);
        while let Some(key) = map.next_key::<String>()? {
            if keys.len() >= 64 || !keys.insert(key.clone()) {
                return Err(de::Error::custom("duplicate or excessive fields"));
            }
            match key.as_str() {
                "id" => id = Some(map.next_value::<String>()?),
                "type" => kind = Some(map.next_value::<String>()?),
                "command" => command = Some(map.next_value::<String>()?),
                "success" => success = map.next_value::<bool>()?,
                "data" => {
                    map.next_value_seed(DataSeed {
                        snapshot: self.snapshot,
                        budget: self.budget.clone(),
                    })?;
                    data = true;
                }
                _ => {
                    map.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        if kind.as_deref() != Some("response")
            || command.as_deref() != Some("get_messages")
            || !success
            || !data
        {
            return Err(de::Error::custom("not a successful history response"));
        }
        id.filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or_else(|| de::Error::custom("invalid response id"))
    }
}

struct DataSeed<'a> {
    snapshot: &'a mut HistorySnapshot,
    budget: Rc<Cell<usize>>,
}
impl<'de> DeserializeSeed<'de> for DataSeed<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        decoder.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for DataSeed<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("history data")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        let mut found = false;
        let mut fields = 0;
        while let Some(key) = map.next_key::<String>()? {
            fields += 1;
            if fields > 64 {
                return Err(de::Error::custom("excessive history fields"));
            }
            if key == "messages" {
                if found {
                    return Err(de::Error::custom("duplicate messages"));
                }
                found = true;
                map.next_value_seed(MessagesSeed {
                    snapshot: self.snapshot,
                    budget: self.budget.clone(),
                })?;
            } else {
                map.next_value::<de::IgnoredAny>()?;
            }
        }
        if found {
            Ok(())
        } else {
            Err(de::Error::custom("missing messages"))
        }
    }
}

struct MessagesSeed<'a> {
    snapshot: &'a mut HistorySnapshot,
    budget: Rc<Cell<usize>>,
}
impl<'de> DeserializeSeed<'de> for MessagesSeed<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        decoder.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for MessagesSeed<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("history messages")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> Result<(), S::Error> {
        loop {
            self.budget.set(MAX_MESSAGE_BYTES);
            let Some(message) = sequence.next_element::<Value>()? else {
                break;
            };
            self.snapshot.append(message).map_err(de::Error::custom)?;
        }
        self.budget.set(MAX_MESSAGE_BYTES);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    fn messages(count: usize) -> Vec<Value> {
        (0..count).map(|index| json!({"role":"user","timestamp":index,"content":format!("message-{index}")})).collect()
    }

    #[test]
    fn header_probe_stops_before_body_and_falls_back_for_ambiguous_headers() {
        let prefix =
            br#"{"id":"request","type":"response","command":"get_messages","success":true,"data":"#;
        assert_eq!(
            history_response_id(Cursor::new(prefix)),
            Some("request".into())
        );
        for input in [
            r#"{"data":[],"id":"request","type":"response","command":"get_messages","success":true}"#,
            r#"{"id":"a","id":"b","type":"response","command":"get_messages","success":true,"data":{}}"#,
            r#"{"id":"a","type":"response","command":"get_state","success":true,"data":{}}"#,
            r#"{"id":"a","type":"response","command":"get_messages","success":false,"data":{}}"#,
        ] {
            assert_eq!(history_response_id(Cursor::new(input)), None);
        }
        let padded = format!(
            "{}{}",
            " ".repeat(64 * 1024),
            String::from_utf8_lossy(prefix)
        );
        assert_eq!(history_response_id(Cursor::new(padded)), None);
    }

    #[test]
    fn pages_backwards_without_repeating_or_reordering_messages() {
        let mut snapshot = HistorySnapshot::from_messages(messages(230)).unwrap();
        let newest = snapshot.page(None, 42).unwrap();
        assert_eq!((newest.start, newest.end, newest.total), (130, 230, 230));
        assert_eq!(newest.messages[0]["content"], "message-130");
        let middle = snapshot.page(Some(newest.start), 42).unwrap();
        let oldest = snapshot.page(Some(middle.start), 42).unwrap();
        assert_eq!((middle.start, middle.end), (30, 130));
        assert_eq!((oldest.start, oldest.end), (0, 30));
        assert_eq!(newest.snapshot_id, oldest.snapshot_id);
        assert_eq!(oldest.event_sequence, 42);
        assert!(snapshot.page(Some(231), 42).is_err());
        assert!(snapshot.page(Some(0), 42).unwrap().messages.is_empty());
    }

    #[test]
    fn streams_an_aggregate_response_larger_than_the_normal_frame_limit() {
        let values: Vec<Value> = (0..150)
            .map(|index| json!({"role":"assistant","timestamp":index,"content":"x".repeat(70_000)}))
            .collect();
        let input = serde_json::to_vec(&json!({"id":"request","type":"response","command":"get_messages","success":true,"data":{"messages":values}})).unwrap();
        assert!(input.len() > 8 * 1024 * 1024);
        let (id, mut snapshot) = HistorySnapshot::read_response(Cursor::new(input)).unwrap();
        assert_eq!(id, "request");
        let page = snapshot.page(None, 5).unwrap();
        assert_eq!(page.total, 150);
        assert!(page.messages.len() < 100);
        assert_eq!(page.messages.last().unwrap()["timestamp"], 149);
    }

    #[test]
    fn rejects_invalid_envelopes_duplicate_arrays_and_oversized_messages() {
        for value in [
            json!({"id":"x","type":"event","command":"get_messages","success":true,"data":{"messages":[]}}),
            json!({"id":"x","type":"response","command":"get_messages","success":true,"data":{}}),
            json!({"id":"x","type":"response","command":"get_state","success":true,"data":{"messages":[]}}),
        ] {
            assert!(HistorySnapshot::read_response(Cursor::new(
                serde_json::to_vec(&value).unwrap()
            ))
            .is_err());
        }
        assert!(HistorySnapshot::read_response(Cursor::new(br#"{"id":"x","type":"response","command":"get_messages","success":true,"data":{"messages":[],"messages":[]}}"#)).is_err());
        assert!(HistorySnapshot::from_messages(vec![
            json!({"role":"user","content":"x".repeat(MAX_MESSAGE_BYTES + 1)})
        ])
        .is_err());
    }

    #[test]
    fn snapshots_preserve_literal_message_values_and_do_not_share_identifiers() {
        let content = json!({"role":"custom","content":[{"type":"text","text":"中文\r\n\u{2028}"},{"type":"future","data":{"x":1}}]});
        let mut first = HistorySnapshot::from_messages(vec![content.clone()]).unwrap();
        let mut second = HistorySnapshot::from_messages(vec![]).unwrap();
        let page = first.page(None, 0).unwrap();
        assert_eq!(page.messages, vec![content]);
        assert_ne!(page.snapshot_id, second.page(None, 0).unwrap().snapshot_id);
    }

    #[test]
    fn store_is_scoped_by_task_and_run_and_old_closes_cannot_remove_a_new_snapshot() {
        let store = HistoryStore::default();
        let old = store
            .insert(
                "task",
                "run",
                HistorySnapshot::from_messages(messages(3)).unwrap(),
                4,
            )
            .unwrap();
        assert!(store.page("other", "run", &old.snapshot_id, 1).is_err());
        assert!(store
            .page("task", "other-run", &old.snapshot_id, 1)
            .is_err());
        store.close("other", "run", &old.snapshot_id).unwrap();
        assert!(store.page("task", "run", &old.snapshot_id, 1).is_ok());
        let new = store
            .insert(
                "task",
                "run",
                HistorySnapshot::from_messages(messages(4)).unwrap(),
                8,
            )
            .unwrap();
        assert!(store.page("task", "run", &old.snapshot_id, 1).is_ok());
        store.close("task", "run", &old.snapshot_id).unwrap();
        assert!(store.page("task", "run", &old.snapshot_id, 1).is_err());
        assert_eq!(
            store
                .page("task", "run", &new.snapshot_id, 2)
                .unwrap()
                .event_sequence,
            8
        );
        store.close("task", "run", &new.snapshot_id).unwrap();
        assert!(store.page("task", "run", &new.snapshot_id, 1).is_err());
    }

    #[test]
    fn cache_capacity_rejects_new_leases_without_evicting_existing_ones() {
        let store = HistoryStore::default();
        let first = store
            .insert(
                "task",
                "run",
                HistorySnapshot::from_messages(messages(1)).unwrap(),
                0,
            )
            .unwrap();
        for _ in 1..64 {
            store
                .insert(
                    "task",
                    "run",
                    HistorySnapshot::from_messages(vec![]).unwrap(),
                    0,
                )
                .unwrap();
        }
        assert!(store
            .insert(
                "task",
                "run",
                HistorySnapshot::from_messages(vec![]).unwrap(),
                0
            )
            .is_err());
        assert!(store.page("task", "run", &first.snapshot_id, 1).is_ok());
        store.close("task", "run", &first.snapshot_id).unwrap();
        assert!(store
            .insert(
                "task",
                "run",
                HistorySnapshot::from_messages(vec![]).unwrap(),
                0
            )
            .is_ok());
    }

    #[test]
    fn idle_expiration_releases_abandoned_leases_and_retains_recent_pages() {
        let store = HistoryStore::default();
        let old = store
            .insert(
                "task",
                "old",
                HistorySnapshot::from_messages(messages(1)).unwrap(),
                0,
            )
            .unwrap();
        store
            .0
            .lock()
            .unwrap()
            .get_mut(&old.snapshot_id)
            .unwrap()
            .last_access = std::time::Instant::now() - std::time::Duration::from_secs(3600);
        let current = store
            .insert(
                "task",
                "current",
                HistorySnapshot::from_messages(messages(1)).unwrap(),
                0,
            )
            .unwrap();
        assert!(store.page("task", "old", &old.snapshot_id, 1).is_err());
        assert!(store
            .page("task", "current", &current.snapshot_id, 1)
            .is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }
}
