#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::project_files::read_text;
    use std::{fs, path::PathBuf};

    struct Fixture {
        base: PathBuf,
        root: PathBuf,
        database: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let base =
                std::env::temp_dir().join(format!("deeppi-recovery-{}", uuid::Uuid::new_v4()));
            let root = base.join("project");
            fs::create_dir_all(&root).unwrap();
            Self {
                database: base.join("recovery.db"),
                base,
                root,
            }
        }
        fn record(&self, store: &RecoveryStore) -> (String, String) {
            let id = uuid::Uuid::new_v4().to_string();
            let pending = format!(".deeppi-{id}.pending");
            let recovery = format!(".deeppi-{id}.recovery");
            store
                .begin(&self.root, "p", "file.txt", &pending, Some(&recovery))
                .unwrap();
            fs::write(self.root.join(&recovery), "old content\r\n").unwrap();
            (pending, recovery)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.base).unwrap();
        }
    }

    #[test]
    fn persisted_intents_survive_reopening_without_scanning_or_adopting_untracked_files() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        let (_, recovery) = fixture.record(&store);
        fs::write(
            fixture.root.join(".deeppi-unregistered.recovery"),
            "unrelated",
        )
        .unwrap();
        drop(store);
        let reopened = RecoveryStore::open(&fixture.database).unwrap();
        let listed = reopened.list(&fixture.root, "p", 0).unwrap();
        assert_eq!(listed.items.len(), 2);
        assert!(listed
            .items
            .iter()
            .any(|item| item.path == recovery && item.status == "available"));
        assert!(listed.items.iter().any(|item| item.status == "missing"));
        assert!(reopened
            .list(&fixture.root, "other-project", 0)
            .unwrap()
            .items
            .is_empty());
    }

    #[test]
    fn exact_version_deletion_keeps_working_file_and_other_backups() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        let (_, recovery) = fixture.record(&store);
        let (_, other) = fixture.record(&store);
        fs::write(fixture.root.join("file.txt"), "working draft").unwrap();
        let item = store
            .list(&fixture.root, "p", 0)
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.path == recovery)
            .unwrap();
        store
            .delete(
                &fixture.root,
                "p",
                &item.id,
                item.version.as_deref(),
                || {
                    assert!(fs::write(fixture.root.join(&recovery), "late").is_err());
                    assert!(
                        fs::rename(fixture.root.join(&recovery), fixture.root.join("moved"))
                            .is_err()
                    );
                },
            )
            .unwrap();
        assert!(!fixture.root.join(&recovery).exists());
        assert!(fixture.root.join(other).exists());
        assert_eq!(
            fs::read_to_string(fixture.root.join("file.txt")).unwrap(),
            "working draft"
        );
    }

    #[test]
    fn rejects_stale_versions_and_cross_project_record_ids() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        let (_, recovery) = fixture.record(&store);
        let item = store
            .list(&fixture.root, "p", 0)
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.path == recovery)
            .unwrap();
        fs::write(fixture.root.join(&recovery), "external changed copy").unwrap();
        assert!(store
            .delete(&fixture.root, "p", &item.id, item.version.as_deref(), || {})
            .is_err());
        let version = read_text(&fixture.root, &recovery).unwrap().version;
        assert!(store
            .delete(&fixture.root, "other", &item.id, Some(&version), || {})
            .is_err());
        assert_eq!(
            fs::read_to_string(fixture.root.join(recovery)).unwrap(),
            "external changed copy"
        );
    }

    #[test]
    fn replacement_of_the_project_root_invalidates_old_recovery_records() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        let (_, recovery) = fixture.record(&store);
        let item = store
            .list(&fixture.root, "p", 0)
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.path == recovery)
            .unwrap();
        fs::rename(&fixture.root, fixture.base.join("old-project")).unwrap();
        fs::create_dir(&fixture.root).unwrap();
        fs::write(fixture.root.join(&recovery), "different project").unwrap();
        let list = store.list(&fixture.root, "p", 0).unwrap();
        assert!(list
            .items
            .iter()
            .all(|item| item.status == "projectChanged"));
        assert!(store
            .delete(&fixture.root, "p", &item.id, item.version.as_deref(), || {})
            .is_err());
        assert!(fixture.root.join(recovery).exists());
    }

    #[test]
    fn forgetting_a_missing_entry_never_deletes_a_newly_created_file() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        let (pending, _) = fixture.record(&store);
        let item = store
            .list(&fixture.root, "p", 0)
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.path == pending)
            .unwrap();
        store
            .delete(&fixture.root, "p", &item.id, None, || {
                fs::write(fixture.root.join(&pending), "new file").unwrap();
            })
            .unwrap();
        assert_eq!(
            fs::read_to_string(fixture.root.join(pending)).unwrap(),
            "new file"
        );
    }

    #[test]
    fn refuses_arbitrary_paths_and_unregistered_file_names() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        assert!(store
            .begin(&fixture.root, "p", "file.txt", "../outside", None)
            .is_err());
        assert!(store
            .begin(&fixture.root, "p", "file.txt", "ordinary.txt", None)
            .is_err());
        let id = uuid::Uuid::new_v4();
        assert!(store
            .begin(
                &fixture.root,
                "p",
                ".git/config",
                &format!(".deeppi-{id}.pending"),
                None
            )
            .is_err());
        assert_eq!(store.list(&fixture.root, "p", 0).unwrap().total, 0);
    }

    #[test]
    fn saves_register_before_mutating_files_and_refuse_a_failed_journal() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        fs::write(fixture.root.join("file.txt"), "old").unwrap();
        let expected = read_text(&fixture.root, "file.txt").unwrap().version;
        let failed = crate::project_edit::save_registered(
            &fixture.root,
            "file.txt",
            "new",
            Some(&expected),
            || {},
            |_, _| Err("journal unavailable".into()),
        );
        assert!(failed.is_err());
        assert_eq!(
            fs::read_to_string(fixture.root.join("file.txt")).unwrap(),
            "old"
        );
        assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 1);
        crate::project_edit::save_registered(
            &fixture.root,
            "file.txt",
            "new",
            Some(&expected),
            || {},
            |pending, recovery| store.begin(&fixture.root, "p", "file.txt", pending, recovery),
        )
        .unwrap();
        drop(store);
        let reopened = RecoveryStore::open(&fixture.database).unwrap();
        assert_eq!(reopened.list(&fixture.root, "p", 0).unwrap().total, 2);
    }

    #[test]
    fn restoring_creates_a_new_file_and_preserves_the_source_and_existing_destination() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        let (_, recovery) = fixture.record(&store);
        let item = store
            .list(&fixture.root, "p", 0)
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.path == recovery)
            .unwrap();
        let source = store
            .source(
                &fixture.root,
                "p",
                &item.id,
                item.version.as_deref().unwrap(),
            )
            .unwrap();
        let save = || {
            crate::project_edit::save_registered(
                &fixture.root,
                "restored.txt",
                &source.content,
                None,
                || {},
                |pending, old| store.begin(&fixture.root, "p", "restored.txt", pending, old),
            )
        };
        assert_eq!(
            save().unwrap().outcome,
            crate::project_edit::SaveOutcome::Saved
        );
        assert_eq!(
            save().unwrap().outcome,
            crate::project_edit::SaveOutcome::Conflict
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("restored.txt")).unwrap(),
            "old content\r\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join(recovery)).unwrap(),
            "old content\r\n"
        );
    }

    #[test]
    fn bounded_pagination_returns_all_registered_entries_without_duplicates() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        for _ in 0..15 {
            fixture.record(&store);
        }
        let first = store.list(&fixture.root, "p", 0).unwrap();
        assert_eq!(first.items.len(), 25);
        let second = store
            .list(&fixture.root, "p", first.next_offset.unwrap())
            .unwrap();
        assert_eq!(second.items.len(), 5);
        assert!(second.next_offset.is_none());
        assert!(second
            .items
            .iter()
            .all(|item| !first.items.iter().any(|first| first.id == item.id)));
    }

    #[test]
    fn replacing_a_nested_parent_does_not_adopt_its_new_files() {
        let fixture = Fixture::new();
        let store = RecoveryStore::open(&fixture.database).unwrap();
        fs::create_dir(fixture.root.join("src")).unwrap();
        let id = uuid::Uuid::new_v4();
        let path = format!("src/.deeppi-{id}.pending");
        store
            .begin(&fixture.root, "p", "src/file.txt", &path, None)
            .unwrap();
        fs::rename(fixture.root.join("src"), fixture.root.join("old")).unwrap();
        fs::create_dir(fixture.root.join("src")).unwrap();
        fs::write(fixture.root.join(&path), "unrelated").unwrap();
        let entry = store.list(&fixture.root, "p", 0).unwrap().items.remove(0);
        assert_eq!(entry.status, "projectChanged");
        assert!(entry.version.is_none());
    }
}
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use crate::message::{msg, msg_with};
use crate::{
    project_edit::FileEditGate,
    project_files::{
        project_root, read_guarded_text, require_main, validate_relative, GuardedPath,
    },
    task::TaskStore,
};

#[derive(Clone)]
pub struct RecoveryStore(Arc<Mutex<Connection>>);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryItem {
    pub id: String,
    pub target: String,
    pub path: String,
    pub kind: String,
    pub created_at: i64,
    pub status: String,
    pub version: Option<String>,
    pub size: Option<u64>,
    pub detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryList {
    pub items: Vec<RecoveryItem>,
    pub total: u64,
    pub next_offset: Option<u64>,
}

#[derive(Clone)]
struct Record {
    item: RecoveryItem,
    root_version: String,
}

fn record(row: &rusqlite::Row<'_>) -> rusqlite::Result<Record> {
    Ok(Record {
        item: RecoveryItem {
            id: row.get(0)?,
            target: row.get(1)?,
            path: row.get(2)?,
            kind: row.get(3)?,
            created_at: row.get(4)?,
            status: "unavailable".into(),
            version: None,
            size: None,
            detail: String::new(),
        },
        root_version: row.get(5)?,
    })
}

fn scope_guard(root: &Path, target: &str) -> Result<(GuardedPath, String), String> {
    validate_relative(target, false)?;
    let parent = target.rsplit_once('/').map_or("", |(parent, _)| parent);
    let guard = GuardedPath::open(root, parent, true)?;
    let version = guard.version(parent, b"")?;
    Ok((guard, version))
}

fn validate_record(target: &str, path: &str, kind: &str) -> Result<String, String> {
    validate_relative(target, false)?;
    validate_relative(path, false)?;
    if !matches!(kind, "pending" | "recovery")
        || target.rsplit_once('/').map(|(parent, _)| parent)
            != path.rsplit_once('/').map(|(parent, _)| parent)
    {
        return Err(msg("recovery.record_invalid"));
    }
    let name = path.rsplit('/').next().unwrap_or("");
    let id = name
        .strip_prefix(".deeppi-")
        .and_then(|name| name.strip_suffix(&format!(".{kind}")))
        .ok_or(msg("recovery.file_name_invalid"))?;
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| msg("recovery.id_invalid"))?;
    if uuid.to_string() != id {
        return Err(msg("recovery.id_format_invalid"));
    }
    Ok(format!("{id}:{kind}"))
}

impl RecoveryStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        crate::snapshot::reject_link(path)?;
        let connection = Connection::open(path).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS file_recovery (
               id TEXT PRIMARY KEY, project_id TEXT NOT NULL, root_version TEXT NOT NULL,
               target TEXT NOT NULL, path TEXT NOT NULL, kind TEXT NOT NULL, created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS file_recovery_project ON file_recovery(project_id, created_at, id);"
        ).map_err(|error| error.to_string())?;
        Ok(Self(Arc::new(Mutex::new(connection))))
    }

    pub(crate) fn begin(
        &self,
        root: &Path,
        project: &str,
        target: &str,
        pending: &str,
        recovery: Option<&str>,
    ) -> Result<(), String> {
        let mut paths = vec![(pending, "pending")];
        if let Some(path) = recovery {
            paths.push((path, "recovery"));
        }
        for (path, kind) in &paths {
            validate_record(target, path, kind)?;
        }
        let (_scope, identity) = scope_guard(root, target)?;
        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis() as i64;
        let mut connection = self
            .0
            .lock()
            .map_err(|_| msg("recovery.lock_unavailable"))?;
        let tx = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let count: i64 = tx
            .query_row("SELECT COUNT(*) FROM file_recovery", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if count + paths.len() as i64 > 20_000 {
            return Err(msg("recovery.limit_reached"));
        }
        for (path, kind) in paths {
            let id = validate_record(target, path, kind)?;
            tx.execute("INSERT INTO file_recovery(id,project_id,root_version,target,path,kind,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![id, project, identity, target, path, kind, created]).map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())
    }

    fn lookup(&self, project: &str, id: &str) -> Result<Record, String> {
        let connection = self
            .0
            .lock()
            .map_err(|_| msg("recovery.lock_unavailable"))?;
        let found = connection.query_row(
            "SELECT id,target,path,kind,created_at,root_version FROM file_recovery WHERE project_id=?1 AND id=?2",
            params![project, id], record).optional().map_err(|error| error.to_string())?
            .ok_or(msg("recovery.record_missing"))?;
        if validate_record(&found.item.target, &found.item.path, &found.item.kind)? != found.item.id
        {
            return Err(msg("recovery.id_mismatch"));
        }
        Ok(found)
    }

    pub(crate) fn list(
        &self,
        root: &Path,
        project: &str,
        offset: u64,
    ) -> Result<RecoveryList, String> {
        if offset > 20_000 {
            return Err(msg("recovery.pagination_invalid"));
        }
        let _root = GuardedPath::open(root, "", true)?;
        let connection = self
            .0
            .lock()
            .map_err(|_| msg("recovery.lock_unavailable"))?;
        let total = connection
            .query_row(
                "SELECT COUNT(*) FROM file_recovery WHERE project_id=?1",
                [project],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())? as u64;
        let mut statement = connection.prepare(
            "SELECT id,target,path,kind,created_at,root_version FROM file_recovery WHERE project_id=?1 ORDER BY created_at DESC,id DESC LIMIT 25 OFFSET ?2"
        ).map_err(|error| error.to_string())?;
        let records = statement
            .query_map(params![project, offset as i64], record)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        drop(connection);
        let mut items = Vec::with_capacity(records.len());
        for found in records {
            let mut item = found.item;
            let scoped = scope_guard(root, &item.target);
            if scoped.as_ref().map(|(_, version)| version).ok() != Some(&found.root_version) {
                item.status = "projectChanged".into();
                item.detail = msg("recovery.detail_project_changed");
            } else if validate_record(&item.target, &item.path, &item.kind)
                .ok()
                .as_deref()
                != Some(&item.id)
            {
                item.detail = msg("recovery.detail_record_invalid");
            } else {
                let parent = item.path.rsplit_once('/').map_or("", |(parent, _)| parent);
                let inspected = GuardedPath::open(root, parent, true).and_then(|_parent| {
                    match fs::symlink_metadata(root.join(&item.path)) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                        Err(error) => Err(error.to_string()),
                        Ok(_) => {
                            let guard = GuardedPath::open(root, &item.path, false)?;
                            read_guarded_text(&guard, &item.path).map(Some)
                        }
                    }
                });
                match inspected {
                    Ok(Some(preview)) => {
                        item.status = "available".into();
                        item.size = Some(preview.size);
                        item.version = Some(preview.version);
                    }
                    Ok(None) => {
                        item.status = "missing".into();
                        item.detail = msg("recovery.detail_copy_missing");
                    }
                    Err(error) => {
                        item.detail = error;
                    }
                }
            }
            items.push(item);
        }
        let next = offset + items.len() as u64;
        Ok(RecoveryList {
            items,
            total,
            next_offset: (next < total).then_some(next),
        })
    }

    pub(crate) fn source(
        &self,
        root: &Path,
        project: &str,
        id: &str,
        expected: &str,
    ) -> Result<crate::project_files::FilePreview, String> {
        let found = self.lookup(project, id)?;
        let (_scope, version) = scope_guard(root, &found.item.target)?;
        if version != found.root_version {
            return Err(msg("recovery.scope_changed"));
        }
        let guard = GuardedPath::open(root, &found.item.path, false)?;
        let preview = read_guarded_text(&guard, &found.item.path)?;
        if preview.version != expected {
            return Err(msg("recovery.copy_changed"));
        }
        Ok(preview)
    }

    pub(crate) fn delete(
        &self,
        root: &Path,
        project: &str,
        id: &str,
        expected: Option<&str>,
        when_locked: impl FnOnce(),
    ) -> Result<(), String> {
        let found = self.lookup(project, id)?;
        if let Some(version) = expected {
            let (_scope, scope_version) = scope_guard(root, &found.item.target)?;
            if scope_version != found.root_version {
                return Err(msg("recovery.scope_changed"));
            }
            let guard = GuardedPath::open_for_delete(root, &found.item.path)?;
            if read_guarded_text(&guard, &found.item.path)?.version != version {
                return Err(msg("recovery.copy_changed_no_delete"));
            }
            when_locked();
            guard.delete()?;
        } else {
            when_locked();
        }
        self.0
            .lock()
            .map_err(|_| msg("recovery.cleanup_failed"))?
            .execute(
                "DELETE FROM file_recovery WHERE project_id=?1 AND id=?2",
                params![project, id],
            )
            .map_err(|error| {
                msg_with("recovery.cleanup_error", &[("detail", &error.to_string())])
            })?;
        Ok(())
    }
}

#[tauri::command]
pub async fn list_project_recoveries(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    recovery: State<'_, RecoveryStore>,
    project_id: String,
    offset: u64,
) -> Result<RecoveryList, String> {
    require_main(&webview)?;
    let root = project_root(&store, &project_id)?;
    let recovery = recovery.inner().clone();
    tauri::async_runtime::spawn_blocking(move || recovery.list(&root, &project_id, offset))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn read_project_recovery(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    recovery: State<'_, RecoveryStore>,
    project_id: String,
    record_id: String,
    expected_version: String,
) -> Result<crate::project_files::FilePreview, String> {
    require_main(&webview)?;
    let root = project_root(&store, &project_id)?;
    let recovery = recovery.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        recovery.source(&root, &project_id, &record_id, &expected_version)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 前端传入的两分支确认文案（前端掌握当前语言，后端不持有任何文案）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteDialogs {
    pub permanent: crate::dialog_text::ConfirmDialog,
    pub record_only: crate::dialog_text::ConfirmDialog,
}

impl DeleteDialogs {
    /// 两条分支文案都要通过与单条对话框相同的长度与字段校验。
    fn validate(&self) -> Result<(), String> {
        self.permanent.validate()?;
        self.record_only.validate()
    }
}

/// 按前端传入的 `expected_version` 挑选删除确认文案：`Some` 走「永久删除副本」，
/// `None` 走「仅移除记录」——与随后 `delete()` 里的版本校验同源。
/// 副本已消失等不一致情况会在删除阶段由版本校验拒绝，不会误删。
fn delete_dialog<'a>(
    dialogs: &'a DeleteDialogs,
    expected_version: Option<&str>,
) -> &'a crate::dialog_text::ConfirmDialog {
    if expected_version.is_some() {
        &dialogs.permanent
    } else {
        &dialogs.record_only
    }
}

#[tauri::command]
// 参数与 Tauri 命令签名一一对应，不再拆分。
#[allow(clippy::too_many_arguments)]
pub async fn delete_project_recovery(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    recovery: State<'_, RecoveryStore>,
    gate: State<'_, FileEditGate>,
    project_id: String,
    record_id: String,
    expected_version: Option<String>,
    dialogs: DeleteDialogs,
) -> Result<bool, String> {
    require_main(&webview)?;
    dialogs.validate()?;
    let root = project_root(&store, &project_id)?;
    let app = webview.app_handle().clone();
    let recovery = recovery.inner().clone();
    let gate = gate.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = gate.try_lock().map_err(|_| msg("recovery.busy"))?;
        let found = recovery.lookup(&project_id, &record_id)?;
        let dialog = delete_dialog(&dialogs, expected_version.as_deref());
        if !app
            .dialog()
            .message(dialog.render(&[
                ("project", root.display().to_string()),
                ("copy", found.item.path.clone()),
                ("target", found.item.target.clone()),
            ]))
            .title(dialog.title.clone())
            .buttons(MessageDialogButtons::OkCancelCustom(
                dialog.confirm_label.clone(),
                dialog.cancel_label.clone(),
            ))
            .blocking_show()
        {
            return Ok(false);
        }
        recovery.delete(
            &root,
            &project_id,
            &record_id,
            expected_version.as_deref(),
            || {},
        )?;
        Ok(true)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRequest {
    record_id: String,
    expected_version: String,
    relative_path: String,
}

#[tauri::command]
pub async fn restore_project_recovery(
    webview: tauri::Webview,
    store: State<'_, TaskStore>,
    recovery: State<'_, RecoveryStore>,
    gate: State<'_, FileEditGate>,
    project_id: String,
    request: RestoreRequest,
    dialog: crate::dialog_text::ConfirmDialog,
) -> Result<Option<crate::project_edit::SaveResult>, String> {
    require_main(&webview)?;
    validate_relative(&request.relative_path, false)?;
    dialog.validate()?;
    let root = project_root(&store, &project_id)?;
    let app = webview.app_handle().clone();
    let recovery = recovery.inner().clone();
    let gate = gate.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = gate.try_lock().map_err(|_| msg("recovery.busy"))?;
        let found = recovery.lookup(&project_id, &request.record_id)?;
        if !app
            .dialog()
            .message(dialog.render(&[
                ("project", root.display().to_string()),
                ("copy", found.item.path.clone()),
                ("new_path", request.relative_path.clone()),
            ]))
            .title(dialog.title.clone())
            .buttons(MessageDialogButtons::OkCancelCustom(
                dialog.confirm_label.clone(),
                dialog.cancel_label.clone(),
            ))
            .blocking_show()
        {
            return Ok(None);
        }
        let _root = GuardedPath::open(&root, "", true)?;
        let preview = recovery.source(
            &root,
            &project_id,
            &request.record_id,
            &request.expected_version,
        )?;
        let result = crate::project_edit::save_registered(
            &root,
            &request.relative_path,
            &preview.content,
            None,
            || {},
            |pending, old| recovery.begin(&root, &project_id, &request.relative_path, pending, old),
        )?;
        Ok(Some(result))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod dialog_tests {
    use super::*;
    use crate::dialog_text::ConfirmDialog;
    use std::collections::BTreeMap;

    fn dialog(message: &str) -> ConfirmDialog {
        ConfirmDialog {
            title: "Confirm".into(),
            message: message.into(),
            confirm_label: "Yes".into(),
            cancel_label: "No".into(),
            terms: BTreeMap::new(),
        }
    }

    fn delete_dialogs() -> DeleteDialogs {
        DeleteDialogs {
            permanent: dialog("permanent project={project} copy={copy} target={target}"),
            record_only: dialog("record project={project} copy={copy} target={target}"),
        }
    }

    #[test]
    fn selects_the_delete_dialog_branch_by_expected_version() {
        let dialogs = delete_dialogs();
        assert!(std::ptr::eq(
            delete_dialog(&dialogs, Some("version")),
            &dialogs.permanent
        ));
        assert!(std::ptr::eq(
            delete_dialog(&dialogs, None),
            &dialogs.record_only
        ));
        assert!(dialogs.validate().is_ok());
    }

    #[test]
    fn renders_delete_placeholders_from_measured_values() {
        let dialogs = delete_dialogs();
        let values = [
            ("project", "F:/work".to_owned()),
            ("copy", "src/.deeppi-id.recovery".to_owned()),
            ("target", "src/a.txt".to_owned()),
        ];
        assert_eq!(
            delete_dialog(&dialogs, Some("version")).render(&values),
            "permanent project=F:/work copy=src/.deeppi-id.recovery target=src/a.txt"
        );
        assert_eq!(
            delete_dialog(&dialogs, None).render(&values),
            "record project=F:/work copy=src/.deeppi-id.recovery target=src/a.txt"
        );
    }

    #[test]
    fn renders_the_restore_placeholder_from_measured_values() {
        let dialog = dialog("restore project={project} copy={copy} new={new_path}");
        assert_eq!(
            dialog.render(&[
                ("project", "F:/work".to_owned()),
                ("copy", "src/.deeppi-id.recovery".to_owned()),
                ("new_path", "src/a.txt.restored".to_owned()),
            ]),
            "restore project=F:/work copy=src/.deeppi-id.recovery new=src/a.txt.restored"
        );
    }

    #[test]
    fn deserializes_the_frontend_delete_payload() {
        let payload = r#"{
            "permanent":{"title":"Delete copy","message":"{target}","confirmLabel":"Delete","cancelLabel":"Cancel"},
            "recordOnly":{"title":"Remove record","message":"{copy}","confirmLabel":"Remove","cancelLabel":"Cancel"}
        }"#;
        let dialogs: DeleteDialogs = serde_json::from_str(payload).unwrap();
        assert!(dialogs.validate().is_ok());
        assert_eq!(delete_dialog(&dialogs, None).title, "Remove record");
        assert_eq!(delete_dialog(&dialogs, Some("v")).title, "Delete copy");
        // 任一分支缺失都是前端契约错误。
        assert!(serde_json::from_str::<DeleteDialogs>(
            r#"{"permanent":{"title":"a","message":"b","confirmLabel":"c","cancelLabel":"d"}}"#
        )
        .is_err());
    }
}
