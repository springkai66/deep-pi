use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::message::msg;

const MAX_TEXT_LENGTH: usize = 500;
const DEFAULT_QUADRANT: i64 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub id: String,
    pub text: String,
    pub quadrant: i64,
    pub completed: bool,
    pub created_at: i64,
}

pub struct ChecklistStore {
    connection: Mutex<Connection>,
}

impl ChecklistStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create checklist directory: {error}"))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| format!("failed to open checklist database: {error}"))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| format!("failed to configure checklist database: {error}"))?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| format!("failed to enable checklist WAL: {error}"))?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.migrate()?;
        Ok(store)
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self, String> {
        let store = Self {
            connection: Mutex::new(
                Connection::open_in_memory()
                    .map_err(|error| format!("failed to open checklist database: {error}"))?,
            ),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "checklist database lock is poisoned".to_string())?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS checklist_items (
                    id TEXT PRIMARY KEY NOT NULL,
                    text TEXT NOT NULL,
                    quadrant INTEGER NOT NULL DEFAULT 1 CHECK(quadrant BETWEEN 1 AND 4),
                    completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
                    created_at INTEGER NOT NULL
                );",
            )
            .map_err(|error| format!("failed to migrate checklist database: {error}"))?;

        let has_quadrant = {
            let mut statement = connection
                .prepare("PRAGMA table_info(checklist_items)")
                .map_err(|error| format!("failed to inspect checklist database: {error}"))?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|error| format!("failed to inspect checklist columns: {error}"))?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| format!("failed to read checklist columns: {error}"))?;
            columns.iter().any(|column| column == "quadrant")
        };
        if !has_quadrant {
            connection
                .execute(
                    "ALTER TABLE checklist_items
                     ADD COLUMN quadrant INTEGER NOT NULL DEFAULT 1
                     CHECK(quadrant BETWEEN 1 AND 4)",
                    [],
                )
                .map_err(|error| format!("failed to migrate checklist quadrants: {error}"))?;
        }
        connection
            .execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_checklist_items_order
                    ON checklist_items(quadrant, completed, created_at DESC);",
            )
            .map_err(|error| format!("failed to index checklist database: {error}"))
    }

    pub fn list(&self) -> Result<Vec<ChecklistItem>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "checklist database lock is poisoned".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT id, text, quadrant, completed, created_at
                 FROM checklist_items
                 ORDER BY quadrant ASC, completed ASC, created_at DESC, id DESC",
            )
            .map_err(|error| format!("failed to prepare checklist list: {error}"))?;
        let rows = statement
            .query_map([], checklist_from_row)
            .map_err(|error| format!("failed to query checklist items: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to read checklist items: {error}"))
    }

    pub fn add(&self, text: &str, quadrant: i64) -> Result<ChecklistItem, String> {
        let text = validate_text(text)?;
        let quadrant = validate_quadrant(quadrant)?;
        let item = ChecklistItem {
            id: format!("checklist-{}", Uuid::new_v4()),
            text: text.to_owned(),
            quadrant,
            completed: false,
            created_at: now_millis()?,
        };
        self.connection
            .lock()
            .map_err(|_| "checklist database lock is poisoned".to_string())?
            .execute(
                "INSERT INTO checklist_items (id, text, quadrant, completed, created_at)
                 VALUES (?1, ?2, ?3, 0, ?4)",
                params![item.id, item.text, item.quadrant, item.created_at],
            )
            .map_err(|error| format!("failed to add checklist item: {error}"))?;
        Ok(item)
    }

    pub fn toggle(&self, id: &str) -> Result<ChecklistItem, String> {
        validate_id(id)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "checklist database lock is poisoned".to_string())?;
        let current = connection
            .query_row(
                "SELECT id, text, quadrant, completed, created_at
                 FROM checklist_items WHERE id = ?1",
                [id],
                checklist_from_row,
            )
            .optional()
            .map_err(|error| format!("failed to find checklist item: {error}"))?
            .ok_or_else(|| msg("checklist.item.not_found"))?;
        let completed = !current.completed;
        connection
            .execute(
                "UPDATE checklist_items SET completed = ?1 WHERE id = ?2",
                params![completed, id],
            )
            .map_err(|error| format!("failed to toggle checklist item: {error}"))?;
        Ok(ChecklistItem {
            completed,
            ..current
        })
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        let affected = self
            .connection
            .lock()
            .map_err(|_| "checklist database lock is poisoned".to_string())?
            .execute("DELETE FROM checklist_items WHERE id = ?1", [id])
            .map_err(|error| format!("failed to delete checklist item: {error}"))?;
        if affected == 0 {
            return Err(msg("checklist.item.not_found"));
        }
        Ok(())
    }
}

fn validate_text(text: &str) -> Result<&str, String> {
    let text = text.trim();
    if text.is_empty()
        || text.chars().count() > MAX_TEXT_LENGTH
        || text.chars().any(|character| character.is_control())
    {
        return Err(msg("checklist.text.invalid"));
    }
    Ok(text)
}

fn validate_quadrant(quadrant: i64) -> Result<i64, String> {
    if (1..=4).contains(&quadrant) {
        Ok(quadrant)
    } else {
        Err(msg("checklist.quadrant.invalid"))
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 100 || id.chars().any(|character| character.is_control()) {
        return Err(msg("checklist.item.not_found"));
    }
    Ok(())
}

fn checklist_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChecklistItem> {
    Ok(ChecklistItem {
        id: row.get(0)?,
        text: row.get(1)?,
        quadrant: row.get(2)?,
        completed: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
    })
}

fn now_millis() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?;
    i64::try_from(duration.as_millis()).map_err(|_| "system time is out of range".into())
}

fn default_quadrant() -> i64 {
    DEFAULT_QUADRANT
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddChecklistItemRequest {
    pub text: String,
    #[serde(default = "default_quadrant")]
    pub quadrant: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItemRequest {
    pub id: String,
}

#[tauri::command]
pub async fn list_checklist_items(app: AppHandle) -> Result<Vec<ChecklistItem>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<ChecklistStore>().list())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn add_checklist_item(
    app: AppHandle,
    request: AddChecklistItemRequest,
) -> Result<ChecklistItem, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ChecklistStore>()
            .add(&request.text, request.quadrant)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn toggle_checklist_item(
    app: AppHandle,
    request: ChecklistItemRequest,
) -> Result<ChecklistItem, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<ChecklistStore>().toggle(&request.id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn delete_checklist_item(
    app: AppHandle,
    request: ChecklistItemRequest,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<ChecklistStore>().delete(&request.id))
        .await
        .map_err(|error| error.to_string())?
}

pub const CHECKLIST_LABEL: &str = "checklist";

/// Prevent duplicate WebviewWindowBuilder calls when the toolbar is double-clicked.
static CHECKLIST_OPEN_IN_FLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub async fn open_checklist_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(CHECKLIST_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    if CHECKLIST_OPEN_IN_FLIGHT.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    let result = build_checklist_window(&app);
    CHECKLIST_OPEN_IN_FLIGHT.store(false, std::sync::atomic::Ordering::SeqCst);
    result
}

fn build_checklist_window(app: &AppHandle) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    // 与看板同理：用无扩展名路由路径让 SvelteKit 客户端路由命中，
    // Tauri 资产解析器会把 /checklist 回退到预渲染的 checklist.html。
    let window =
        WebviewWindowBuilder::new(app, CHECKLIST_LABEL, WebviewUrl::App("checklist".into()))
            .title("DeepPi")
            .decorations(false)
            .resizable(true)
            .minimizable(false)
            .maximizable(false)
            .closable(true)
            .skip_taskbar(true)
            .inner_size(760.0, 620.0)
            .min_inner_size(560.0, 440.0)
            .visible(false)
            .build()
            .map_err(|error| format!("failed to create checklist window: {error}"))?;
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ChecklistStore;

    #[test]
    fn adds_lists_toggles_and_deletes_items() {
        let store = ChecklistStore::in_memory().expect("checklist store should open");
        let item = store.add("Ship the release", 2).expect("item should add");
        assert_eq!(item.quadrant, 2);
        assert_eq!(store.list().expect("items should list").len(), 1);

        let toggled = store.toggle(&item.id).expect("item should toggle");
        assert!(toggled.completed);
        assert!(store.list().expect("items should list")[0].completed);

        store.delete(&item.id).expect("item should delete");
        assert!(store.list().expect("items should list").is_empty());
    }

    #[test]
    fn trims_text_and_rejects_empty_or_control_text() {
        let store = ChecklistStore::in_memory().expect("checklist store should open");
        let item = store
            .add("  Keep the workspace tidy  ", 1)
            .expect("text should trim");
        assert_eq!(item.text, "Keep the workspace tidy");
        assert!(store.add("   ", 1).is_err());
        assert!(store.add("bad\nitem", 1).is_err());
    }

    #[test]
    fn rejects_quadrants_outside_the_matrix() {
        let store = ChecklistStore::in_memory().expect("checklist store should open");
        assert!(store.add("invalid", 0).is_err());
        assert!(store.add("invalid", 5).is_err());
    }

    #[test]
    fn migrates_legacy_items_into_the_first_quadrant() {
        let root = std::env::temp_dir().join(format!("deeppi-checklist-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("test directory should exist");
        let path = root.join("checklist.db");
        {
            let connection =
                rusqlite::Connection::open(&path).expect("legacy database should open");
            connection
                .execute_batch(
                    "CREATE TABLE checklist_items (
                        id TEXT PRIMARY KEY NOT NULL,
                        text TEXT NOT NULL,
                        completed INTEGER NOT NULL DEFAULT 0,
                        created_at INTEGER NOT NULL
                    );
                    INSERT INTO checklist_items (id, text, completed, created_at)
                    VALUES ('legacy-item', 'Legacy item', 0, 1);",
                )
                .expect("legacy schema should be created");
        }
        let store = ChecklistStore::open(&path).expect("checklist store should migrate");
        let items = store.list().expect("items should list");
        assert_eq!(items[0].quadrant, 1);
        drop(store);
        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[test]
    fn persists_items_in_a_file() {
        let root = std::env::temp_dir().join(format!("deeppi-checklist-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("test directory should exist");
        let path = root.join("checklist.db");
        {
            let store = ChecklistStore::open(&path).expect("checklist store should open");
            store.add("Persist this", 4).expect("item should add");
        }
        let store = ChecklistStore::open(&path).expect("checklist store should reopen");
        let items = store.list().expect("items should list");
        assert_eq!(items[0].text, "Persist this");
        assert_eq!(items[0].quadrant, 4);
        drop(store);
        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }
}
