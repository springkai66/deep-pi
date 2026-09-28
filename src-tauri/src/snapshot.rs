use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::recovery::activate_directory;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct Entry {
    target: PathBuf,
    saved: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
struct Manifest {
    version: u8,
    complete: bool,
    entries: Vec<Entry>,
}

pub struct Snapshot {
    root: PathBuf,
    entries: Vec<Entry>,
}

pub(crate) fn reject_link(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("cannot inspect {}: {error}", ancestor.display())),
        };
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = false;
        if metadata.file_type().is_symlink() || reparse {
            return Err(format!(
                "cannot safely snapshot a linked path: {}",
                ancestor.display()
            ));
        }
    }
    Ok(())
}

pub(crate) fn sync_tree(path: &Path) -> Result<(), String> {
    reject_link(path)?;
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
            sync_tree(&entry.map_err(|error| error.to_string())?.path())?;
        }
    } else {
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn copy_entry(source: &Path, destination: &Path) -> Result<(), String> {
    reject_link(source)?;
    if source.is_dir() {
        fs::create_dir_all(destination).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            copy_entry(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        fs::copy(source, destination)
            .map_err(|error| format!("cannot copy {}: {error}", source.display()))?;
    }
    Ok(())
}

impl Snapshot {
    fn mark(&self, complete: bool) -> Result<(), String> {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": 1, "complete": complete, "entries": self.entries,
        }))
        .map_err(|error| error.to_string())?;
        crate::durable_file::write(&self.root.join("manifest.json"), &bytes)
    }

    pub fn ensure_ready(backups: &Path) -> Result<(), String> {
        if Self::pending(backups)?.is_empty() {
            Ok(())
        } else {
            Err("pending package recovery; restart DeepPi before starting sessions or modifying runtimes".into())
        }
    }

    pub fn recover_pending(backups: &Path) -> Result<(), String> {
        for snapshot in Self::pending(backups)? {
            snapshot.restore()?;
            log::warn!(
                "event=snapshot_recovery status=restored path={}",
                snapshot.root.display()
            );
        }
        Ok(())
    }

    fn pending(backups: &Path) -> Result<Vec<Self>, String> {
        let mut pending = Vec::new();
        reject_link(backups)?;
        for directory in fs::read_dir(backups).map_err(|error| error.to_string())? {
            let directory = directory.map_err(|error| error.to_string())?;
            if !directory
                .file_name()
                .to_string_lossy()
                .starts_with("operation-")
            {
                continue;
            }
            let root = directory.path();
            reject_link(&root)?;
            let manifest_path = root.join("manifest.json");
            reject_link(&manifest_path)?;
            let bytes = match fs::read(&manifest_path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            // Older snapshots have no transaction state and remain available for manual diagnosis.
            if serde_json::from_slice::<serde_json::Value>(&bytes)
                .is_ok_and(|value| value.is_array())
            {
                continue;
            }
            let manifest: Manifest = serde_json::from_slice(&bytes)
                .map_err(|error| format!("invalid snapshot {}: {error}", root.display()))?;
            if manifest.version != 1 {
                return Err("unsupported snapshot version".into());
            }
            if manifest.complete {
                continue;
            }
            for (index, entry) in manifest.entries.iter().enumerate() {
                if !entry.target.is_absolute()
                    || entry
                        .target
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                    || entry
                        .saved
                        .as_ref()
                        .is_some_and(|saved| saved != &root.join(index.to_string()))
                {
                    return Err("invalid snapshot recovery path".into());
                }
            }
            let snapshot = Self {
                root,
                entries: manifest.entries,
            };
            pending.push(snapshot);
        }
        Ok(pending)
    }

    pub fn capture(targets: &[PathBuf], backups: &Path) -> Result<Self, String> {
        let keys = targets
            .iter()
            .map(|path| {
                if !path.is_absolute()
                    || path
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                {
                    return Err("snapshot targets must be absolute paths".to_string());
                }
                reject_link(path)?;
                let mut key = path.to_string_lossy().replace('\\', "/");
                if cfg!(windows) {
                    key.make_ascii_lowercase();
                }
                Ok(key.trim_end_matches('/').to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?;
        for (index, key) in keys.iter().enumerate() {
            if keys.iter().enumerate().any(|(other_index, other)| {
                index != other_index && (key == other || other.starts_with(&format!("{key}/")))
            }) {
                return Err("snapshot targets must not overlap".into());
            }
        }
        reject_link(backups)?;
        let root = backups.join(format!("operation-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let mut snapshot = Self {
            root,
            entries: Vec::new(),
        };
        for (index, target) in targets.iter().enumerate() {
            let result = (|| {
                reject_link(target)?;
                let saved = if target.exists() {
                    let saved = snapshot.root.join(index.to_string());
                    copy_entry(target, &saved)?;
                    Some(saved)
                } else {
                    None
                };
                Ok::<_, String>(Entry {
                    target: target.clone(),
                    saved,
                })
            })();
            match result {
                Ok(entry) => snapshot.entries.push(entry),
                Err(error) => {
                    let _ = fs::remove_dir_all(&snapshot.root);
                    return Err(error);
                }
            }
        }
        sync_tree(&snapshot.root)?;
        snapshot.mark(false)?;
        Ok(snapshot)
    }

    pub fn preserve_entry(&self, target: &Path, destination: &Path) -> Result<(), String> {
        for entry in &self.entries {
            if let (Ok(relative), Some(saved)) = (target.strip_prefix(&entry.target), &entry.saved)
            {
                let source = saved.join(relative);
                if source.exists() {
                    return copy_entry(&source, destination);
                }
            }
        }
        Ok(())
    }

    pub fn restore_error(self, original: String) -> String {
        match self.restore() {
            Ok(()) => format!(
                "{original}; previous package state restored; snapshot: {}",
                self.root.display()
            ),
            Err(error) => format!(
                "{original}; recovery failed: {error}; snapshot: {}",
                self.root.display()
            ),
        }
    }

    fn restore(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        for entry in &self.entries {
            let result = (|| {
                reject_link(&entry.target)?;
                let parent = entry
                    .target
                    .parent()
                    .ok_or("snapshot target has no parent")?;
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                let displaced = parent.join(format!(".deeppi-failed-{}", uuid::Uuid::new_v4()));
                if let Some(saved) = &entry.saved {
                    let staging = parent.join(format!(".deeppi-restore-{}", uuid::Uuid::new_v4()));
                    copy_entry(saved, &staging)?;
                    activate_directory(&entry.target, &staging, &displaced, || Ok(()))
                } else if entry.target.exists() {
                    crate::durable_file::rename(&entry.target, &displaced)
                        .map_err(|error| error.to_string())
                } else {
                    Ok(())
                }
            })();
            if let Err(error) = result {
                errors.push(format!("{}: {error}", entry.target.display()));
            }
        }
        if errors.is_empty() {
            for entry in &self.entries {
                if entry.target.exists() {
                    sync_tree(&entry.target)?;
                }
            }
            self.mark(true)
        } else {
            Err(errors.join("; "))
        }
    }

    pub fn commit(self) -> Result<(), String> {
        self.commit_with(|root| fs::remove_dir_all(root))
    }

    fn commit_with(self, cleanup: impl FnOnce(&Path) -> std::io::Result<()>) -> Result<(), String> {
        let persisted = (|| {
            for entry in &self.entries {
                if entry.target.exists() {
                    sync_tree(&entry.target)?;
                }
            }
            self.mark(true)
        })();
        if let Err(error) = persisted {
            return Err(self.restore_error(format!("snapshot commit failed: {error}")));
        }
        if let Err(error) = cleanup(&self.root) {
            log::warn!(
                "event=snapshot_cleanup status=deferred path={} error={error}",
                self.root.display()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn restores_after_the_writer_process_exits_without_cleanup() {
        const ROOT_ENV: &str = "DEEPPI_SNAPSHOT_CRASH_TEST_ROOT";
        if let Some(root) = std::env::var_os(ROOT_ENV) {
            let root = PathBuf::from(root);
            let target = root.join("settings.json");
            let _snapshot =
                Snapshot::capture(std::slice::from_ref(&target), &root.join("backups")).unwrap();
            fs::write(target, "interrupted").unwrap();
            std::process::exit(73);
        }
        let root = std::env::temp_dir().join(format!("deeppi-crash-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("settings.json"), "old").unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "snapshot::tests::restores_after_the_writer_process_exits_without_cleanup",
            ])
            .env(ROOT_ENV, &root);
        let output =
            crate::process_runner::run(&mut child, std::time::Duration::from_secs(15)).unwrap();
        assert_eq!(output.status.code(), Some(73), "{}", output.text());
        assert_eq!(
            fs::read_to_string(root.join("settings.json")).unwrap(),
            "interrupted"
        );
        Snapshot::recover_pending(&root.join("backups")).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("settings.json")).unwrap(),
            "old"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cleanup_failure_does_not_replay_a_committed_operation() {
        let root = std::env::temp_dir().join(format!("deeppi-commit-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("settings.json");
        fs::write(&target, "old").unwrap();
        let snapshot =
            Snapshot::capture(std::slice::from_ref(&target), &root.join("backups")).unwrap();
        fs::write(&target, "new").unwrap();
        snapshot
            .commit_with(|_| Err(std::io::Error::other("cleanup denied")))
            .unwrap();
        Snapshot::recover_pending(&root.join("backups")).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovers_an_interrupted_operation_once_on_startup() {
        let root = std::env::temp_dir().join(format!("deeppi-restart-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("settings.json");
        fs::write(&target, "old").unwrap();
        let snapshot =
            Snapshot::capture(std::slice::from_ref(&target), &root.join("backups")).unwrap();
        fs::write(&target, "interrupted").unwrap();
        drop(snapshot);
        assert!(Snapshot::ensure_ready(&root.join("backups")).is_err());
        Snapshot::recover_pending(&root.join("backups")).unwrap();
        assert!(Snapshot::ensure_ready(&root.join("backups")).is_ok());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        fs::write(&target, "later").unwrap();
        Snapshot::recover_pending(&root.join("backups")).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "later");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_overlapping_snapshot_targets_before_writing() {
        let root = std::env::temp_dir().join(format!("deeppi-overlap-{}", uuid::Uuid::new_v4()));
        let result = Snapshot::capture(
            &[root.join("npm"), root.join("npm/.pi")],
            &root.join("backups"),
        );
        assert!(result.is_err());
        assert!(!root.exists());
    }

    #[test]
    fn restores_files_package_bytes_and_previously_absent_paths() {
        let root = std::env::temp_dir().join(format!("deeppi-snapshot-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("npm")).unwrap();
        fs::write(root.join("npm/version"), "old").unwrap();
        fs::write(root.join("settings.json"), "{\"packages\":[]}").unwrap();
        let snapshot = Snapshot::capture(
            &[
                root.join("npm"),
                root.join("settings.json"),
                root.join("git"),
            ],
            &root.join("backups"),
        )
        .unwrap();
        fs::write(root.join("npm/version"), "new").unwrap();
        fs::write(root.join("settings.json"), "invalid").unwrap();
        fs::create_dir(root.join("git")).unwrap();
        let error = snapshot.restore_error("command failed".into());
        assert!(error.contains("restored"), "{error}");
        assert_eq!(fs::read_to_string(root.join("npm/version")).unwrap(), "old");
        assert_eq!(
            fs::read_to_string(root.join("settings.json")).unwrap(),
            "{\"packages\":[]}"
        );
        assert!(!root.join("git").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
