use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    current: PathBuf,
    previous: Option<PathBuf>,
}

fn resolve(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let parts = relative.components().collect::<Vec<_>>();
    let allowed = relative == Path::new("current")
        || (parts.len() == 2
            && matches!(parts[0], Component::Normal(name) if name == "versions" || name == "backups")
            && matches!(parts[1], Component::Normal(_)));
    if !allowed {
        return Err("runtime pointer contains an invalid path".into());
    }
    let path = root.join(relative);
    crate::snapshot::reject_link(&path)?;
    if !path.is_dir() {
        return Err(format!(
            "runtime pointer target is missing: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn read(root: &Path) -> Result<Option<Pointer>, String> {
    let path = root.join("active.json");
    crate::snapshot::reject_link(&path)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let pointer: Pointer = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid runtime pointer {}: {error}", path.display()))?;
    resolve(root, &pointer.current)?;
    if let Some(previous) = &pointer.previous {
        resolve(root, previous)?;
    }
    Ok(Some(pointer))
}

pub fn current(root: &Path) -> Result<PathBuf, String> {
    match read(root)? {
        Some(pointer) => resolve(root, &pointer.current),
        None => Ok(root.join("current")),
    }
}

pub fn previous(root: &Path) -> Result<Option<PathBuf>, String> {
    read(root)?
        .and_then(|pointer| pointer.previous)
        .map(|relative| resolve(root, &relative))
        .transpose()
}

fn write(root: &Path, pointer: &Pointer) -> Result<(), String> {
    let bytes = serde_json::to_vec(pointer).map_err(|error| error.to_string())?;
    crate::durable_file::write(&root.join("active.json"), &bytes)
        .map_err(|error| format!("runtime pointer commit failed: {error}"))
}

pub fn activate<T>(
    root: &Path,
    candidate: &Path,
    verify: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let old = current(root)?;
    crate::snapshot::reject_link(candidate)?;
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| "runtime candidate is outside its root")?;
    if relative.components().count() != 2 || !relative.starts_with("versions") {
        return Err("runtime candidate is outside the version directory".into());
    }
    resolve(root, relative)?;
    if old == candidate {
        return Err("runtime candidate is already active".into());
    }
    // Unreferenced complete versions are harmless after interruption; never move the active tree.
    crate::snapshot::sync_tree(candidate)?;
    let value = verify(candidate)?;
    write(
        root,
        &Pointer {
            current: relative.to_path_buf(),
            previous: old
                .is_dir()
                .then(|| old.strip_prefix(root).unwrap().to_path_buf()),
        },
    )?;
    Ok(value)
}

pub fn rollback<T>(
    root: &Path,
    legacy_backup: Option<PathBuf>,
    verify: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let old = current(root)?;
    let target = previous(root)?
        .or(legacy_backup)
        .ok_or("no runtime rollback version")?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| "backup is outside runtime root")?;
    resolve(root, relative)?;
    let value = verify(&target)?;
    write(
        root,
        &Pointer {
            current: relative.to_path_buf(),
            previous: old
                .is_dir()
                .then(|| old.strip_prefix(root).unwrap().to_path_buf()),
        },
    )?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn process_exit_before_or_after_commit_leaves_a_complete_runtime() {
        const ROOT_ENV: &str = "DEEPPI_POINTER_CRASH_TEST_ROOT";
        const PHASE_ENV: &str = "DEEPPI_POINTER_CRASH_TEST_PHASE";
        if let Some(root) = std::env::var_os(ROOT_ENV) {
            let root = PathBuf::from(root);
            activate(&root, &root.join("versions/new"), |_| {
                if std::env::var(PHASE_ENV).unwrap() == "before" {
                    std::process::exit(73);
                }
                Ok(())
            })
            .unwrap();
            std::process::exit(73);
        }
        for phase in ["before", "after"] {
            let root = fixture();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child.args(["--exact", "runtime_pointer::tests::process_exit_before_or_after_commit_leaves_a_complete_runtime"])
                .env(ROOT_ENV, &root).env(PHASE_ENV, phase);
            let output =
                crate::process_runner::run(&mut child, std::time::Duration::from_secs(15)).unwrap();
            assert_eq!(output.status.code(), Some(73), "{}", output.text());
            let expected = if phase == "before" { "old" } else { "new" };
            assert_eq!(
                fs::read_to_string(current(&root).unwrap().join("version")).unwrap(),
                expected
            );
            assert_eq!(
                fs::read_to_string(root.join("current/version")).unwrap(),
                "old"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("deeppi-pointer-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("current")).unwrap();
        fs::write(root.join("current/version"), "old").unwrap();
        fs::create_dir_all(root.join("versions/new")).unwrap();
        fs::write(root.join("versions/new/version"), "new").unwrap();
        root
    }

    #[test]
    fn failed_activation_preserves_the_previous_pointer_and_bytes() {
        let root = fixture();
        let error = activate(&root, &root.join("versions/new"), |_| {
            Err::<(), _>("cancelled".into())
        })
        .unwrap_err();
        assert!(error.contains("cancelled"));
        assert_eq!(current(&root).unwrap(), root.join("current"));
        assert_eq!(
            fs::read_to_string(root.join("current/version")).unwrap(),
            "old"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn activation_survives_reopening_and_rollback_keeps_both_versions() {
        let root = fixture();
        activate(&root, &root.join("versions/new"), |_| Ok(())).unwrap();
        let active = current(&root).unwrap();
        assert_eq!(fs::read_to_string(active.join("version")).unwrap(), "new");
        assert_eq!(previous(&root).unwrap(), Some(root.join("current")));
        rollback(&root, None, |_| Ok(())).unwrap();
        assert_eq!(current(&root).unwrap(), root.join("current"));
        assert!(active.is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_or_escaping_pointers_do_not_fall_back_to_system_runtime() {
        let root = fixture();
        fs::write(
            root.join("active.json"),
            r#"{"current":"../outside","previous":null}"#,
        )
        .unwrap();
        assert!(current(&root).is_err());
        fs::write(root.join("active.json"), "{").unwrap();
        assert!(current(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
