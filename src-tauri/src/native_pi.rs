use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::time::Instant;

#[cfg(test)]
use crate::task::TaskStore;
use crate::{app_paths::AppPaths, message::msg, snapshot::reject_link};

const CONFIG_BYTES: u64 = 1024 * 1024;
const WINDOW_BYTES: u64 = 64 * 1024;
#[cfg(test)]
const MAX_ENTRIES: usize = 20_000;
#[cfg(test)]
const MAX_SESSIONS: usize = 5_000;

#[derive(Debug)]
#[allow(dead_code)]
pub struct NativeSession {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub file: String,
    pub timestamp: i64,
}

#[cfg(test)]
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub truncated: bool,
}

#[cfg(test)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeProvider {
    id: String,
    name: String,
    api: Option<String>,
    auth_type: Option<String>,
    models: Vec<String>,
}

pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id.as_bytes()[id.len() - 1].is_ascii_alphanumeric()
}

pub fn session_key(home: &str, file: &str) -> String {
    let normalize = |path: &str| {
        path.trim_start_matches(r"\\?\")
            .replace('\\', "/")
            .to_lowercase()
    };
    let mut hash = Sha256::new();
    hash.update(normalize(home));
    hash.update([0]);
    hash.update(normalize(file));
    format!("{:x}", hash.finalize())
}

fn bounded_json(path: &Path) -> Result<Value, String> {
    reject_link(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!({}))
        }
        Err(_) => return Err(msg("native.config.read_failed")),
    };
    let mut bytes = Vec::new();
    file.take(CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| msg("native.config.read_failed"))?;
    if bytes.len() as u64 > CONFIG_BYTES {
        return Err(msg("native.config.too_large"));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| msg("native.config.invalid_json"))?;
    if !value.is_object() {
        return Err(msg("native.config.not_object"));
    }
    Ok(value)
}

#[cfg(test)]
fn provider_metadata(home: &Path) -> Result<Vec<NativeProvider>, String> {
    let models = bounded_json(&home.join("models.json"))?;
    let auth = bounded_json(&home.join("auth.json"))?;
    let mut result = BTreeMap::new();
    if let Some(providers) = models.get("providers").and_then(Value::as_object) {
        if providers.len() > 200 {
            return Err("本机 Pi 供应商数量超过上限".into());
        }
        for (id, value) in providers {
            if !valid_session_id(id) {
                continue;
            }
            result.insert(
                id.clone(),
                NativeProvider {
                    id: id.clone(),
                    name: value["name"]
                        .as_str()
                        .unwrap_or(id)
                        .chars()
                        .take(128)
                        .collect(),
                    api: value["api"]
                        .as_str()
                        .map(|api| api.chars().take(128).collect()),
                    auth_type: value["apiKey"]
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .map(|_| "configured".into()),
                    models: value["models"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(200)
                        .filter_map(|model| model["id"].as_str())
                        .map(|id| id.chars().take(256).collect())
                        .collect(),
                },
            );
        }
    }
    if let Some(entries) = auth.as_object() {
        if entries.len() > 200 {
            return Err("本机 Pi 认证条目数量超过上限".into());
        }
        for (id, value) in entries {
            if !valid_session_id(id) {
                continue;
            }
            let kind = value["type"]
                .as_str()
                .filter(|kind| matches!(*kind, "oauth" | "api_key"));
            let Some(kind) = kind else {
                continue;
            };
            let provider = result.entry(id.clone()).or_insert_with(|| NativeProvider {
                id: id.clone(),
                name: id.clone(),
                api: None,
                auth_type: None,
                models: Vec::new(),
            });
            provider.auth_type = Some(kind.to_owned());
        }
    }
    Ok(result.into_values().collect())
}

fn parse_session(path: &Path) -> Result<NativeSession, String> {
    reject_link(path)?;
    let mut file = File::open(path).map_err(|_| msg("native.session.open_failed"))?;
    let before = file
        .metadata()
        .map_err(|_| msg("native.session.metadata_failed"))?;
    if !before.is_file() {
        return Err(msg("native.session.not_a_file"));
    }
    let mut head = Vec::new();
    (&mut file)
        .take(WINDOW_BYTES)
        .read_to_end(&mut head)
        .map_err(|_| msg("native.session.head_read_failed"))?;
    let end = head
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or(msg("native.session.head_incomplete"))?;
    let header: Value =
        serde_json::from_slice(&head[..end]).map_err(|_| msg("native.session.head_invalid"))?;
    if header["type"] != "session" || !matches!(header["version"].as_u64().unwrap_or(1), 1..=3) {
        return Err(msg("native.session.unsupported_format"));
    }
    let id = header["id"]
        .as_str()
        .filter(|id| valid_session_id(id))
        .ok_or(msg("native.session.id_invalid"))?;
    let cwd = header["cwd"]
        .as_str()
        .filter(|cwd| cwd.len() <= 4096 && Path::new(cwd).is_absolute())
        .ok_or(msg("native.session.cwd_invalid"))?;
    let mut title = id.to_owned();
    let tail_start = before.len().saturating_sub(WINDOW_BYTES);
    let tail = if tail_start == 0 {
        head
    } else {
        file.seek(SeekFrom::Start(tail_start))
            .map_err(|_| msg("native.session.tail_read_failed"))?;
        let mut tail = Vec::new();
        (&mut file)
            .take(WINDOW_BYTES)
            .read_to_end(&mut tail)
            .map_err(|_| msg("native.session.tail_read_failed"))?;
        tail
    };
    // Only complete JSONL records are candidates, including when the last append is partial.
    let start = if tail_start == 0 {
        0
    } else {
        tail.iter()
            .position(|byte| *byte == b'\n')
            .map_or(tail.len(), |position| position + 1)
    };
    for line in tail[start..].split_inclusive(|byte| *byte == b'\n') {
        if line.last() != Some(&b'\n') {
            continue;
        }
        if let Ok(value) = serde_json::from_slice::<Value>(line) {
            if value["type"] == "session_info" {
                if let Some(name) = value["name"]
                    .as_str()
                    .filter(|name| !name.trim().is_empty())
                {
                    title = name
                        .chars()
                        .filter(|ch| !ch.is_control())
                        .take(120)
                        .collect();
                }
            }
        }
    }
    let after = file
        .metadata()
        .map_err(|_| msg("native_pi.session_metadata_unreadable"))?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(msg("native.session.write_in_progress"));
    }
    let timestamp = before
        .created()
        .or_else(|_| before.modified())
        .unwrap_or(SystemTime::now())
        .duration_since(UNIX_EPOCH)
        .map_err(|_| msg("native.session.time_invalid"))?
        .as_millis();
    Ok(NativeSession {
        id: id.to_owned(),
        title,
        cwd: cwd.to_owned(),
        file: path.to_string_lossy().into_owned(),
        timestamp: i64::try_from(timestamp).map_err(|_| msg("native.session.time_out_of_range"))?,
    })
}

#[cfg(test)]
fn scan_sessions(roots: &[PathBuf]) -> Result<(Vec<NativeSession>, ImportReport), String> {
    let started = Instant::now();
    let mut paths = Vec::new();
    let mut report = ImportReport::default();
    let mut visited = HashSet::new();
    let mut entries = 0;
    for root in roots {
        if !root.exists() {
            continue;
        }
        reject_link(root)?;
        let mut directories = vec![(root.clone(), 0)];
        while let Some((directory, depth)) = directories.pop() {
            if !visited.insert(directory.clone()) {
                continue;
            }
            let children = match fs::read_dir(directory) {
                Ok(children) => children,
                Err(_) => {
                    report.skipped += 1;
                    continue;
                }
            };
            for child in children {
                entries += 1;
                if entries > MAX_ENTRIES
                    || paths.len() >= MAX_SESSIONS
                    || started.elapsed().as_secs() >= 10
                {
                    report.truncated = true;
                    break;
                }
                let Ok(child) = child else {
                    report.skipped += 1;
                    continue;
                };
                let path = child.path();
                if reject_link(&path).is_err() {
                    report.skipped += 1;
                    continue;
                }
                let Ok(kind) = child.file_type() else {
                    report.skipped += 1;
                    continue;
                };
                if kind.is_dir() && depth == 0 {
                    directories.push((path, depth + 1));
                } else if kind.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "jsonl")
                {
                    paths.push(path);
                }
            }
            if report.truncated {
                break;
            }
        }
        if report.truncated {
            break;
        }
    }
    paths.sort();
    paths.dedup();
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4);
    let chunk_size = paths.len().div_ceil(workers).max(1);
    let mut sessions = Vec::new();
    std::thread::scope(|scope| {
        let jobs: Vec<_> = paths
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|path| parse_session(path))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for job in jobs {
            match job.join() {
                Ok(results) => {
                    for result in results {
                        match result {
                            Ok(session) => sessions.push(session),
                            Err(_) => report.skipped += 1,
                        }
                    }
                }
                Err(_) => report.truncated = true,
            }
        }
    });
    sessions.sort_by(|a, b| {
        b.timestamp
            .cmp(&a.timestamp)
            .then_with(|| a.file.cmp(&b.file))
    });
    Ok((sessions, report))
}

fn session_roots(
    paths: &AppPaths,
    home: &Path,
    projects: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    let mut roots = vec![home.join("sessions")];
    let settings = bounded_json(&home.join("settings.json"))?;
    // 只读托管 home 的配置；不读宿主环境变量，避免受本机安装的 Pi 影响。
    let custom = settings["sessionDir"]
        .as_str()
        .filter(|path| !path.is_empty())
        .map(str::to_owned);
    if let Some(custom) = custom {
        let path = if let Some(suffix) = custom
            .strip_prefix("~/")
            .or_else(|| custom.strip_prefix("~\\"))
        {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| PathBuf::from(home).join(suffix))
                .ok_or(msg("native.session.home_unresolved"))?
        } else {
            PathBuf::from(custom)
        };
        if path.is_absolute() {
            roots.push(path);
        } else {
            roots.push(paths.project_root.join(&path));
            roots.extend(projects.iter().map(|project| project.join(&path)));
        }
    }
    Ok(roots)
}

pub fn validate_resume(file: &str, id: &str, cwd: &str) -> Result<(), String> {
    let session = parse_session(Path::new(file))?;
    if session.id != id {
        return Err(msg("native.session.id_mismatch"));
    }
    let expected =
        fs::canonicalize(cwd).map_err(|_| msg("native.session.task_project_unavailable"))?;
    let actual =
        fs::canonicalize(session.cwd).map_err(|_| msg("native.session.project_unavailable"))?;
    if actual != expected {
        return Err(msg("native.session.project_changed"));
    }
    Ok(())
}

pub fn session_arguments(record: &crate::task::TaskRecord) -> Result<Vec<String>, String> {
    // Pi 可能在首次写入前就上报会话文件路径，用户也可能删除会话文件；
    // 记录的路径不存在时回退到 `--session-id`（Pi 会按 id 恢复已有会话或新建文件），
    // 避免任务因为一个尚不存在的路径而永远无法重启。
    let resume = record
        .session_file
        .as_deref()
        .filter(|file| Path::new(file).is_file());
    let mut arguments = if let Some(file) = resume {
        validate_resume(file, &record.session_id, &record.project_path)?;
        vec!["--session".into(), file.to_owned()]
    } else {
        if !valid_session_id(&record.session_id) {
            return Err("Invalid Pi session ID".into());
        }
        vec!["--session-id".into(), record.session_id.clone()]
    };
    arguments.extend(["--name".into(), record.title.clone()]);
    Ok(arguments)
}

/// 休眠会话历史：单文件读取字节上限（与 pi MAX_HISTORY_BYTES 同量级）。
const MAX_SESSION_LOG_BYTES: u64 = 192 * 1024 * 1024;
/// 休眠会话历史：单行（单条消息）字节上限。
const MAX_SESSION_LOG_LINE: usize = 32 * 1024 * 1024;
/// 休眠会话历史：条目数上限，防止异常文件撑爆内存。
const MAX_SESSION_LOG_ENTRIES: usize = 200_000;
/// 会话目录搜索的目录数上限（sessions/<cwd 目录>/<文件> 只有两层，留足余量）。
const MAX_SESSION_SCAN_DIRS: usize = 512;
const MAX_SESSION_SCAN_FILES: usize = 20_000;

/// 休眠历史投影出的会话设置：叶子路径上最后一次记录的模型与推理强度。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSettings {
    pub model: Option<SessionModel>,
    pub thinking_level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionModel {
    pub provider: String,
    pub id: String,
}

/// 一次读取得到的休眠会话内容：投影消息 + 会话设置（避免为两者重复读文件）。
#[derive(Debug)]
pub struct SessionHistory {
    pub messages: Vec<Value>,
    pub settings: SessionSettings,
}

/// 把 pi 记录的 ISO 时间戳（`new Date().toISOString()` 产物，恒为 `Z` 结尾）转成毫秒。
/// 只接受 UTC 形式；解析失败返回 None，与 pi 对缺失时间的容错一致。
fn iso_to_epoch_ms(value: Option<&Value>) -> Option<i64> {
    let text = value?.as_str()?;
    let body = text.strip_suffix('Z').unwrap_or(text);
    let (date, time) = body.split_once('T')?;
    let (year, month, day) = parse_date(date)?;
    let (hour, minute, second, millis) = parse_time(time)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000 + millis)
}

fn parse_date(date: &str) -> Option<(i64, i64, i64)> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((year, month, day))
}

fn parse_time(time: &str) -> Option<(i64, i64, i64, i64)> {
    let (clock, fraction) = match time.split_once('.') {
        Some((clock, fraction)) => (clock, fraction),
        None => (time, ""),
    };
    let mut parts = clock.split(':');
    let hour = parts.next()?.parse::<i64>().ok()?;
    let minute = parts.next()?.parse::<i64>().ok()?;
    let second = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let mut millis = 0i64;
    if !fraction.is_empty() {
        // 只取前三位毫秒；更长的精度直接截断（与 JS Date 一致）。
        let digits: String = fraction.chars().take(3).collect();
        let padded = format!("{digits:0<3}");
        millis = padded.parse::<i64>().ok()?;
    }
    Some((hour, minute, second, millis))
}

/// Howard Hinnant 的 days_from_civil：公历日期 → 自 1970-01-01 的天数。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// 把一条会话条目投影为与 RPC `get_messages` 相同结构的消息；非消息条目返回 None。
fn project_session_entry(entry: &Value) -> Option<Value> {
    match entry["type"].as_str()? {
        "message" => {
            let mut message = entry["message"].as_object()?.clone();
            // pi 对旧文件里 content 缺失的 user/assistant/toolResult 消息补空数组。
            let role = message.get("role").and_then(Value::as_str).unwrap_or("");
            let content_missing = message.get("content").is_none_or(Value::is_null);
            if content_missing && matches!(role, "user" | "assistant" | "toolResult") {
                message.insert("content".into(), Value::Array(Vec::new()));
            }
            Some(Value::Object(message))
        }
        "custom_message" => Some(json!({
            "role": "custom",
            "customType": entry["customType"],
            "content": entry.get("content").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
            "display": entry["display"],
            "details": entry["details"],
            "timestamp": iso_to_epoch_ms(entry.get("timestamp")),
        })),
        "branch_summary" => Some(json!({
            "role": "branchSummary",
            "summary": entry["summary"],
            "fromId": entry["fromId"],
            "timestamp": iso_to_epoch_ms(entry.get("timestamp")),
        })),
        "compaction" => Some(json!({
            "role": "compactionSummary",
            "summary": entry["summary"],
            "tokensBefore": entry["tokensBefore"],
            "timestamp": iso_to_epoch_ms(entry.get("timestamp")),
        })),
        _ => None,
    }
}

/// 会话条目按 id/parentId 建索引：v1 文件没有 id/parentId，按线性顺序补齐
///（与 pi 的 migrate 等价）；已有 id 的条目保留文件里的 id 与 parentId（null 表示分支根）。
fn session_nodes(entries: &[Value]) -> Vec<(String, Option<String>, &Value)> {
    let mut nodes: Vec<(String, Option<String>, &Value)> = Vec::with_capacity(entries.len());
    let mut previous: Option<String> = None;
    for entry in &entries[1..] {
        if entry["type"] == "session" {
            continue;
        }
        let (id, parent) = match entry["id"].as_str() {
            Some(existing) => (
                existing.to_owned(),
                entry["parentId"].as_str().map(str::to_owned),
            ),
            None => {
                let id = format!("#{}", nodes.len());
                (id.clone(), previous.clone())
            }
        };
        previous = Some(id.clone());
        nodes.push((id, parent, entry));
    }
    nodes
}

/// 叶子 = 最后一个条目；沿 parentId 上溯到根（带访问集防环），返回正序下标路径。
fn session_leaf_path(nodes: &[(String, Option<String>, &Value)]) -> Vec<usize> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let by_id: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, (id, _, _))| (id.as_str(), index))
        .collect();
    let mut path: Vec<usize> = Vec::new();
    let mut visited: HashSet<usize> = HashSet::new();
    let mut cursor = Some(nodes.len() - 1);
    while let Some(index) = cursor {
        if !visited.insert(index) {
            break;
        }
        path.push(index);
        cursor = nodes[index]
            .1
            .as_deref()
            .and_then(|parent| by_id.get(parent).copied());
    }
    path.reverse();
    path
}

/// 压缩感知的上下文截取：只保留最后一次压缩摘要、firstKeptEntryId 起的被保留条目
/// 与压缩之后的条目（与 pi 的 `buildContextEntries` 等价）。
fn session_context_indices(
    nodes: &[(String, Option<String>, &Value)],
    path: &[usize],
) -> Vec<usize> {
    match path
        .iter()
        .rposition(|&index| nodes[index].2["type"] == "compaction")
    {
        Some(position) => {
            let compaction = path[position];
            let kept_id = nodes[compaction].2["firstKeptEntryId"].as_str();
            let mut context = vec![compaction];
            let mut kept_found = false;
            for &index in &path[..position] {
                if kept_id.is_some_and(|kept| nodes[index].0 == kept) {
                    kept_found = true;
                }
                if kept_found {
                    context.push(index);
                }
            }
            context.extend_from_slice(&path[position + 1..]);
            context
        }
        None => path.to_vec(),
    }
}

/// 叶子路径上的模型与推理强度（与 pi `getSessionContextSettings` 等价）：
/// 模型取路径上最后一次 model_change，退而取最后一条 assistant 消息的 provider/model；
/// 推理强度取最后一次 thinking_level_change。
fn session_settings(nodes: &[(String, Option<String>, &Value)], path: &[usize]) -> SessionSettings {
    let mut model = None;
    let mut thinking_level = None;
    for &index in path {
        let entry = nodes[index].2;
        match entry["type"].as_str() {
            Some("thinking_level_change") => {
                if let Some(level) = entry["thinkingLevel"].as_str() {
                    thinking_level = Some(level.to_owned());
                }
            }
            Some("model_change") => {
                if let (Some(provider), Some(id)) =
                    (entry["provider"].as_str(), entry["modelId"].as_str())
                {
                    model = Some(SessionModel {
                        provider: provider.to_owned(),
                        id: id.to_owned(),
                    });
                }
            }
            Some("message") if entry["message"]["role"] == "assistant" => {
                if let (Some(provider), Some(id)) = (
                    entry["message"]["provider"].as_str(),
                    entry["message"]["model"].as_str(),
                ) {
                    model = Some(SessionModel {
                        provider: provider.to_owned(),
                        id: id.to_owned(),
                    });
                }
            }
            _ => {}
        }
    }
    SessionSettings {
        model,
        thinking_level,
    }
}

/// 读取 pi 会话 JSONL 并投影为消息列表 + 会话设置（供休眠会话直接展示历史，无需启动进程）。
/// 文件首行必须是 `type: session` 的头部且 id 与任务一致；损坏行按 pi 的行为跳过。
pub fn read_session_history(file: &Path, expected_id: &str) -> Result<SessionHistory, String> {
    let handle =
        File::open(file).map_err(|error| format!("failed to open Pi session file: {error}"))?;
    if handle.metadata().map_err(|error| error.to_string())?.len() > MAX_SESSION_LOG_BYTES {
        return Err("Pi session file is too large".into());
    }
    let mut reader = BufReader::new(handle);
    let mut entries: Vec<Value> = Vec::new();
    let mut total = 0u64;
    let mut buffer: Vec<u8> = Vec::with_capacity(64 * 1024);
    loop {
        buffer.clear();
        let read = reader
            .read_until(b'\n', &mut buffer)
            .map_err(|error| format!("failed to read Pi session file: {error}"))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_SESSION_LOG_BYTES {
            return Err("Pi session file is too large".into());
        }
        if buffer.len() > MAX_SESSION_LOG_LINE {
            return Err("Pi session message is too large".into());
        }
        if entries.len() >= MAX_SESSION_LOG_ENTRIES {
            return Err("Pi session has too many entries".into());
        }
        let line = String::from_utf8_lossy(&buffer);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<Value>(trimmed) {
            entries.push(entry);
        }
    }
    let header = entries.first().ok_or("Pi session file is empty")?;
    if header["type"] != "session" {
        return Err(msg("native.session.not_a_session_file"));
    }
    if header["id"].as_str() != Some(expected_id) {
        return Err(msg("native.session.id_mismatch"));
    }
    let nodes = session_nodes(&entries);
    let path = session_leaf_path(&nodes);
    let settings = session_settings(&nodes, &path);
    let messages = session_context_indices(&nodes, &path)
        .iter()
        .filter_map(|&index| project_session_entry(nodes[index].2))
        .collect();
    Ok(SessionHistory { messages, settings })
}

/// 只取消息列表的兼容入口（测试与仅需消息的调用方使用）。
#[cfg(test)]
fn read_session_messages(file: &Path, expected_id: &str) -> Result<Vec<Value>, String> {
    Ok(read_session_history(file, expected_id)?.messages)
}

/// 在 pi 主目录的 sessions 下按 `<时间戳>_<session_id>.jsonl` 命名规则定位会话文件；
/// 多个匹配（分支/恢复产生的同名文件）取修改时间最新的一个。
pub fn find_session_file(agent_dir: &Path, session_id: &str) -> Result<Option<PathBuf>, String> {
    if !valid_session_id(session_id) {
        return Ok(None);
    }
    let suffix = format!("_{session_id}.jsonl");
    let mut directories = vec![agent_dir.join("sessions")];
    let mut scanned = 0usize;
    let mut matches: Vec<PathBuf> = Vec::new();
    while let Some(directory) = directories.pop() {
        scanned += 1;
        if scanned > MAX_SESSION_SCAN_DIRS {
            break;
        }
        let read = match fs::read_dir(&directory) {
            Ok(read) => read,
            Err(_) => continue,
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if directories.len() < MAX_SESSION_SCAN_DIRS {
                    directories.push(path);
                }
                continue;
            }
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&suffix))
            {
                if reject_link(&path).is_err() {
                    continue;
                }
                matches.push(path);
                if matches.len() >= MAX_SESSION_SCAN_FILES {
                    break;
                }
            }
        }
    }
    // 直接比较 SystemTime（NTFS 精度到 100ns）；不要截断到秒，否则同秒写入会平局，
    // 由目录遍历顺序随机决定胜者。
    let latest = matches
        .into_iter()
        .max_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok());
    Ok(latest)
}

/// 休眠历史的无状态分页：`before` 为 None 时取最后一页，否则取 `before` 之前的一页。
pub fn page_bounds(total: usize, before: Option<usize>, limit: Option<usize>) -> (usize, usize) {
    let limit = limit.unwrap_or(100).clamp(1, 100);
    let end = before.unwrap_or(total).min(total);
    let start = end.saturating_sub(limit);
    (start, end)
}

pub fn validate_reported_session(
    paths: &AppPaths,
    task: &crate::task::TaskRecord,
    id: &str,
    file: &str,
) -> Result<(), String> {
    if id != task.session_id {
        return Err(msg("native.session.reported_id_mismatch"));
    }
    let path = Path::new(file);
    if !path.is_absolute()
        || path
            .extension()
            .is_none_or(|extension| extension != "jsonl")
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(msg("native.session.reported_path_invalid"));
    }
    reject_link(path)?;
    if let Some(expected) = &task.session_file {
        if session_key("", expected) != session_key("", file) {
            return Err(msg("native.session.reported_file_mismatch"));
        }
        return Ok(());
    }
    let home = paths.task_pi_home(task)?;
    let project = PathBuf::from(&task.project_path);
    let mut roots = session_roots(paths, &home, std::slice::from_ref(&project))?;
    // Pi also accepts a project-local sessionDir. Do not execute configuration commands.
    let settings = bounded_json(&project.join(".pi/settings.json"))?;
    if let Some(directory) = settings["sessionDir"]
        .as_str()
        .filter(|value| !value.is_empty())
    {
        let directory = PathBuf::from(directory);
        roots.push(if directory.is_absolute() {
            directory
        } else {
            project.join(directory)
        });
    }
    let parent = path.parent().ok_or(msg("native.session.parent_missing"))?;
    let parent =
        fs::canonicalize(parent).map_err(|_| msg("native.session.directory_unavailable"))?;
    if !roots
        .iter()
        .any(|root| fs::canonicalize(root).is_ok_and(|root| parent.starts_with(root)))
    {
        return Err(msg("native.session.outside_configured_roots"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn session(root: &Path, id: &str) -> PathBuf {
        let path = root.join(format!("{id}.jsonl"));
        fs::write(
            &path,
            format!(
                "{}\n{}\n",
                json!({"type":"session","version":3,"id":id,"cwd":root}),
                json!({"type":"session_info","name":"Native task"}),
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn discovery_reads_metadata_and_keeps_native_files_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let path = session(root.path(), "native-session");
        let before = fs::read(&path).unwrap();
        let (sessions, report) = scan_sessions(&[root.path().to_owned()]).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "Native task");
        assert!(!report.truncated);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn latest_complete_name_wins_and_partial_append_is_ignored() {
        let root = tempfile::tempdir().unwrap();
        let path = session(root.path(), "native-session");
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{}", json!({"type":"session_info","name":"Renamed"})).unwrap();
        write!(file, "{{\"type\":\"session_info\",\"name\":\"partial").unwrap();
        assert_eq!(parse_session(&path).unwrap().title, "Renamed");
    }

    #[test]
    fn provider_metadata_never_serializes_credentials_or_headers() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("models.json"), json!({"providers":{"custom":{
            "api":"openai-responses","apiKey":"secret-model-key",
            "headers":{"Authorization":"secret-header"},"baseUrl":"https://example.com?key=secret-url",
            "models":[{"id":"model"}],
        }}}).to_string()).unwrap();
        fs::write(
            root.path().join("auth.json"),
            json!({"oauth-provider":{
                "type":"oauth","access":"secret-access","refresh":"secret-refresh",
            }})
            .to_string(),
        )
        .unwrap();
        let providers = provider_metadata(root.path()).unwrap();
        let encoded = serde_json::to_string(&providers).unwrap();
        assert!(!encoded.contains("secret"));
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[1].auth_type.as_deref(), Some("oauth"));
    }

    #[test]
    fn rejects_invalid_ids_and_wrong_session_on_resume() {
        assert!(valid_session_id("my-session.v2"));
        for id in ["", "../file", "-flag", "a/b", "a b"] {
            assert!(!valid_session_id(id));
        }
        let root = tempfile::tempdir().unwrap();
        let path = session(root.path(), "native-session");
        assert!(validate_resume(
            path.to_str().unwrap(),
            "another-id",
            root.path().to_str().unwrap()
        )
        .is_err());
    }

    #[test]
    fn native_import_is_idempotent_preserves_renames_and_honors_deletion() {
        let root = tempfile::tempdir().unwrap();
        let file = session(root.path(), "native-session");
        let sessions = vec![parse_session(&file).unwrap()];
        let store = TaskStore::in_memory().unwrap();
        assert_eq!(
            store
                .import_native_sessions(&sessions, root.path())
                .unwrap(),
            (1, 0)
        );
        let task = store.list().unwrap().remove(0);
        assert_eq!(task.pi_environment, "native");
        assert_eq!(task.interaction_mode, "rpc");
        assert_eq!(task.session_file.as_deref(), file.to_str());
        store.rename(&task.id, "Local title").unwrap();
        assert_eq!(
            store
                .import_native_sessions(&sessions, root.path())
                .unwrap(),
            (0, 0)
        );
        assert_eq!(store.list().unwrap()[0].title, "Local title");
        store.delete(&task.id).unwrap();
        assert_eq!(
            store
                .import_native_sessions(&sessions, root.path())
                .unwrap(),
            (0, 0)
        );
        assert!(store.list().unwrap().is_empty());
        assert!(file.is_file());
    }

    #[test]
    fn discovery_does_not_reactivate_a_removed_project() {
        let root = tempfile::tempdir().unwrap();
        let store = TaskStore::in_memory().unwrap();
        let project = store.add_project(root.path().to_str().unwrap()).unwrap();
        store.remove_project(&project.id).unwrap();
        let file = session(root.path(), "native-session");
        store
            .import_native_sessions(&[parse_session(&file).unwrap()], root.path())
            .unwrap();
        assert!(store.list_projects().unwrap().is_empty());
        assert_eq!(
            store.list().unwrap()[0].project_id.as_deref(),
            Some(project.id.as_str())
        );
    }

    #[test]
    fn restart_keeps_environment_and_uses_an_exact_file_not_a_new_session_id() {
        let root = tempfile::tempdir().unwrap();
        let store = TaskStore::in_memory().unwrap();
        let file = session(root.path(), "named-session");
        store
            .import_native_sessions(&[parse_session(&file).unwrap()], root.path())
            .unwrap();
        let task = store.list().unwrap().remove(0);
        let restarted = store.restart(&task.id).unwrap();
        assert_eq!(restarted.pi_environment, "native");
        let arguments = session_arguments(&restarted).unwrap();
        assert_eq!(arguments[0], "--session");
        assert_eq!(arguments[1], file.to_str().unwrap());
        assert!(!arguments.iter().any(|arg| arg == "--session-id"));
    }

    #[test]
    fn missing_session_file_falls_back_to_session_id() {
        let root = tempfile::tempdir().unwrap();
        let store = TaskStore::in_memory().unwrap();
        let file = session(root.path(), "gone-session");
        store
            .import_native_sessions(&[parse_session(&file).unwrap()], root.path())
            .unwrap();
        let task = store.list().unwrap().remove(0);
        assert!(task.session_file.is_some());
        // Pi 可能在首次写入前上报路径，用户也可能删除会话文件；两者都不应让重启永久失败。
        fs::remove_file(&file).unwrap();
        let restarted = store.restart(&task.id).unwrap();
        let arguments = session_arguments(&restarted).unwrap();
        assert_eq!(arguments[0], "--session-id");
        assert_eq!(arguments[1], restarted.session_id);
        assert!(!arguments.iter().any(|arg| arg == "--session"));
    }

    #[test]
    fn oversized_records_do_not_require_reading_an_entire_conversation() {
        let root = tempfile::tempdir().unwrap();
        let path = session(root.path(), "large-session");
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "{{\"type\":\"message\",\"text\":\"").unwrap();
        for _ in 0..32 {
            file.write_all(&vec![b'x'; 64 * 1024]).unwrap();
        }
        writeln!(file, "\"}}").unwrap();
        writeln!(
            file,
            "{}",
            json!({"type":"session_info","name":"Latest title"})
        )
        .unwrap();
        assert_eq!(parse_session(&path).unwrap().title, "Latest title");
    }

    #[test]
    fn equal_ids_in_different_files_are_distinct_and_deletion_is_scoped() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("one");
        let second = root.path().join("two");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let a = parse_session(&session(&first, "shared-id")).unwrap();
        let b = parse_session(&session(&second, "shared-id")).unwrap();
        let store = TaskStore::in_memory().unwrap();
        assert_eq!(
            store
                .import_native_sessions(&[a, b], root.path())
                .unwrap()
                .0,
            2
        );
        let task = store.list().unwrap().remove(0);
        store.delete(&task.id).unwrap();
        assert_eq!(
            store
                .import_native_sessions(
                    &[
                        parse_session(&first.join("shared-id.jsonl")).unwrap(),
                        parse_session(&second.join("shared-id.jsonl")).unwrap(),
                    ],
                    root.path()
                )
                .unwrap()
                .0,
            0
        );
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn legacy_native_task_is_preserved_but_cannot_start_or_resume() {
        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_roots(
            root.path().join("app"),
            root.path().join("local"),
            root.path().to_owned(),
        )
        .unwrap();
        let home = root.path().join("native-home");
        fs::create_dir_all(&home).unwrap();
        let file = session(&home, "native-session");
        let before = fs::read(&file).unwrap();
        let store = TaskStore::in_memory().unwrap();
        store
            .import_native_sessions(&[parse_session(&file).unwrap()], &home)
            .unwrap();
        let task = store.list().unwrap().remove(0);
        assert_eq!(task.pi_environment, "native");
        assert!(store.get(&task.id).unwrap().is_some());
        let error = paths.task_pi_home(&task).unwrap_err();
        assert!(error.contains("managed.native_task_unsupported"), "{error}");
        // 登记文件仍然保留并用于身份校验；换成其他会话文件会被拒绝。
        assert!(
            validate_reported_session(&paths, &task, &task.session_id, file.to_str().unwrap())
                .is_ok()
        );
        let other = session(&home, "other-session");
        assert!(validate_reported_session(
            &paths,
            &task,
            &task.session_id,
            other.to_str().unwrap()
        )
        .is_err());
        assert_eq!(fs::read(&file).unwrap(), before);
    }

    #[test]
    fn managed_task_resolves_its_bound_configuration_home() {
        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_roots(
            root.path().join("app"),
            root.path().join("local"),
            root.path().to_owned(),
        )
        .unwrap();
        let store = TaskStore::in_memory().unwrap();
        let project = store.add_project(root.path().to_str().unwrap()).unwrap();
        let task = store
            .create_pi_task_for_project(&project.id, "Managed task")
            .unwrap();
        store
            .set_pi_environment(&task.id, "managed", &paths.pi_home)
            .unwrap();
        let task = store.get(&task.id).unwrap().unwrap();
        assert_eq!(task.pi_environment, "managed");
        assert_eq!(
            paths.task_pi_home(&task).unwrap(),
            fs::canonicalize(&paths.pi_home).unwrap()
        );
    }

    fn write_session(path: &Path, lines: &[Value]) {
        fs::write(
            path,
            lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
    }

    fn header(id: &str) -> Value {
        json!({"type":"session","version":3,"id":id,"cwd":"C:\\proj"})
    }

    fn message_entry(id: &str, parent: Option<&str>, role: &str, content: &str) -> Value {
        let mut entry = json!({
            "type":"message","id":id,"parentId":parent,
            "timestamp":"2026-09-20T10:00:00.000Z",
            "message":{"role":role,"content":content},
        });
        if parent.is_none() {
            entry["parentId"] = Value::Null;
        }
        entry
    }

    #[test]
    fn dormant_history_projects_a_linear_session_into_messages() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        write_session(
            &file,
            &[
                header("session-1"),
                message_entry("a1", None, "user", "你好"),
                message_entry("a2", Some("a1"), "assistant", "你好！"),
                message_entry("a3", Some("a2"), "toolResult", "ok"),
                json!({"type":"model_change","id":"a4","parentId":"a3","provider":"p","modelId":"m"}),
            ],
        );
        let messages = read_session_messages(&file, "session-1").unwrap();
        let roles: Vec<&str> = messages
            .iter()
            .map(|message| message["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, ["user", "assistant", "toolResult"]);
        assert_eq!(messages[0]["content"], "你好");
    }

    #[test]
    fn dormant_history_keeps_only_the_compaction_aware_context() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        write_session(
            &file,
            &[
                header("session-1"),
                message_entry("a1", None, "user", "one"),
                message_entry("a2", Some("a1"), "assistant", "two"),
                json!({
                    "type":"compaction","id":"a3","parentId":"a2","firstKeptEntryId":"a2",
                    "summary":"sum","tokensBefore":1000,"timestamp":"2026-09-20T10:00:01.000Z",
                }),
                message_entry("a4", Some("a3"), "user", "three"),
            ],
        );
        let messages = read_session_messages(&file, "session-1").unwrap();
        let roles: Vec<&str> = messages
            .iter()
            .map(|message| message["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, ["compactionSummary", "assistant", "user"]);
        assert_eq!(messages[0]["summary"], "sum");
        assert_eq!(messages[0]["tokensBefore"], 1000);
        assert_eq!(messages[0]["timestamp"], 1_789_898_401_000i64);
        assert_eq!(messages[1]["content"], "two");
        assert_eq!(messages[2]["content"], "three");
    }

    #[test]
    fn dormant_history_synthesizes_custom_and_branch_messages() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        write_session(
            &file,
            &[
                header("session-1"),
                message_entry("a1", None, "user", "hi"),
                json!({
                    "type":"custom_message","id":"a2","parentId":"a1",
                    "customType":"bash","content":"ls","display":"$ ls",
                    "timestamp":"2026-09-20T10:00:02.500Z",
                }),
                json!({
                    "type":"branch_summary","id":"a3","parentId":"a2","summary":"br",
                    "fromId":"a1","timestamp":"2026-09-20T10:00:03.000Z",
                }),
            ],
        );
        let messages = read_session_messages(&file, "session-1").unwrap();
        let roles: Vec<&str> = messages
            .iter()
            .map(|message| message["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, ["user", "custom", "branchSummary"]);
        assert_eq!(messages[1]["customType"], "bash");
        assert_eq!(messages[1]["timestamp"], 1_789_898_402_500i64);
        assert_eq!(messages[2]["fromId"], "a1");
    }

    #[test]
    fn dormant_history_chains_v1_entries_without_ids() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        write_session(
            &file,
            &[
                json!({"type":"session","version":1,"id":"session-1","cwd":"C:\\proj"}),
                json!({"type":"message","timestamp":"2026-09-20T10:00:00.000Z","message":{"role":"user","content":"old"}}),
                json!({"type":"message","timestamp":"2026-09-20T10:00:01.000Z","message":{"role":"assistant","content":"older"}}),
            ],
        );
        let messages = read_session_messages(&file, "session-1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["content"], "old");
        assert_eq!(messages[1]["content"], "older");
    }

    #[test]
    fn dormant_history_rejects_foreign_session_ids_and_bad_headers() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        write_session(
            &file,
            &[header("session-1"), message_entry("a1", None, "user", "hi")],
        );
        assert!(read_session_messages(&file, "session-2").is_err());
        fs::write(&file, "not json\n").unwrap();
        assert!(read_session_messages(&file, "session-1").is_err());
    }

    #[test]
    fn dormant_history_tolerates_partial_trailing_lines_and_skips_corrupt_rows() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        write_session(
            &file,
            &[
                header("session-1"),
                message_entry("a1", None, "user", "kept"),
            ],
        );
        let mut handle = fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(handle, "{{\"type\":\"message\"").unwrap(); // 损坏行被跳过
        write!(handle, "{{\"partial\"").unwrap(); // 未写完的行被跳过
        drop(handle);
        let messages = read_session_messages(&file, "session-1").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["content"], "kept");
    }

    #[test]
    fn dormant_history_pages_backwards_without_crossing_the_start() {
        assert_eq!(page_bounds(250, None, None), (150, 250));
        assert_eq!(page_bounds(250, Some(150), None), (50, 150));
        assert_eq!(page_bounds(250, Some(30), None), (0, 30));
        assert_eq!(page_bounds(250, Some(30), Some(50)), (0, 30));
        assert_eq!(page_bounds(0, None, Some(400)), (0, 0));
    }

    #[test]
    fn dormant_history_reports_settings_from_the_leaf_path() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        write_session(
            &file,
            &[
                header("session-1"),
                message_entry("a1", None, "user", "hi"),
                {
                    let mut entry = message_entry("a2", Some("a1"), "assistant", "hello");
                    entry["message"]["provider"] = json!("anthropic");
                    entry["message"]["model"] = json!("claude-old");
                    entry
                },
                json!({"type":"model_change","id":"a3","parentId":"a2","provider":"openai","modelId":"gpt-5"}),
                json!({"type":"thinking_level_change","id":"a4","parentId":"a3","thinkingLevel":"high"}),
            ],
        );
        let history = read_session_history(&file, "session-1").unwrap();
        assert_eq!(
            history.settings.model,
            Some(SessionModel {
                provider: "openai".into(),
                id: "gpt-5".into()
            })
        );
        assert_eq!(history.settings.thinking_level.as_deref(), Some("high"));
        assert_eq!(history.messages.len(), 2);
    }

    #[test]
    fn dormant_history_falls_back_to_the_last_assistant_model() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        let mut first = message_entry("a1", None, "assistant", "one");
        first["message"]["provider"] = json!("p1");
        first["message"]["model"] = json!("m1");
        let mut second = message_entry("a2", Some("a1"), "assistant", "two");
        second["message"]["provider"] = json!("p2");
        second["message"]["model"] = json!("m2");
        write_session(&file, &[header("session-1"), first, second]);
        let history = read_session_history(&file, "session-1").unwrap();
        assert_eq!(
            history.settings.model,
            Some(SessionModel {
                provider: "p2".into(),
                id: "m2".into()
            })
        );
        assert_eq!(history.settings.thinking_level, None);
    }

    #[test]
    fn dormant_history_ignores_settings_off_the_leaf_branch() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("s.jsonl");
        let mut off_branch = message_entry("a2", Some("a1"), "assistant", "off branch");
        off_branch["message"]["provider"] = json!("stale-provider");
        off_branch["message"]["model"] = json!("stale-model");
        write_session(
            &file,
            &[
                header("session-1"),
                message_entry("a1", None, "user", "root"),
                off_branch,
                json!({"type":"model_change","id":"a3","parentId":"a1","provider":"on-path","modelId":"chosen"}),
                message_entry("a4", Some("a3"), "user", "leaf"),
            ],
        );
        let history = read_session_history(&file, "session-1").unwrap();
        assert_eq!(
            history.settings.model,
            Some(SessionModel {
                provider: "on-path".into(),
                id: "chosen".into()
            })
        );
        let contents: Vec<&str> = history
            .messages
            .iter()
            .map(|message| message["content"].as_str().unwrap())
            .collect();
        assert_eq!(contents, ["root", "leaf"]);
    }

    /// 连续两次写入可能落在同一 mtime 刻度内，「按修改时间取最新」会退化为
    /// 目录序的平局裁决（CI 上偶发）。显式把 mtime 拨开，让规则被确定性验证。
    fn backdate_mtime(path: &Path, offset_secs: u64) {
        let file = File::options().append(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - std::time::Duration::from_secs(offset_secs))
            .unwrap();
    }

    #[test]
    fn find_session_file_locates_by_id_suffix_and_prefers_the_latest() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("sessions").join("--C---proj--");
        fs::create_dir_all(&directory).unwrap();
        let old = directory.join("2026-09-20T10-00-00-000Z_abc12345.jsonl");
        fs::write(&old, "{}").unwrap();
        backdate_mtime(&old, 120);
        assert_eq!(
            find_session_file(root.path(), "abc12345").unwrap(),
            Some(old.clone())
        );
        let newer = directory.join("2026-09-20T11-00-00-000Z_abc12345.jsonl");
        fs::write(&newer, "{}").unwrap();
        backdate_mtime(&newer, 60);
        assert_eq!(
            find_session_file(root.path(), "abc12345").unwrap(),
            Some(newer)
        );
        // 重写旧文件并把它的修改时间拨到最新，验证“同 id 多文件取最新”的规则。
        fs::write(&old, "{\"x\":1}").unwrap();
        backdate_mtime(&old, 0);
        assert_eq!(
            find_session_file(root.path(), "abc12345").unwrap(),
            Some(old)
        );
        assert_eq!(find_session_file(root.path(), "ffffffff").unwrap(), None);
        // 非法 session_id 直接返回 None，不做目录扫描。
        assert_eq!(find_session_file(root.path(), "..").unwrap(), None);
    }
}
