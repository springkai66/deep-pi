use std::{fs, io::Write, path::PathBuf};

use atomic_write_file::AtomicWriteFile;

use crate::app_paths::AppPaths;

const BRIDGE_SOURCE: &str = include_str!("../resources/deeppi-bridge.ts");

pub fn install(paths: &AppPaths) -> Result<PathBuf, String> {
    let extension_dir = paths.pi_home.join("extensions");
    fs::create_dir_all(&extension_dir)
        .map_err(|error| format!("failed to create Pi extension directory: {error}"))?;
    let destination = extension_dir.join("deeppi-bridge.ts");
    if fs::read_to_string(&destination).ok().as_deref() == Some(BRIDGE_SOURCE) {
        return Ok(destination);
    }
    let mut file = AtomicWriteFile::open(&destination)
        .map_err(|error| format!("failed to open bridge extension: {error}"))?;
    file.write_all(BRIDGE_SOURCE.as_bytes())
        .map_err(|error| format!("failed to write bridge extension: {error}"))?;
    file.commit()
        .map_err(|error| format!("failed to install bridge extension: {error}"))?;
    Ok(destination)
}

pub fn state_file(paths: &AppPaths, task_id: &str) -> Result<PathBuf, String> {
    if task_id.is_empty()
        || task_id.len() > 100
        || !task_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("task id is not safe for a state file".into());
    }
    let directory = paths.temp.join("task-state");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create task state directory: {error}"))?;
    Ok(directory.join(format!("{task_id}.state")))
}

pub fn pipe_name(paths: &AppPaths, task_id: &str) -> Result<String, String> {
    let state = state_file(paths, task_id)?;
    let name = state
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "task id is not safe for a pipe name".to_string())?;
    Ok(format!(r"\\.\pipe\deeppi-{name}"))
}

#[cfg(test)]
mod tests {
    use crate::app_paths::AppPaths;

    use super::{install, pipe_name, state_file};

    #[test]
    fn installs_bridge_and_builds_scoped_state_path() {
        let root = std::env::temp_dir().join(format!("deeppi-bridge-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = AppPaths::from_roots(
            root.join("roaming"),
            root.join("local"),
            root.join("project"),
        )
        .expect("application paths should initialize");

        let extension = install(&paths).expect("bridge should install");
        assert!(extension.is_file());
        assert!(std::fs::read_to_string(extension)
            .expect("bridge should read")
            .contains("agent_settled"));
        assert!(state_file(&paths, "task-1")
            .expect("task id should be safe")
            .starts_with(&paths.temp));

        assert_eq!(
            pipe_name(&paths, "task-1").expect("pipe name should be safe"),
            r"\\.\pipe\deeppi-task-1"
        );

        std::fs::remove_dir_all(root).expect("test paths should be removed");
    }
}
