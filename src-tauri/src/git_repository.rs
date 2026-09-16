use crate::git_operation::GitBudget;
use crate::message::{msg, msg_with};
#[cfg(test)]
use crate::process_runner::ProcessOutput;
use crate::project_files::{validate_relative, FileIdentity, GuardedPath};
#[cfg(test)]
use std::time::Duration;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RepositoryIdentity {
    project: PathBuf,
    worktree: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
    file_identities: Vec<FileIdentity>,
}

#[derive(Default, Clone)]
pub struct GitStatusGate(Arc<Mutex<HashSet<RepositoryIdentity>>>);

pub(crate) struct GitRepository {
    pub(crate) project: GuardedPath,
    pub(crate) worktree: GuardedPath,
    pub(crate) prefix: String,
    identity: RepositoryIdentity,
    executable: GuardedPath,
    _metadata: Vec<GuardedPath>,
    pub(crate) budget: GitBudget,
}

pub(crate) fn base_command(executable: &Path, cwd: &Path) -> Command {
    let mut command = Command::new(executable);
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    command
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .args([
            "--no-pager",
            "--no-optional-locks",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
        ]);
    command
}

#[cfg(test)]
pub(crate) fn run_git(command: &mut Command) -> Result<ProcessOutput, String> {
    crate::process_runner::run_cancellable(command, Duration::from_secs(60), None)
}

fn discover(executable: &Path, project: &Path, budget: &GitBudget) -> Result<Vec<String>, String> {
    let result = budget.run(base_command(executable, project).args([
        "rev-parse",
        "--path-format=absolute",
        "--show-toplevel",
        "--absolute-git-dir",
        "--git-common-dir",
        "--show-prefix",
    ]))?;
    if !result.status.success() || result.truncated {
        return Err(msg("git.repository.locate_failed"));
    }
    let text =
        std::str::from_utf8(&result.stdout).map_err(|_| msg("git.repository.path_encoding"))?;
    let lines: Vec<String> = text
        .split_terminator('\n')
        .map(|line| line.trim_end_matches('\r').into())
        .collect();
    if lines.len() != 4 {
        return Err(msg("git.repository.locate_format"));
    }
    Ok(lines)
}

fn same_path(left: &Path, right: &Path) -> Result<bool, String> {
    let left = fs::canonicalize(left).map_err(|error| error.to_string())?;
    let right = fs::canonicalize(right).map_err(|error| error.to_string())?;
    Ok(left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase())
}

impl GitRepository {
    #[cfg(test)]
    pub(crate) fn open(root: &Path) -> Result<Self, String> {
        // 生产请求使用 30 秒用户可见预算；测试在默认并行下墙钟时间会被进程/磁盘争用放大，
        // 这里放宽以免机器负载造成假失败。预算与取消语义仍由 git_operation 的专项测试
        // 和 git_index 的取消测试覆盖。
        Self::open_with_budget(root, GitBudget::new(Duration::from_secs(300)))
    }

    pub(crate) fn open_with_budget(root: &Path, budget: GitBudget) -> Result<Self, String> {
        budget.check()?;
        let project = GuardedPath::open(root, "", true)?;
        let mut marker = None;
        for ancestor in root.ancestors() {
            budget.check()?;
            let path = ancestor.join(".git");
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    marker = Some(GuardedPath::pin(&path, metadata.is_dir())?);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(msg_with(
                        "git.repository.marker_unreadable",
                        &[("detail", &error.to_string())],
                    ))
                }
            }
        }
        let marker = marker.ok_or(msg("git.repository.not_a_worktree"))?;
        let executable = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|path| path.is_absolute())
            .find_map(|path| {
                crate::project_files::pin_external_executable(root, &path.join("git.exe")).ok()
            })
            .ok_or(msg("git.repository.git_missing"))?;
        let located = discover(&executable.path, &project.path, &budget)?;
        let worktree = GuardedPath::pin(Path::new(&located[0]), true)?;
        let executable =
            crate::project_files::pin_external_executable(&worktree.path, &executable.path)?;
        let git_dir = GuardedPath::pin(Path::new(&located[1]), true)?;
        let common_dir = GuardedPath::pin(Path::new(&located[2]), true)?;
        let prefix = located[3].clone();
        validate_relative(prefix.trim_end_matches('/'), true)?;
        if !prefix.is_empty() && !prefix.ends_with('/')
            || !same_path(&worktree.path.join(&prefix), &project.path)?
            || !same_path(
                marker
                    .path
                    .parent()
                    .ok_or(msg("git.repository.marker_parent_missing"))?,
                &worktree.path,
            )?
        {
            return Err(msg("git.repository.worktree_mismatch"));
        }
        let mut identity = RepositoryIdentity {
            project: project.path.clone(),
            worktree: worktree.path.clone(),
            git_dir: git_dir.path.clone(),
            common_dir: common_dir.path.clone(),
            file_identities: vec![project.identity()?, worktree.identity()?],
        };
        let mut metadata = vec![marker];
        let common_marker = git_dir.path.join("commondir");
        match fs::symlink_metadata(&common_marker) {
            Ok(_) => metadata.push(GuardedPath::pin(&common_marker, false)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        metadata.extend([git_dir, common_dir]);
        for guard in &metadata {
            identity.file_identities.push(guard.identity()?);
        }
        if discover(&executable.path, &project.path, &budget)? != located {
            return Err(msg("git.repository.changed"));
        }
        Ok(Self {
            project,
            worktree,
            prefix,
            identity,
            executable,
            _metadata: metadata,
            budget,
        })
    }

    pub(crate) fn command(&self) -> Command {
        let mut command = base_command(&self.executable.path, &self.worktree.path);
        command
            .arg("--git-dir")
            .arg(&self.identity.git_dir)
            .arg("--work-tree")
            .arg(&self.worktree.path)
            .env("GIT_COMMON_DIR", &self.identity.common_dir);
        command
    }

    pub(crate) fn git_dir(&self) -> &Path {
        &self.identity.git_dir
    }

    pub(crate) fn to_repo_path(&self, path: &str) -> Result<String, String> {
        validate_relative(path, false)?;
        Ok(format!("{}{path}", self.prefix))
    }

    pub(crate) fn to_project_path<'a>(&self, path: &'a str) -> Option<&'a str> {
        path.get(..self.prefix.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(&self.prefix))?;
        path.get(self.prefix.len()..)
            .filter(|path| !path.is_empty())
    }
}

fn ensure_trusted(
    trusted: &mut HashSet<RepositoryIdentity>,
    identity: &RepositoryIdentity,
    confirm: impl FnOnce() -> bool,
) -> Result<(), String> {
    if !trusted.contains(identity) {
        if !confirm() {
            return Err(msg("git.repository.not_trusted"));
        }
        trusted.insert(identity.clone());
    }
    Ok(())
}

pub(crate) fn with_repository<T>(
    app: &tauri::AppHandle,
    root: &Path,
    // 请求用户信任时的确认文案；`None` 表示本次操作不请求信任（直接按未信任处理）。
    trust_dialog: Option<crate::dialog_text::ConfirmDialog>,
    budget: GitBudget,
    operation: impl FnOnce(&GitRepository) -> Result<T, String>,
) -> Result<T, String> {
    let gate = app.state::<GitStatusGate>().inner().clone();
    let mut trusted = budget.lock(&gate.0)?;
    let repo = GitRepository::open_with_budget(root, budget.clone())?;
    ensure_trusted(&mut trusted, &repo.identity, || {
        let Some(dialog) = trust_dialog.as_ref() else {
            return false;
        };
        app.dialog()
            .message(dialog.render(&[
                ("project", repo.project.path.display().to_string()),
                ("worktree", repo.worktree.path.display().to_string()),
                ("git_dir", repo.identity.git_dir.display().to_string()),
                ("common_dir", repo.identity.common_dir.display().to_string()),
            ]))
            .title(dialog.title.clone())
            .buttons(MessageDialogButtons::OkCancelCustom(
                dialog.confirm_label.clone(),
                dialog.cancel_label.clone(),
            ))
            .blocking_show()
    })?;
    budget.check()?;
    operation(&repo)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::git_test_support::TestRepo;

    #[test]
    fn locates_nested_project_and_scopes_literal_paths() {
        let fixture = TestRepo::new();
        let subdir = fixture.root.join("nested space");
        std::fs::create_dir(&subdir).unwrap();
        let repo = GitRepository::open(&subdir).unwrap();
        assert_eq!(repo.prefix, "nested space/");
        assert_eq!(
            repo.to_repo_path("file.txt").unwrap(),
            "nested space/file.txt"
        );
        assert_eq!(
            repo.to_project_path("nested space/file.txt"),
            Some("file.txt")
        );
        assert_eq!(repo.to_project_path("elsewhere/file.txt"), None);
        assert!(repo.to_repo_path("../elsewhere").is_err());
        assert!(repo.to_repo_path(":(top)*").is_err());
    }

    #[test]
    fn locates_linked_worktree_and_pins_gitfile() {
        let fixture = TestRepo::new();
        std::fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        let worktree = fixture.base.join("linked");
        fixture.git(&[
            "worktree",
            "add",
            "-b",
            "feature",
            worktree.to_str().unwrap(),
        ]);
        let repo = GitRepository::open(&worktree).unwrap();
        assert_ne!(repo.identity.git_dir, repo.identity.common_dir);
        assert!(std::fs::write(worktree.join(".git"), "gitdir: another").is_err());
        assert!(repo.prefix.is_empty());
    }

    #[test]
    fn trust_is_bound_to_project_and_resolved_metadata() {
        let fixture = TestRepo::new();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let mut trusted = HashSet::new();
        assert!(ensure_trusted(&mut trusted, &repo.identity, || false).is_err());
        ensure_trusted(&mut trusted, &repo.identity, || true).unwrap();
        ensure_trusted(&mut trusted, &repo.identity, || panic!("duplicate prompt")).unwrap();
        let mut changed = repo.identity.clone();
        changed.git_dir = fixture.base.join("another");
        assert!(ensure_trusted(&mut trusted, &changed, || false).is_err());
    }

    #[test]
    fn replacing_git_directory_at_same_path_requires_new_consent() {
        let fixture = TestRepo::new();
        let previous = GitRepository::open(&fixture.root).unwrap().identity;
        let mut trusted = HashSet::new();
        ensure_trusted(&mut trusted, &previous, || true).unwrap();
        fs::rename(fixture.root.join(".git"), fixture.base.join("previous-git")).unwrap();
        fixture.git(&["init", "--template=", "--initial-branch=main"]);
        let replacement = GitRepository::open(&fixture.root).unwrap().identity;
        assert_ne!(previous, replacement);
        assert!(ensure_trusted(&mut trusted, &replacement, || false).is_err());
    }

    #[test]
    fn reopening_unchanged_repository_preserves_consent() {
        let fixture = TestRepo::new();
        let identity = GitRepository::open(&fixture.root).unwrap().identity;
        let mut trusted = HashSet::new();
        ensure_trusted(&mut trusted, &identity, || true).unwrap();
        fs::write(fixture.root.join("normal-change.txt"), "user edit\n").unwrap();
        let reopened = GitRepository::open(&fixture.root).unwrap().identity;
        assert_eq!(identity, reopened);
        ensure_trusted(&mut trusted, &reopened, || {
            panic!("normal edits must not revoke trust")
        })
        .unwrap();
    }

    #[test]
    fn replacing_nested_project_directory_requires_new_consent() {
        let fixture = TestRepo::new();
        let project = fixture.root.join("nested");
        fs::create_dir(&project).unwrap();
        let original = GitRepository::open(&project).unwrap().identity;
        fs::rename(&project, fixture.root.join("previous-nested")).unwrap();
        fs::create_dir(&project).unwrap();
        let replaced = GitRepository::open(&project).unwrap().identity;
        assert_eq!(original.git_dir, replaced.git_dir);
        assert_ne!(original, replaced);
        let mut trusted = HashSet::new();
        ensure_trusted(&mut trusted, &original, || true).unwrap();
        assert!(ensure_trusted(&mut trusted, &replaced, || false).is_err());
    }
}
