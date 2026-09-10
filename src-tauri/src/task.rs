#[cfg(test)]
use std::collections::HashMap;
use std::{
    collections::HashSet,
    fs,
    path::Path,
    str::FromStr,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::dsh_api::DshSessionSummary;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Queued,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Waiting)
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "waiting" => Ok(Self::Waiting),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown task status: {value}")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub run_id: Option<String>,
    pub title: String,
    pub agent: String,
    pub status: TaskStatus,
    pub project_id: Option<String>,
    pub project_path: String,
    pub session_id: String,
    pub session_file: Option<String>,
    pub execution_target: String,
    pub interaction_mode: String,
    pub pi_environment: String,
    pub pi_agent_dir: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub archived_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: i64,
    pub last_opened_at: i64,
}

pub struct TaskStore {
    connection: Mutex<Connection>,
}

impl TaskStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create data directory: {error}"))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| format!("failed to open task database: {error}"))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| format!("failed to configure task database: {error}"))?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| format!("failed to enable WAL: {error}"))?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.migrate()?;
        store.backfill_legacy_projects()?;
        store.recover_interrupted()?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self, String> {
        let store = Self {
            connection: Mutex::new(
                Connection::open_in_memory()
                    .map_err(|error| format!("failed to open in-memory database: {error}"))?,
            ),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS tasks (
                    id TEXT PRIMARY KEY NOT NULL,
                    title TEXT NOT NULL,
                    agent TEXT NOT NULL CHECK(agent IN ('pi', 'dsh')),
                    status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'waiting', 'completed', 'failed', 'cancelled')),
                    project_path TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    session_file TEXT,
                    execution_target TEXT NOT NULL DEFAULT 'local',
                    created_at INTEGER NOT NULL,
                    started_at INTEGER,
                    completed_at INTEGER,
                    archived_at INTEGER
                );
                CREATE TABLE IF NOT EXISTS ignored_dsh_sessions (
                    session_id TEXT PRIMARY KEY NOT NULL,
                    deleted_at INTEGER NOT NULL
                );",
            )
            .map_err(|error| format!("failed to create task tables: {error}"))?;
        let has_project_id = connection
            .prepare("PRAGMA table_info(tasks)")
            .map_err(|error| format!("failed to inspect task schema: {error}"))?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| format!("failed to read task schema: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to collect task schema: {error}"))?
            .iter()
            .any(|name| name == "project_id");
        if !has_project_id {
            connection
                .execute("ALTER TABLE tasks ADD COLUMN project_id TEXT", [])
                .map_err(|error| format!("failed to add task project column: {error}"))?;
        }
        let has_interaction_mode = connection
            .prepare("PRAGMA table_info(tasks)")
            .map_err(|error| error.to_string())?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|name| name == "interaction_mode");
        if !has_interaction_mode {
            connection.execute(
                "ALTER TABLE tasks ADD COLUMN interaction_mode TEXT NOT NULL DEFAULT 'tui' CHECK(interaction_mode IN ('tui', 'rpc'))",
                [],
            ).map_err(|error| error.to_string())?;
        }
        let has_pi_environment = connection
            .prepare("PRAGMA table_info(tasks)")
            .map_err(|error| error.to_string())?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|name| name == "pi_environment");
        if !has_pi_environment {
            connection.execute(
                "ALTER TABLE tasks ADD COLUMN pi_environment TEXT NOT NULL DEFAULT 'managed' CHECK(pi_environment IN ('managed', 'native'))", [],
            ).map_err(|error| error.to_string())?;
        }
        let has_pi_agent_dir = connection
            .prepare("PRAGMA table_info(tasks)")
            .map_err(|error| error.to_string())?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|name| name == "pi_agent_dir");
        if !has_pi_agent_dir {
            connection
                .execute("ALTER TABLE tasks ADD COLUMN pi_agent_dir TEXT", [])
                .map_err(|error| error.to_string())?;
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS ignored_pi_sessions (session_id TEXT PRIMARY KEY NOT NULL);
             CREATE INDEX IF NOT EXISTS idx_tasks_pi_session ON tasks(agent, pi_environment, session_id);",
        ).map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL,
                    canonical_path TEXT NOT NULL UNIQUE,
                    created_at INTEGER NOT NULL,
                    last_opened_at INTEGER NOT NULL,
                    removed_at INTEGER
                );
                CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
                CREATE INDEX IF NOT EXISTS idx_tasks_archived_at ON tasks(archived_at);
                CREATE INDEX IF NOT EXISTS idx_tasks_project_id ON tasks(project_id);",
            )
            .map_err(|error| format!("failed to migrate project tables: {error}"))?;
        let has_removed_at = connection
            .prepare("PRAGMA table_info(projects)")
            .map_err(|error| format!("failed to inspect project schema: {error}"))?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| format!("failed to read project schema: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to collect project schema: {error}"))?
            .iter()
            .any(|name| name == "removed_at");
        if !has_removed_at {
            connection
                .execute("ALTER TABLE projects ADD COLUMN removed_at INTEGER", [])
                .map_err(|error| format!("failed to add project removal column: {error}"))?;
        }
        connection
            .execute_batch("PRAGMA user_version = 6;")
            .map_err(|error| format!("failed to update task schema version: {error}"))?;
        Ok(())
    }

    fn backfill_legacy_projects(&self) -> Result<(), String> {
        let legacy_paths = {
            let connection = self
                .connection
                .lock()
                .map_err(|_| "task database lock is poisoned".to_string())?;
            let mut statement = connection
                .prepare(
                    "SELECT DISTINCT project_path FROM tasks
                     WHERE agent = 'pi' AND project_id IS NULL AND project_path <> ''",
                )
                .map_err(|error| format!("failed to inspect legacy projects: {error}"))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| format!("failed to query legacy projects: {error}"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("failed to read legacy projects: {error}"))?
        };
        for path in legacy_paths {
            if !Path::new(&path).is_dir() {
                continue;
            }
            let project = self.add_project(&path)?;
            self.connection
                .lock()
                .map_err(|_| "task database lock is poisoned".to_string())?
                .execute(
                    "UPDATE tasks SET project_id = ?1
                     WHERE agent = 'pi' AND project_id IS NULL AND project_path = ?2",
                    params![project.id, path],
                )
                .map_err(|error| format!("failed to link legacy tasks: {error}"))?;
        }
        Ok(())
    }

    pub fn add_project(&self, path: &str) -> Result<ProjectRecord, String> {
        let input = Path::new(path);
        if !input.is_dir() {
            return Err("project directory does not exist".into());
        }
        let canonical = fs::canonicalize(input)
            .map_err(|error| format!("failed to resolve project directory: {error}"))?;
        let raw_path = canonical.to_string_lossy();
        let display_path = clean_display_path(&raw_path);
        let canonical_path = display_path.replace('\\', "/").to_lowercase();
        let name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("项目")
            .to_owned();
        let timestamp = now_millis()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?;
        if let Some(existing) = connection
            .query_row(
                "SELECT id, name, path, created_at, last_opened_at
                 FROM projects WHERE canonical_path = ?1",
                [&canonical_path],
                project_from_row,
            )
            .optional()
            .map_err(|error| format!("failed to find project: {error}"))?
        {
            connection
                .execute(
                    "UPDATE projects SET last_opened_at = ?1, removed_at = NULL WHERE id = ?2",
                    params![timestamp, existing.id],
                )
                .map_err(|error| format!("failed to update project: {error}"))?;
            return Ok(ProjectRecord {
                last_opened_at: timestamp,
                ..existing
            });
        }
        let project = ProjectRecord {
            id: format!("project-{}", Uuid::new_v4()),
            name,
            path: display_path,
            created_at: timestamp,
            last_opened_at: timestamp,
        };
        connection
            .execute(
                "INSERT INTO projects (id, name, path, canonical_path, created_at, last_opened_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    project.id,
                    project.name,
                    project.path,
                    canonical_path,
                    project.created_at
                ],
            )
            .map_err(|error| format!("failed to add project: {error}"))?;
        Ok(project)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT id, name, path, created_at, last_opened_at
                 FROM projects WHERE removed_at IS NULL
                 ORDER BY last_opened_at DESC, name COLLATE NOCASE ASC",
            )
            .map_err(|error| format!("failed to prepare project list: {error}"))?;
        let rows = statement
            .query_map([], project_from_row)
            .map_err(|error| format!("failed to query projects: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to read projects: {error}"))
    }

    pub fn project_path(&self, id: &str) -> Result<String, String> {
        self.connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .query_row(
                "SELECT path FROM projects WHERE id = ?1 AND removed_at IS NULL",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("failed to read project: {error}"))?
            .ok_or_else(|| "Project is not in the workspace".into())
    }

    pub fn touch_project(&self, id: &str) -> Result<(), String> {
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE projects SET last_opened_at = ?1 WHERE id = ?2",
                params![now_millis()?, id],
            )
            .map_err(|error| format!("failed to update project access time: {error}"))?;
        ensure_updated(affected)
    }

    pub fn remove_project(&self, id: &str) -> Result<(), String> {
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE projects SET removed_at = ?1 WHERE id = ?2 AND removed_at IS NULL",
                params![now_millis()?, id],
            )
            .map_err(|error| format!("failed to remove project: {error}"))?;
        ensure_updated(affected)
    }

    fn project(&self, id: &str) -> Result<Option<ProjectRecord>, String> {
        self.connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .query_row(
                "SELECT id, name, path, created_at, last_opened_at
                 FROM projects WHERE id = ?1 AND removed_at IS NULL",
                [id],
                project_from_row,
            )
            .optional()
            .map_err(|error| format!("failed to load project: {error}"))
    }

    #[cfg(test)]
    pub fn create_pi_task(&self, title: &str, project_path: &str) -> Result<TaskRecord, String> {
        self.insert_pi_task(None, title, project_path)
    }

    pub fn create_pi_task_for_project(
        &self,
        project_id: &str,
        title: &str,
    ) -> Result<TaskRecord, String> {
        let project = self
            .project(project_id)?
            .ok_or_else(|| "project was not found".to_string())?;
        self.insert_pi_task(Some(project_id), title, &project.path)
    }

    fn insert_pi_task(
        &self,
        project_id: Option<&str>,
        title: &str,
        project_path: &str,
    ) -> Result<TaskRecord, String> {
        let title = validate_title(title)?;
        let id = format!("task-{}", Uuid::new_v4());
        let session_id = Uuid::new_v4().to_string();
        let timestamp = now_millis()?;
        self.connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "INSERT INTO tasks (
                    id, title, agent, status, project_id, project_path, session_id,
                    execution_target, created_at, started_at
                 ) VALUES (?1, ?2, 'pi', 'running', ?3, ?4, ?5, 'local', ?6, ?6)",
                params![id, title, project_id, project_path, session_id, timestamp],
            )
            .map_err(|error| format!("failed to create task: {error}"))?;
        self.get(&id)?
            .ok_or_else(|| "created task could not be loaded".into())
    }

    pub fn sync_dsh_sessions(&self, sessions: &[DshSessionSummary]) -> Result<(), String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start DSH session sync: {error}"))?;
        let ignored = {
            let mut statement = transaction
                .prepare("SELECT session_id FROM ignored_dsh_sessions")
                .map_err(|error| format!("failed to inspect ignored DSH sessions: {error}"))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| format!("failed to query ignored DSH sessions: {error}"))?;
            rows.collect::<Result<HashSet<_>, _>>()
                .map_err(|error| format!("failed to read ignored DSH sessions: {error}"))?
        };
        for session in sessions {
            if ignored.contains(&session.session_id) {
                continue;
            }
            let id = format!("dsh-{}", session.session_id);
            let title = dsh_title(session);
            let status = if session.running {
                "running"
            } else {
                "waiting"
            };
            transaction
                .execute(
                    "INSERT INTO tasks (
                        id, title, agent, status, project_id, project_path, session_id,
                        execution_target, created_at, started_at
                     ) VALUES (?1, ?2, 'dsh', ?3, NULL, ?4, ?5, 'local', ?6, ?6)
                     ON CONFLICT(id) DO UPDATE SET
                        title = excluded.title,
                        status = excluded.status,
                        project_path = excluded.project_path,
                        created_at = excluded.created_at",
                    params![
                        id,
                        title,
                        status,
                        session.cwd.as_deref().unwrap_or_default(),
                        session.session_id,
                        session.updated_at,
                    ],
                )
                .map_err(|error| format!("failed to upsert DSH session: {error}"))?;
        }
        let existing = {
            let mut statement = transaction
                .prepare("SELECT id, session_id FROM tasks WHERE agent = 'dsh'")
                .map_err(|error| format!("failed to inspect DSH tasks: {error}"))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| format!("failed to query DSH tasks: {error}"))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("failed to read DSH tasks: {error}"))?
        };
        for (id, session_id) in existing {
            if !sessions
                .iter()
                .any(|session| session.session_id == session_id)
            {
                transaction
                    .execute("DELETE FROM tasks WHERE id = ?1", [id])
                    .map_err(|error| format!("failed to remove stale DSH task: {error}"))?;
            }
        }
        for session_id in ignored {
            if !sessions
                .iter()
                .any(|session| session.session_id == session_id)
            {
                transaction
                    .execute(
                        "DELETE FROM ignored_dsh_sessions WHERE session_id = ?1",
                        [session_id],
                    )
                    .map_err(|error| format!("failed to remove stale DSH tombstone: {error}"))?;
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("failed to commit DSH session sync: {error}"))
    }

    pub fn list(&self) -> Result<Vec<TaskRecord>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT id, title, agent, status, project_id, project_path, session_id,
                        session_file, execution_target, created_at, started_at,
                        completed_at, archived_at, interaction_mode, pi_environment, pi_agent_dir
                 FROM tasks
                 ORDER BY archived_at IS NOT NULL, created_at DESC",
            )
            .map_err(|error| format!("failed to prepare task list: {error}"))?;
        let rows = statement
            .query_map([], task_from_row)
            .map_err(|error| format!("failed to query tasks: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to read tasks: {error}"))
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<(), String> {
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE tasks SET title = ?1 WHERE id = ?2",
                params![validate_title(title)?, id],
            )
            .map_err(|error| format!("failed to rename task: {error}"))?;
        ensure_updated(affected)
    }

    pub fn set_interaction_mode(&self, id: &str, mode: &str) -> Result<(), String> {
        if !matches!(mode, "rpc" | "tui") {
            return Err("Unknown interaction mode".into());
        }
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned")?
            .execute(
                "UPDATE tasks SET interaction_mode = ?1 WHERE id = ?2 AND agent = 'pi'",
                params![mode, id],
            )
            .map_err(|error| error.to_string())?;
        ensure_updated(affected)
    }

    pub fn set_pi_environment(
        &self,
        id: &str,
        environment: &str,
        home: &Path,
    ) -> Result<(), String> {
        if !matches!(environment, "native" | "managed") {
            return Err("Unknown Pi environment".into());
        }
        crate::snapshot::reject_link(home)?;
        let home = fs::canonicalize(home).map_err(|_| "Pi configuration directory unavailable")?;
        let affected = self.connection.lock().map_err(|_| "task database lock is poisoned")?
            .execute("UPDATE tasks SET pi_environment = ?1, pi_agent_dir = ?3 WHERE id = ?2 AND agent = 'pi'", params![environment, id, home.to_string_lossy()])
            .map_err(|error| error.to_string())?;
        ensure_updated(affected)
    }

    #[cfg(test)]
    pub fn import_native_sessions(
        &self,
        sessions: &[crate::native_pi::NativeSession],
        home: &Path,
    ) -> Result<(usize, usize), String> {
        crate::snapshot::reject_link(home)?;
        let home = fs::canonicalize(home).map_err(|_| "Native Pi directory unavailable")?;
        let home = home.to_string_lossy().into_owned();
        let (mut existing, mut projects) = {
            let connection = self
                .connection
                .lock()
                .map_err(|_| "task database lock is poisoned")?;
            let mut statement = connection.prepare(
                "SELECT session_id, session_file, pi_agent_dir FROM tasks WHERE agent = 'pi' AND pi_environment = 'native'",
            ).map_err(|error| error.to_string())?;
            let mut existing = statement
                .query_map([], |row| {
                    let id: String = row.get(0)?;
                    let file: Option<String> = row.get(1)?;
                    let directory: Option<String> = row.get(2)?;
                    Ok(match (file, directory) {
                        (Some(file), Some(directory)) => {
                            crate::native_pi::session_key(&directory, &file)
                        }
                        _ => id,
                    })
                })
                .map_err(|error| error.to_string())?
                .collect::<Result<HashSet<_>, _>>()
                .map_err(|error| error.to_string())?;
            let mut ignored = connection
                .prepare("SELECT session_id FROM ignored_pi_sessions")
                .map_err(|error| error.to_string())?;
            existing.extend(
                ignored
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|error| error.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?,
            );
            let mut statement = connection
                .prepare("SELECT canonical_path, id FROM projects")
                .map_err(|error| error.to_string())?;
            let projects = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| error.to_string())?
                .collect::<Result<HashMap<_, _>, _>>()
                .map_err(|error| error.to_string())?;
            (existing, projects)
        };
        let mut prepared = Vec::new();
        let mut resolved = HashMap::new();
        let mut skipped = 0;
        for session in sessions {
            let key = crate::native_pi::session_key(&home, &session.file);
            if existing.contains(&key) || existing.contains(&session.id) {
                continue;
            }
            if !crate::native_pi::valid_session_id(&session.id) {
                skipped += 1;
                continue;
            }
            let project = if let Some(project) = resolved.get(&session.cwd) {
                project
            } else {
                let canonical = match fs::canonicalize(&session.cwd) {
                    Ok(path) if path.is_dir() => path,
                    _ => {
                        skipped += 1;
                        continue;
                    }
                };
                let path = clean_display_path(&canonical.to_string_lossy());
                let key = path.replace('\\', "/").to_lowercase();
                let project_id = if let Some(id) = projects.get(&key) {
                    id.clone()
                } else {
                    let project = self.add_project(&path)?;
                    projects.insert(key, project.id.clone());
                    project.id
                };
                resolved
                    .entry(session.cwd.clone())
                    .or_insert((project_id, path))
            };
            prepared.push((session, project.clone(), key.clone()));
            existing.insert(key);
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned")?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let mut imported = 0;
        for (session, (project_id, path), key) in prepared {
            imported += transaction.execute(
                "INSERT INTO tasks (id, title, agent, status, project_id, project_path, session_id,
                  session_file, execution_target, interaction_mode, pi_environment, created_at, completed_at, pi_agent_dir)
                 SELECT ?1, ?2, 'pi', 'completed', ?3, ?4, ?5, ?6, 'local', 'rpc', 'native', ?7, ?7, ?8
                 WHERE NOT EXISTS (SELECT 1 FROM ignored_pi_sessions WHERE session_id IN (?5, ?9))
                   AND NOT EXISTS (SELECT 1 FROM tasks WHERE id = ?1 OR (agent = 'pi' AND pi_agent_dir = ?8 AND session_id = ?5 AND (session_file IS NULL OR session_file = ?6)))",
                params![format!("pi-native-{key}"), session.title, project_id, path, session.id, session.file, session.timestamp, home, key],
            ).map_err(|error| format!("failed to index native Pi session: {error}"))?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok((imported, skipped))
    }

    pub fn set_session_file(&self, id: &str, session_id: &str, path: &str) -> Result<(), String> {
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned")?
            .execute(
                "UPDATE tasks SET session_file = ?1 WHERE id = ?2 AND session_id = ?3",
                params![path, id, session_id],
            )
            .map_err(|error| error.to_string())?;
        ensure_updated(affected)
    }

    pub fn restart(&self, id: &str) -> Result<TaskRecord, String> {
        let task = self
            .get(id)?
            .ok_or_else(|| "task was not found".to_string())?;
        if task.status.is_active() {
            return Err("task is already active".into());
        }
        if task.archived_at.is_some() {
            return Err("archived tasks must be restored before restart".into());
        }
        if !crate::native_pi::valid_session_id(&task.session_id) {
            return Err("task has an invalid Pi session id".into());
        }
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE tasks
                 SET status = 'running', started_at = ?1, completed_at = NULL
                 WHERE id = ?2",
                params![now_millis()?, id],
            )
            .map_err(|error| format!("failed to restart task: {error}"))?;
        ensure_updated(affected)?;
        self.get(id)?
            .ok_or_else(|| "restarted task could not be loaded".into())
    }

    pub fn set_status(&self, id: &str, status: TaskStatus) -> Result<(), String> {
        let completed_at = status.is_terminal().then(now_millis).transpose()?;
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE tasks SET status = ?1, completed_at = ?2 WHERE id = ?3",
                params![status.as_str(), completed_at, id],
            )
            .map_err(|error| format!("failed to update task status: {error}"))?;
        ensure_updated(affected)
    }

    pub fn set_active_status(&self, id: &str, status: TaskStatus) -> Result<bool, String> {
        if !matches!(status, TaskStatus::Running | TaskStatus::Waiting) {
            return Err("bridge status must be running or waiting".into());
        }
        self.connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE tasks SET status = ?1
                 WHERE id = ?2 AND status IN ('queued', 'running', 'waiting')",
                params![status.as_str(), id],
            )
            .map(|affected| affected > 0)
            .map_err(|error| format!("failed to update active task status: {error}"))
    }

    pub fn finish_if_active(&self, id: &str, status: TaskStatus) -> Result<(), String> {
        if !status.is_terminal() {
            return Err("finished task status must be terminal".into());
        }
        self.connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE tasks
                 SET status = ?1, completed_at = ?2
                 WHERE id = ?3 AND status IN ('queued', 'running', 'waiting')",
                params![status.as_str(), now_millis()?, id],
            )
            .map(|_| ())
            .map_err(|error| format!("failed to finish task: {error}"))
    }

    pub fn archive(&self, id: &str) -> Result<(), String> {
        let task = self
            .get(id)?
            .ok_or_else(|| "task was not found".to_string())?;
        if task.status.is_active() {
            return Err("active tasks must be stopped before archiving".into());
        }
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE tasks SET archived_at = ?1 WHERE id = ?2",
                params![now_millis()?, id],
            )
            .map_err(|error| format!("failed to archive task: {error}"))?;
        ensure_updated(affected)
    }

    pub fn unarchive(&self, id: &str) -> Result<(), String> {
        let affected = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE tasks SET archived_at = NULL WHERE id = ?1 AND archived_at IS NOT NULL",
                [id],
            )
            .map_err(|error| format!("failed to restore task: {error}"))?;
        ensure_updated(affected)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let task = self
            .get(id)?
            .ok_or_else(|| "task was not found".to_string())?;
        if task.agent == "pi" && task.status.is_active() {
            return Err("active Pi tasks must be stopped before deletion".into());
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("failed to start task deletion: {error}"))?;
        if task.agent == "dsh" {
            transaction
                .execute(
                    "INSERT OR REPLACE INTO ignored_dsh_sessions (session_id, deleted_at)
                     VALUES (?1, ?2)",
                    params![task.session_id, now_millis()?],
                )
                .map_err(|error| format!("failed to hide DSH session: {error}"))?;
        }
        if task.agent == "pi" && task.pi_environment == "native" {
            let key = match (&task.pi_agent_dir, &task.session_file) {
                (Some(home), Some(file)) => crate::native_pi::session_key(home, file),
                _ => task.session_id.clone(),
            };
            transaction
                .execute(
                    "INSERT OR IGNORE INTO ignored_pi_sessions (session_id) VALUES (?1)",
                    [&key],
                )
                .map_err(|error| error.to_string())?;
        }
        let affected = transaction
            .execute("DELETE FROM tasks WHERE id = ?1", [id])
            .map_err(|error| format!("failed to delete task: {error}"))?;
        ensure_updated(affected)?;
        transaction
            .commit()
            .map_err(|error| format!("failed to commit task deletion: {error}"))
    }

    pub fn recover_interrupted(&self) -> Result<(), String> {
        let timestamp = now_millis()?;
        self.connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .execute(
                "UPDATE tasks
                 SET status = 'failed', completed_at = ?1
                 WHERE status IN ('queued', 'running', 'waiting')",
                [timestamp],
            )
            .map(|_| ())
            .map_err(|error| format!("failed to recover interrupted tasks: {error}"))
    }

    pub(crate) fn get(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        self.connection
            .lock()
            .map_err(|_| "task database lock is poisoned".to_string())?
            .query_row(
                "SELECT id, title, agent, status, project_id, project_path, session_id,
                        session_file, execution_target, created_at, started_at,
                        completed_at, archived_at, interaction_mode, pi_environment, pi_agent_dir
                 FROM tasks WHERE id = ?1",
                [id],
                task_from_row,
            )
            .optional()
            .map_err(|error| format!("failed to load task: {error}"))
    }
}

fn dsh_title(session: &DshSessionSummary) -> String {
    let fallback = session
        .cwd
        .as_deref()
        .and_then(|cwd| cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next())
        .filter(|name| !name.is_empty())
        .unwrap_or(&session.session_id);
    session
        .title
        .as_deref()
        .unwrap_or(fallback)
        .chars()
        .take(200)
        .collect()
}

fn clean_display_path(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else {
        raw.strip_prefix("\\\\?\\").unwrap_or(raw).to_owned()
    }
}

fn validate_title(title: &str) -> Result<&str, String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        Err("task title must contain 1 to 200 characters".into())
    } else {
        Ok(title)
    }
}

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<ProjectRecord> {
    let path: String = row.get(2)?;
    Ok(ProjectRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        path: clean_display_path(&path),
        created_at: row.get(3)?,
        last_opened_at: row.get(4)?,
    })
}

fn task_from_row(row: &Row<'_>) -> rusqlite::Result<TaskRecord> {
    let status: String = row.get(3)?;
    Ok(TaskRecord {
        id: row.get(0)?,
        run_id: None,
        title: row.get(1)?,
        agent: row.get(2)?,
        status: TaskStatus::from_str(&status).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, error.into())
        })?,
        project_id: row.get(4)?,
        project_path: row.get(5)?,
        session_id: row.get(6)?,
        session_file: row.get(7)?,
        execution_target: row.get(8)?,
        interaction_mode: row.get(13)?,
        pi_environment: row.get(14)?,
        pi_agent_dir: row.get(15)?,
        created_at: row.get(9)?,
        started_at: row.get(10)?,
        completed_at: row.get(11)?,
        archived_at: row.get(12)?,
    })
}

fn now_millis() -> Result<i64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?;
    i64::try_from(duration.as_millis()).map_err(|_| "system time is out of range".into())
}

fn ensure_updated(affected: usize) -> Result<(), String> {
    if affected == 0 {
        Err("task was not found".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn list_projects(app: AppHandle) -> Result<Vec<ProjectRecord>, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<TaskStore>().list_projects())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn add_project(app: AppHandle, path: String) -> Result<ProjectRecord, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<TaskStore>().add_project(&path))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn remove_project(app: AppHandle, project_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<TaskStore>().remove_project(&project_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn touch_project(app: AppHandle, project_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<TaskStore>().touch_project(&project_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn list_tasks(app: AppHandle) -> Result<Vec<TaskRecord>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut records = app.state::<TaskStore>().list()?;
        app.state::<crate::pty::PtyManager>()
            .attach_run_ids(&mut records)?;
        app.state::<crate::rpc::RpcManager>()
            .attach_run_ids(&mut records)?;
        Ok(records)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn archive_task(app: AppHandle, task_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<TaskStore>().archive(&task_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn rename_task(app: AppHandle, task_id: String, title: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<TaskStore>().rename(&task_id, &title))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn restore_task(app: AppHandle, task_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<TaskStore>().unarchive(&task_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn delete_task(app: AppHandle, task_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<TaskStore>().delete(&task_id))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::{TaskStatus, TaskStore};

    #[test]
    fn adds_a_project_once_and_links_new_pi_tasks() {
        let root =
            std::env::temp_dir().join(format!("deeppi-project-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("project directory should be created");
        let store = TaskStore::in_memory().expect("in-memory store should open");

        let first = store
            .add_project(root.to_str().expect("project path should be utf8"))
            .expect("project should be added");
        let second = store
            .add_project(root.to_str().expect("project path should be utf8"))
            .expect("duplicate project should reuse the existing row");
        let task = store
            .create_pi_task_for_project(&first.id, "Project task")
            .expect("task should use the selected project");

        assert_eq!(first.id, second.id);
        assert_eq!(
            store.list_projects().expect("projects should list").len(),
            1
        );
        assert_eq!(task.project_id.as_deref(), Some(first.id.as_str()));
        assert_eq!(task.project_path, first.path);

        std::fs::remove_dir_all(root).expect("project directory should be removed");
    }

    #[test]
    fn removes_project_from_navigation_and_reactivates_it() {
        let root =
            std::env::temp_dir().join(format!("deeppi-project-remove-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("project directory should be created");
        let store = TaskStore::in_memory().expect("in-memory store should open");
        let project = store
            .add_project(root.to_str().expect("project path should be utf8"))
            .expect("project should be added");
        store
            .create_pi_task_for_project(&project.id, "Retained task")
            .expect("task should use the project");

        store
            .remove_project(&project.id)
            .expect("project should be removed from navigation");

        assert!(store.project_path(&project.id).is_err());
        assert!(store.project_path("unknown-project").is_err());

        assert!(store
            .list_projects()
            .expect("projects should list")
            .is_empty());
        assert_eq!(store.list().expect("tasks should list").len(), 1);
        assert!(store
            .create_pi_task_for_project(&project.id, "Hidden task")
            .is_err());

        let restored = store
            .add_project(root.to_str().expect("project path should be utf8"))
            .expect("project should be reactivated");
        assert_eq!(restored.id, project.id);
        assert_eq!(
            store.list_projects().expect("projects should list").len(),
            1
        );

        std::fs::remove_dir_all(root).expect("project directory should be removed");
    }

    #[test]
    fn removes_windows_extended_path_prefix_for_display() {
        assert_eq!(super::clean_display_path(r"\\?\F:\repo"), r"F:\repo");
        assert_eq!(
            super::clean_display_path(r"\\?\UNC\server\repo"),
            r"\\server\repo"
        );
    }

    #[test]
    fn creates_and_lists_a_task_with_stable_ids() {
        let store = TaskStore::in_memory().expect("in-memory store should open");

        let created = store
            .create_pi_task("First task", "F:/project")
            .expect("task should be created");
        let tasks = store.list().expect("tasks should be listed");

        assert!(created.id.starts_with("task-"));
        assert!(uuid::Uuid::parse_str(&created.session_id).is_ok());
        assert_eq!(created.status, TaskStatus::Running);
        assert_eq!(tasks, vec![created]);
    }

    #[test]
    fn migration_is_idempotent() {
        let store = TaskStore::in_memory().expect("in-memory store should open");

        store.migrate().expect("second migration should succeed");
    }

    #[test]
    fn renames_a_task() {
        let store = TaskStore::in_memory().expect("in-memory store should open");
        let task = store
            .create_pi_task("Original", "F:/project")
            .expect("task should be created");

        store
            .rename(&task.id, "Renamed")
            .expect("task should rename");

        assert_eq!(store.list().expect("tasks should list")[0].title, "Renamed");
    }

    #[test]
    fn archives_and_deletes_a_completed_task() {
        let store = TaskStore::in_memory().expect("in-memory store should open");
        let task = store
            .create_pi_task("First task", "F:/project")
            .expect("task should be created");
        store
            .set_status(&task.id, TaskStatus::Completed)
            .expect("task should complete");

        store.archive(&task.id).expect("task should archive");
        assert!(store.list().expect("tasks should list")[0]
            .archived_at
            .is_some());

        store
            .unarchive(&task.id)
            .expect("task should leave the archive");
        assert!(store.list().expect("tasks should list")[0]
            .archived_at
            .is_none());

        store.archive(&task.id).expect("task should archive again");
        store.delete(&task.id).expect("task should delete");
        assert!(store.list().expect("tasks should list").is_empty());
    }

    #[test]
    fn process_exit_does_not_override_a_cancelled_task() {
        let store = TaskStore::in_memory().expect("in-memory store should open");
        let task = store
            .create_pi_task("Cancelled task", "F:/project")
            .expect("task should be created");
        store
            .set_status(&task.id, TaskStatus::Cancelled)
            .expect("task should cancel");

        store
            .finish_if_active(&task.id, TaskStatus::Failed)
            .expect("late process exit should be ignored");

        assert_eq!(
            store.list().expect("tasks should list")[0].status,
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn bridge_updates_only_active_task_statuses() {
        let store = TaskStore::in_memory().expect("in-memory store should open");
        let task = store
            .create_pi_task("Bridge task", "F:/project")
            .expect("task should be created");

        store
            .set_active_status(&task.id, TaskStatus::Waiting)
            .expect("active task should wait");
        assert_eq!(
            store.list().expect("tasks should list")[0].status,
            TaskStatus::Waiting
        );

        store
            .set_status(&task.id, TaskStatus::Completed)
            .expect("task should complete");
        store
            .set_active_status(&task.id, TaskStatus::Running)
            .expect("late bridge update should be ignored");
        assert_eq!(
            store.list().expect("tasks should list")[0].status,
            TaskStatus::Completed
        );
    }

    #[test]
    fn restart_keeps_the_original_pi_session() {
        let store = TaskStore::in_memory().expect("in-memory store should open");
        let task = store
            .create_pi_task("Restarted task", "F:/project")
            .expect("task should be created");
        store
            .set_status(&task.id, TaskStatus::Completed)
            .expect("task should complete");

        let restarted = store.restart(&task.id).expect("task should restart");

        assert_eq!(restarted.status, TaskStatus::Running);
        assert_eq!(restarted.session_id, task.session_id);
        assert_eq!(restarted.completed_at, None);
    }

    #[test]
    fn syncs_dsh_sessions_and_removes_missing_rows() {
        let store = TaskStore::in_memory().expect("in-memory store should open");
        let sessions = vec![
            crate::dsh_api::DshSessionSummary {
                session_id: "session-1".into(),
                updated_at: 100,
                running: false,
                blank: true,
                cwd: Some("F:/one".into()),
                title: Some("First DSH task".into()),
            },
            crate::dsh_api::DshSessionSummary {
                session_id: "session-2".into(),
                updated_at: 200,
                running: true,
                blank: false,
                cwd: Some("F:/two".into()),
                title: None,
            },
        ];

        store
            .sync_dsh_sessions(&sessions)
            .expect("DSH sessions should sync");
        let tasks = store.list().expect("tasks should list");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].status, TaskStatus::Running);
        assert_eq!(tasks[1].title, "First DSH task");

        store
            .delete("dsh-session-1")
            .expect("DSH task should be hidden locally");
        store
            .sync_dsh_sessions(&sessions)
            .expect("deleted DSH task should stay hidden");
        assert_eq!(store.list().expect("tasks should list").len(), 1);

        store
            .sync_dsh_sessions(&sessions[..1])
            .expect("missing DSH session should be removed");
        assert!(store.list().expect("tasks should list").is_empty());
    }

    #[test]
    fn recovers_stale_running_tasks_as_failed() {
        let store = TaskStore::in_memory().expect("in-memory store should open");
        store
            .create_pi_task("Interrupted task", "F:/project")
            .expect("task should be created");

        store
            .recover_interrupted()
            .expect("interrupted tasks should recover");

        assert_eq!(
            store.list().expect("tasks should list")[0].status,
            TaskStatus::Failed
        );
    }
}
