use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::{
    collections::{BTreeMap, HashSet},
    time::Instant,
};

#[cfg(test)]
use crate::task::TaskStore;
use crate::{app_paths::AppPaths, snapshot::reject_link};

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
        Err(_) => return Err("无法读取本机 Pi 配置".into()),
    };
    let mut bytes = Vec::new();
    file.take(CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取本机 Pi 配置")?;
    if bytes.len() as u64 > CONFIG_BYTES {
        return Err("本机 Pi 配置超过 1 MiB".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "本机 Pi 配置不是有效 JSON")?;
    if !value.is_object() {
        return Err("本机 Pi 配置必须是 JSON 对象".into());
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
    let mut file = File::open(path).map_err(|_| "无法读取本机 Pi 会话")?;
    let before = file.metadata().map_err(|_| "无法读取本机会话元数据")?;
    if !before.is_file() {
        return Err("不是会话文件".into());
    }
    let mut head = Vec::new();
    (&mut file)
        .take(WINDOW_BYTES)
        .read_to_end(&mut head)
        .map_err(|_| "无法读取会话头")?;
    let end = head
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("会话头不完整或超过上限")?;
    let header: Value = serde_json::from_slice(&head[..end]).map_err(|_| "会话头无效")?;
    if header["type"] != "session" || !matches!(header["version"].as_u64().unwrap_or(1), 1..=3) {
        return Err("不支持的 Pi 会话格式".into());
    }
    let id = header["id"]
        .as_str()
        .filter(|id| valid_session_id(id))
        .ok_or("会话 ID 无效")?;
    let cwd = header["cwd"]
        .as_str()
        .filter(|cwd| cwd.len() <= 4096 && Path::new(cwd).is_absolute())
        .ok_or("会话项目目录无效")?;
    let mut title = id.to_owned();
    let tail_start = before.len().saturating_sub(WINDOW_BYTES);
    let tail = if tail_start == 0 {
        head
    } else {
        file.seek(SeekFrom::Start(tail_start))
            .map_err(|_| "无法读取会话尾")?;
        let mut tail = Vec::new();
        (&mut file)
            .take(WINDOW_BYTES)
            .read_to_end(&mut tail)
            .map_err(|_| "无法读取会话尾")?;
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
    let after = file.metadata().map_err(|_| "无法检查会话元数据")?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("会话正在写入，稍后重试".into());
    }
    let timestamp = before
        .created()
        .or_else(|_| before.modified())
        .unwrap_or(SystemTime::now())
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "会话时间无效")?
        .as_millis();
    Ok(NativeSession {
        id: id.to_owned(),
        title,
        cwd: cwd.to_owned(),
        file: path.to_string_lossy().into_owned(),
        timestamp: i64::try_from(timestamp).map_err(|_| "会话时间超出范围")?,
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
                .ok_or("无法解析本机会话目录")?
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
        return Err("会话文件已被替换，ID 不匹配".into());
    }
    let expected = fs::canonicalize(cwd).map_err(|_| "任务项目目录不可用")?;
    let actual = fs::canonicalize(session.cwd).map_err(|_| "会话项目目录不可用")?;
    if actual != expected {
        return Err("会话文件所属项目已变化".into());
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

pub fn validate_reported_session(
    paths: &AppPaths,
    task: &crate::task::TaskRecord,
    id: &str,
    file: &str,
) -> Result<(), String> {
    if id != task.session_id {
        return Err("Pi 返回的会话 ID 与任务不一致".into());
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
        return Err("Pi 返回的会话文件路径无效".into());
    }
    reject_link(path)?;
    if let Some(expected) = &task.session_file {
        if session_key("", expected) != session_key("", file) {
            return Err("Pi 返回的会话文件与已绑定文件不一致".into());
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
    let parent = path.parent().ok_or("会话路径缺少父目录")?;
    let parent = fs::canonicalize(parent).map_err(|_| "会话目录不可用")?;
    if !roots
        .iter()
        .any(|root| fs::canonicalize(root).is_ok_and(|root| parent.starts_with(root)))
    {
        return Err("Pi 会话文件不在已配置的会话目录中".into());
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
        assert!(error.contains("仅支持 DeepPi 托管任务"), "{error}");
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
}
