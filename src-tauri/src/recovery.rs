use std::path::Path;

pub fn activate_directory<T>(
    current: &Path,
    candidate: &Path,
    backup: &Path,
    verify: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    activate_directory_with(current, candidate, backup, verify, |from, to| {
        crate::durable_file::rename(from, to)
    })
}

fn activate_directory_with<T>(
    current: &Path,
    candidate: &Path,
    backup: &Path,
    verify: impl FnOnce() -> Result<T, String>,
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<T, String> {
    let had_current = current.exists();
    if had_current {
        rename(current, backup)
            .map_err(|error| format!("failed to back up {}: {error}", current.display()))?;
    }
    let result = rename(candidate, current)
        .map_err(|error| format!("failed to activate {}: {error}", candidate.display()))
        .and_then(|()| verify());
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let failed = backup.with_file_name(format!("failed-{}", uuid::Uuid::new_v4()));
            let recovery = (|| {
                if current.exists() {
                    rename(current, &failed)?;
                }
                if had_current {
                    rename(backup, current)?;
                }
                Ok::<_, std::io::Error>(())
            })();
            Err(match recovery {
                Ok(()) => format!("{error}; previous state restored"),
                Err(recovery) => format!(
                    "{error}; recovery failed: {recovery}; backup preserved at {}",
                    backup.display()
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_candidate_does_not_lose_the_current_version() {
        let root = std::env::temp_dir().join(format!("deeppi-recovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("current")).unwrap();
        fs::write(root.join("current/version"), "old").unwrap();
        assert!(activate_directory(
            &root.join("current"),
            &root.join("missing"),
            &root.join("backup"),
            || Ok(())
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(root.join("current/version")).unwrap(),
            "old"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_verification_restores_previous_directory() {
        let root = std::env::temp_dir().join(format!("deeppi-recovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("current")).unwrap();
        fs::create_dir_all(root.join("candidate")).unwrap();
        fs::write(root.join("current/version"), "old").unwrap();
        fs::write(root.join("candidate/version"), "new").unwrap();
        let error = activate_directory(
            &root.join("current"),
            &root.join("candidate"),
            &root.join("backup"),
            || Err::<(), _>("verification failed".into()),
        )
        .unwrap_err();
        assert!(error.contains("verification failed"), "{error}");
        assert_eq!(
            fs::read_to_string(root.join("current/version")).unwrap(),
            "old"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_both_activation_and_recovery_errors() {
        let root = std::env::temp_dir().join(format!("deeppi-recovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("current")).unwrap();
        fs::create_dir_all(root.join("candidate")).unwrap();
        let backup = root.join("backup");
        let error = activate_directory_with(
            &root.join("current"),
            &root.join("candidate"),
            &backup,
            || Err::<(), _>("verification failed".into()),
            |from, to| {
                if from == backup {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "restore denied",
                    ))
                } else {
                    fs::rename(from, to)
                }
            },
        )
        .unwrap_err();
        assert!(error.contains("verification failed"), "{error}");
        assert!(error.contains("restore denied"), "{error}");
        assert!(backup.is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
