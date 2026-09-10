use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use serde::Serialize;
use tauri::{Manager, State};

use crate::{
    project_files::{project_root, require_main, validate_relative, MAX_PREVIEW_BYTES},
    task::TaskStore,
};

#[derive(Clone, Default)]
pub struct FileEditGate(pub(crate) Arc<Mutex<()>>);

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SaveOutcome {
    Saved,
    Conflict,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub outcome: SaveOutcome,
    pub version: Option<String>,
    pub recovery_path: Option<String>,
    pub pending_path: Option<String>,
    pub detail: String,
}

#[cfg(test)]
fn save_text(
    root: &Path,
    relative: &str,
    content: &str,
    expected: Option<&str>,
    before_publish: impl FnOnce(),
) -> Result<SaveResult, String> {
    save_registered(root, relative, content, expected, before_publish, |_, _| {
        Ok(())
    })
}

pub(crate) fn save_registered(
    root: &Path,
    relative: &str,
    content: &str,
    expected: Option<&str>,
    before_publish: impl FnOnce(),
    register: impl FnOnce(&str, Option<&str>) -> Result<(), String>,
) -> Result<SaveResult, String> {
    validate_save(relative, content, expected)?;
    #[cfg(windows)]
    {
        save_windows(root, relative, content, expected, before_publish, register)
    }
    #[cfg(not(windows))]
    {
        let _ = (root, before_publish, register);
        Err("Secure project file saving currently requires Windows".into())
    }
}

fn validate_save(relative: &str, content: &str, expected: Option<&str>) -> Result<(), String> {
    validate_relative(relative, false)?;
    if content.len() as u64 > MAX_PREVIEW_BYTES || content.contains('\0') {
        return Err("Only UTF-8 text without NUL, up to 2 MiB, can be saved".into());
    }
    if expected.is_some_and(|version| {
        version.len() != 64 || !version.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err("Invalid expected file version".into());
    }
    Ok(())
}

impl SaveResult {
    fn conflict(detail: &str) -> Self {
        Self {
            outcome: SaveOutcome::Conflict,
            version: None,
            recovery_path: None,
            pending_path: None,
            detail: detail.into(),
        }
    }
}

#[cfg(windows)]
fn save_windows(
    root: &Path,
    relative: &str,
    content: &str,
    expected: Option<&str>,
    before_publish: impl FnOnce(),
    register: impl FnOnce(&str, Option<&str>) -> Result<(), String>,
) -> Result<SaveResult, String> {
    use crate::project_files::{read_guarded_text, read_text, GuardedPath};
    use std::{fs, io::Write, os::windows::fs::OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH,
    };

    let parent = relative.rsplit_once('/').map_or("", |(parent, _)| parent);
    let _parent_guard = GuardedPath::open(root, parent, true)?;
    let target = root.join(relative);
    let original = match expected {
        Some(version) => {
            if !target.try_exists().map_err(|error| error.to_string())? {
                return Ok(SaveResult::conflict(
                    "文件已删除或移动，请重载或另存为新文件。",
                ));
            }
            let guard = GuardedPath::open(root, relative, false)?;
            if read_guarded_text(&guard, relative)?.version != version {
                return Ok(SaveResult::conflict(
                    "文件已被外部修改，本次未写入。请比较或重载后再保存。",
                ));
            }
            if fs::metadata(&target)
                .map_err(|error| error.to_string())?
                .permissions()
                .readonly()
            {
                return Err("文件为只读，本次未写入。".into());
            }
            Some(guard)
        }
        None => {
            if fs::symlink_metadata(&target).is_ok() {
                return Ok(SaveResult::conflict(
                    "另存目标已存在，本次未覆盖。请选择新文件名。",
                ));
            }
            None
        }
    };
    let id = uuid::Uuid::new_v4();
    let join_name = |suffix| {
        if parent.is_empty() {
            format!(".deeppi-{id}.{suffix}")
        } else {
            format!("{parent}/.deeppi-{id}.{suffix}")
        }
    };
    let pending_relative = join_name("pending");
    let recovery_relative = join_name("recovery");
    register(
        &pending_relative,
        expected.map(|_| recovery_relative.as_str()),
    )?;
    let temporary = root.join(&pending_relative);
    let recovery = root.join(&recovery_relative);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(&temporary)
        .map_err(|error| format!("无法创建保存临时文件：{error}"))?;
    let written = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all());
    drop(file);
    let mut pending = PendingFile {
        path: temporary.clone(),
        preserve: false,
    };
    written.map_err(|error| format!("无法写入保存临时文件：{error}"))?;
    let candidate = GuardedPath::open(root, &pending_relative, false)?;
    let new_version = candidate.version(relative, content.as_bytes())?;
    drop(candidate);
    let _original_read_lock = original
        .as_ref()
        .map(GuardedPath::retain_read_lock_for_replace)
        .transpose()?;
    drop(original);
    before_publish();
    let source_name = wide_path(&temporary)?;
    let target_name = wide_path(&target)?;
    if expected.is_none() {
        // No REPLACE_EXISTING flag: a late creator must win without being overwritten.
        let published = unsafe {
            MoveFileExW(
                source_name.as_ptr(),
                target_name.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        };
        if published == 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(80 | 183)) {
                return Ok(SaveResult::conflict(
                    "另存目标刚被其他程序创建，本次未覆盖。",
                ));
            }
            return Err(format!("无法发布新文件，本次未覆盖现有文件：{error}"));
        }
        pending.preserve = true;
        if !read_text(root, relative).is_ok_and(|current| current.version == new_version) {
            return Ok(SaveResult {
                outcome: SaveOutcome::Unknown,
                version: None,
                recovery_path: None,
                pending_path: None,
                detail:
                    "新文件已发布，但内容或身份核验不一致。请保留草稿并检查目标文件，不要直接重试。"
                        .into(),
            });
        }
        return Ok(SaveResult {
            outcome: SaveOutcome::Saved,
            version: Some(new_version),
            recovery_path: None,
            pending_path: None,
            detail: "已另存为新文件。".into(),
        });
    }
    let recovery_name = wide_path(&recovery)?;
    // ReplaceFile can partially succeed. Preserve both possible recovery paths from this point.
    pending.preserve = true;
    let replaced = unsafe {
        ReplaceFileW(
            target_name.as_ptr(),
            source_name.as_ptr(),
            recovery_name.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if replaced == 0 {
        return Ok(SaveResult {
            outcome: SaveOutcome::Unknown, version: None,
            recovery_path: Some(recovery_relative), pending_path: Some(pending_relative),
            detail: format!("替换结果未确认：{}。目标或恢复路径可能已变化，请先核对；未自动重试或删除恢复文件。", std::io::Error::last_os_error()),
        });
    }
    // The backup is the file actually replaced, not a stale preflight copy.
    let replaced_version = GuardedPath::open(root, &recovery_relative, false)
        .and_then(|guard| read_guarded_text(&guard, relative).map(|backup| backup.version));
    match replaced_version {
        Ok(version) if Some(version.as_str()) == expected => {
            if !read_text(root, relative).is_ok_and(|current| current.version == new_version) {
                return Ok(SaveResult { outcome: SaveOutcome::Unknown, version: None,
                    recovery_path: Some(recovery_relative), pending_path: None,
                    detail: "文件已替换，但新内容或身份核验不一致。旧版本副本已保留，请比较后处理。".into() });
            }
            Ok(SaveResult { outcome: SaveOutcome::Saved, version: Some(new_version),
                recovery_path: Some(recovery_relative),
                pending_path: None,
                detail: "已保存，旧版本保留在恢复副本中。".into() })
        }
        Ok(_) => Ok(SaveResult { outcome: SaveOutcome::Conflict, version: None,
            recovery_path: Some(recovery_relative), pending_path: None,
            detail: "替换期间检测到外部修改：目标可能已是本次内容，外部版本已保留在恢复副本。请比较后处理，不要直接重试。".into() }),
        Err(error) => Ok(SaveResult { outcome: SaveOutcome::Unknown, version: None,
            recovery_path: Some(recovery_relative), pending_path: None,
            detail: format!("文件已替换，但无法核验旧版本：{error}。已保留恢复副本，请先核对。") }),
    }
}

#[cfg(windows)]
struct PendingFile {
    path: std::path::PathBuf,
    preserve: bool,
}

#[cfg(windows)]
impl Drop for PendingFile {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Result<Vec<u16>, String> {
    use std::os::windows::ffi::OsStrExt;
    // absolute() normalizes the path; the parent is already pinned and rejects reparse points.
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let mut wide: Vec<_> = absolute.as_os_str().encode_wide().collect();
    if !wide.starts_with(&[92, 92, 63, 92]) {
        if wide.starts_with(&[92, 92]) {
            let mut prefix: Vec<_> = "\\\\?\\UNC\\".encode_utf16().collect();
            prefix.extend_from_slice(&wide[2..]);
            wide = prefix;
        } else {
            let mut prefix: Vec<_> = "\\\\?\\".encode_utf16().collect();
            prefix.extend(wide);
            wide = prefix;
        }
    }
    wide.push(0);
    Ok(wide)
}

#[tauri::command]
pub async fn save_project_file(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    gate: State<'_, FileEditGate>,
    project_id: String,
    relative_path: String,
    content: String,
    expected_version: Option<String>,
) -> Result<SaveResult, String> {
    require_main(&webview)?;
    validate_save(&relative_path, &content, expected_version.as_deref())?;
    let root = project_root(&store, &project_id)?;
    let gate = gate.0.clone();
    let recovery = webview
        .app_handle()
        .state::<crate::file_recovery::RecoveryStore>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = gate
            .try_lock()
            .map_err(|_| "另一文件正在保存，请稍后重试。")?;
        save_registered(
            &root,
            &relative_path,
            &content,
            expected_version.as_deref(),
            || {},
            |pending, old| recovery.begin(&root, &project_id, &relative_path, pending, old),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::project_files::read_text;
    use std::{fs, path::PathBuf};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("deeppi-save-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn write(&self, name: &str, content: &str) -> String {
            fs::write(self.0.join(name), content).unwrap();
            read_text(&self.0, name).unwrap().version
        }
        fn save(&self, name: &str, content: &str, expected: Option<&str>) -> SaveResult {
            save_text(&self.0, name, content, expected, || {}).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn saves_exact_utf8_bytes_and_returns_the_published_file_version() {
        let fixture = Fixture::new();
        let old = fixture.write("中文 name.txt", "before");
        let content = "\u{feff}第一行\r\nsecond\nlast\r";
        let saved = fixture.save("中文 name.txt", content, Some(&old));
        assert_eq!(saved.outcome, SaveOutcome::Saved);
        let current = read_text(&fixture.0, "中文 name.txt").unwrap();
        assert_eq!(current.content, content);
        assert_eq!(saved.version.as_deref(), Some(current.version.as_str()));
        assert_ne!(current.version, old);
        assert_eq!(
            fs::read_to_string(fixture.0.join(saved.recovery_path.unwrap())).unwrap(),
            "before"
        );
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
    }

    #[test]
    fn stale_version_does_not_modify_or_backup_the_external_file() {
        let fixture = Fixture::new();
        let old = fixture.write("file.txt", "old");
        fixture.write("file.txt", "external");
        let result = fixture.save("file.txt", "draft", Some(&old));
        assert_eq!(result.outcome, SaveOutcome::Conflict);
        assert!(result.version.is_none());
        assert_eq!(
            fs::read_to_string(fixture.0.join("file.txt")).unwrap(),
            "external"
        );
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[test]
    fn concurrent_replacement_is_reported_and_the_actual_replaced_bytes_are_preserved() {
        let fixture = Fixture::new();
        let old = fixture.write("file.txt", "old");
        let external = fixture.0.join("external.txt");
        fs::write(&external, "external change").unwrap();
        let result = save_text(&fixture.0, "file.txt", "draft", Some(&old), || {
            fs::rename(fixture.0.join("file.txt"), fixture.0.join("old.txt")).unwrap();
            fs::rename(&external, fixture.0.join("file.txt")).unwrap();
        })
        .unwrap();
        assert_eq!(result.outcome, SaveOutcome::Conflict);
        assert!(result.version.is_none());
        let recovery = result.recovery_path.unwrap();
        assert_eq!(
            fs::read_to_string(fixture.0.join(recovery)).unwrap(),
            "external change"
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("file.txt")).unwrap(),
            "draft"
        );
    }

    #[test]
    fn save_as_is_create_only_even_when_another_writer_creates_the_destination() {
        let fixture = Fixture::new();
        let result = save_text(&fixture.0, "new.txt", "draft", None, || {
            fs::write(fixture.0.join("new.txt"), "external").unwrap();
        })
        .unwrap();
        assert_eq!(result.outcome, SaveOutcome::Conflict);
        assert_eq!(
            fs::read_to_string(fixture.0.join("new.txt")).unwrap(),
            "external"
        );
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[test]
    fn save_as_creates_only_the_selected_file() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("src")).unwrap();
        let saved = fixture.save("src/new.txt", "new\r\n", None);
        assert_eq!(saved.outcome, SaveOutcome::Saved);
        let current = read_text(&fixture.0, "src/new.txt").unwrap();
        assert_eq!(saved.version, Some(current.version));
        assert_eq!(current.content, "new\r\n");
        assert_eq!(fs::read_dir(fixture.0.join("src")).unwrap().count(), 1);
        assert_eq!(
            fixture.save("src/new.txt", "overwrite", None).outcome,
            SaveOutcome::Conflict
        );
    }

    #[test]
    fn refuses_to_report_success_when_the_temporary_bytes_change_before_publication() {
        for existing in [false, true] {
            let fixture = Fixture::new();
            let old = existing.then(|| fixture.write("file.txt", "old"));
            let result = save_text(&fixture.0, "file.txt", "draft", old.as_deref(), || {
                let pending = fs::read_dir(&fixture.0)
                    .unwrap()
                    .filter_map(Result::ok)
                    .find(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "pending")
                    })
                    .unwrap()
                    .path();
                fs::write(pending, "unexpected bytes").unwrap();
            })
            .unwrap();
            assert_eq!(result.outcome, SaveOutcome::Unknown);
            assert!(result.version.is_none());
            assert_eq!(
                fs::read_to_string(fixture.0.join("file.txt")).unwrap(),
                "unexpected bytes"
            );
            if existing {
                assert_eq!(
                    fs::read_to_string(fixture.0.join(result.recovery_path.unwrap())).unwrap(),
                    "old"
                );
            }
        }
    }

    #[test]
    fn deleted_file_is_not_recreated_and_a_late_deletion_keeps_the_draft() {
        let fixture = Fixture::new();
        let old = fixture.write("file.txt", "old");
        fs::remove_file(fixture.0.join("file.txt")).unwrap();
        assert_eq!(
            fixture.save("file.txt", "draft", Some(&old)).outcome,
            SaveOutcome::Conflict
        );
        assert!(!fixture.0.join("file.txt").exists());
        let old = fixture.write("file.txt", "old");
        let result = save_text(&fixture.0, "file.txt", "draft", Some(&old), || {
            fs::rename(fixture.0.join("file.txt"), fixture.0.join("moved.txt")).unwrap();
        })
        .unwrap();
        assert_eq!(result.outcome, SaveOutcome::Unknown);
        assert_eq!(
            fs::read_to_string(fixture.0.join(result.pending_path.unwrap())).unwrap(),
            "draft"
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("moved.txt")).unwrap(),
            "old"
        );
        assert!(!fixture.0.join("file.txt").exists());
    }

    #[test]
    fn refuses_read_only_files_without_changing_them() {
        let fixture = Fixture::new();
        let old = fixture.write("file.txt", "old");
        let target = fixture.0.join("file.txt");
        let permissions = fs::metadata(&target).unwrap().permissions();
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&target, read_only).unwrap();
        let result = save_text(&fixture.0, "file.txt", "draft", Some(&old), || {});
        fs::set_permissions(&target, permissions).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "old");
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[test]
    fn pins_parents_and_denies_in_place_writers_through_the_publish_boundary() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("src")).unwrap();
        let old = fixture.write("src/file.txt", "old");
        let result = save_text(&fixture.0, "src/file.txt", "draft", Some(&old), || {
            assert!(fs::write(fixture.0.join("src/file.txt"), "external").is_err());
            assert!(fs::rename(fixture.0.join("src"), fixture.0.join("moved")).is_err());
        })
        .unwrap();
        assert_eq!(result.outcome, SaveOutcome::Saved);
    }

    #[test]
    fn rejects_a_junction_parent_without_writing_outside_the_project() {
        let fixture = Fixture::new();
        let outside = Fixture::new();
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(fixture.0.join("link"))
            .arg(&outside.0)
            .output()
            .unwrap();
        assert!(output.status.success());
        let result = save_text(&fixture.0, "link/file.txt", "draft", None, || {});
        fs::remove_dir(fixture.0.join("link")).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
    }

    #[test]
    fn rejects_invalid_paths_versions_binary_content_and_oversized_drafts() {
        let fixture = Fixture::new();
        for path in [
            "../outside",
            ".git/config",
            "C:/outside",
            "a\\b",
            "file.txt:stream",
            "NUL",
        ] {
            assert!(
                save_text(&fixture.0, path, "text", None, || {}).is_err(),
                "{path}"
            );
        }
        assert!(save_text(&fixture.0, "file", "a\0b", None, || {}).is_err());
        assert!(save_text(
            &fixture.0,
            "file",
            &"a".repeat(2 * 1024 * 1024 + 1),
            None,
            || {}
        )
        .is_err());
        assert!(save_text(&fixture.0, "file", "text", Some("not-a-version"), || {}).is_err());
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
    }
}
