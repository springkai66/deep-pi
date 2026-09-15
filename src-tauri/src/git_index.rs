use crate::git_status::{read_status_for_index, EntryKind, GitEntry, GitStatus};
use crate::{
    git_operation::{run_git_command, GitBudget},
    git_repository::GitRepository,
    project_files::GuardedPath,
};
use serde::Deserialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

const MAX_INDEX_BYTES: u64 = 32 * 1024 * 1024;

fn read_index(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取 Git 索引: {error}")),
        Ok(_) => {}
    }
    let _guard = GuardedPath::pin(path, false)?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_INDEX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err("Git 索引超过 32 MiB，未执行写入".into());
    }
    Ok(Some(bytes))
}

// Git mutates a private index while the real index.lock excludes cooperating writers.
// Only commit publishes it; command failure, cancellation and Drop leave the real index intact.
pub(crate) struct IndexTransaction {
    index: PathBuf,
    lock_path: PathBuf,
    lock: Option<File>,
    directory: Option<PathBuf>,
    temporary_index: PathBuf,
    temporary_guard: Option<GuardedPath>,
    _metadata: GuardedPath,
    published: bool,
}

impl IndexTransaction {
    pub(crate) fn begin(repo: &GitRepository) -> Result<Self, String> {
        repo.budget.check()?;
        let metadata = GuardedPath::pin(repo.git_dir(), true)?;
        let lock_path = repo.git_dir().join("index.lock");
        let lock = OpenOptions::new().write(true).create_new(true).open(&lock_path)
            .map_err(|error| format!("无法取得 Git 索引锁，未执行写入；请检查其他 Git 操作（不会删除已有锁）: {error}"))?;
        let directory = repo
            .git_dir()
            .join(format!("deeppi-index-{}", uuid::Uuid::new_v4()));
        let mut transaction = Self {
            index: repo.git_dir().join("index"),
            lock_path,
            lock: Some(lock),
            temporary_index: directory.join("index"),
            directory: None,
            temporary_guard: None,
            _metadata: metadata,
            published: false,
        };
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        transaction.directory = Some(directory.clone());
        transaction.temporary_guard = Some(GuardedPath::pin(&directory, true)?);
        if let Some(bytes) = read_index(&transaction.index)? {
            let mut temporary = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&transaction.temporary_index)
                .map_err(|error| error.to_string())?;
            temporary
                .write_all(&bytes)
                .map_err(|error| error.to_string())?;
        }
        Ok(transaction)
    }

    pub(crate) fn run(&self, repo: &GitRepository, args: &[&str]) -> Result<(), String> {
        let output = repo.budget.run(self.command(repo).args(args))?;
        if !output.status.success() || output.truncated {
            return Err(format!(
                "Git 索引操作未完成（退出码 {:?}），真实索引未更新；请检查文件状态或过滤器",
                output.status.code()
            ));
        }
        Ok(())
    }

    pub(crate) fn command(&self, repo: &GitRepository) -> std::process::Command {
        let mut command = repo.command();
        command.env("GIT_INDEX_FILE", &self.temporary_index).args([
            "-c",
            "core.splitIndex=false",
            "-c",
            "index.sparse=false",
            "-c",
            "core.hooksPath=/dev/null",
        ]);
        command
    }

    pub(crate) fn input_file(&self, name: &str, bytes: &[u8]) -> Result<GuardedPath, String> {
        if name != "commit-message" {
            return Err("Invalid transaction input".into());
        }
        let directory = self
            .directory
            .as_ref()
            .ok_or("Index transaction has no directory")?;
        let path = directory.join(name);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        drop(file);
        GuardedPath::pin(&path, false)
    }

    pub(crate) fn commit(mut self, budget: &GitBudget) -> Result<(), String> {
        budget.check()?;
        let bytes = read_index(&self.temporary_index)?.ok_or("Git 未生成临时索引，未执行写入")?;
        let lock = self.lock.as_mut().ok_or("Git 索引事务已结束")?;
        lock.write_all(&bytes)
            .and_then(|()| lock.sync_all())
            .map_err(|error| error.to_string())?;
        budget.check()?;
        // No cancellable work follows this atomic publication point.
        fs::rename(&self.lock_path, &self.index)
            .map_err(|error| format!("无法发布 Git 索引，原索引未替换: {error}"))?;
        self.published = true;
        Ok(())
    }
}

impl Drop for IndexTransaction {
    fn drop(&mut self) {
        self.lock.take();
        if !self.published {
            let _ = fs::remove_file(&self.lock_path);
        }
        if let Some(directory) = &self.directory {
            let _ = fs::remove_file(&self.temporary_index);
            let _ = fs::remove_file(directory.join("index.lock"));
            let _ = fs::remove_file(directory.join("commit-message"));
            self.temporary_guard.take();
            let _ = fs::remove_dir(directory);
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum IndexAction {
    Stage,
    Unstage,
    Resolve,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexRequest {
    entry: GitEntry,
    action: IndexAction,
}

fn selected_paths(
    repo: &GitRepository,
    request: &IndexRequest,
    status: &GitStatus,
) -> Result<Vec<String>, String> {
    let entry = &request.entry;
    let path = repo.to_repo_path(&entry.path)?;
    let current = status
        .entries
        .iter()
        .find(|current| current.path == entry.path)
        .ok_or("文件状态已变化，请刷新并重新选择，不会自动重试写入")?;
    if current.source_outside_project {
        return Err("重命名跨越项目边界，请从完整仓库项目中操作".into());
    }
    if current != entry {
        return Err("文件状态已变化，请刷新并重新选择，不会自动重试写入".into());
    }
    let eligible = match request.action {
        IndexAction::Stage => {
            entry.kind == EntryKind::Untracked
                || entry.kind == EntryKind::Tracked && entry.worktree_status != '.'
        }
        IndexAction::Unstage => entry.kind == EntryKind::Tracked && entry.index_status != '.',
        IndexAction::Resolve => entry.kind == EntryKind::Conflict,
    };
    if !eligible {
        return Err("文件不属于此操作对应的变更分组，请刷新后重试".into());
    }
    let mut paths = vec![path];
    if request.action == IndexAction::Unstage && entry.index_status == 'R' {
        paths.push(repo.to_repo_path(entry.original_path.as_deref().ok_or("重命名缺少源路径")?)?);
    }
    Ok(paths)
}

fn change_index(repo: &GitRepository, request: &IndexRequest) -> Result<(), String> {
    let transaction = IndexTransaction::begin(repo)?;
    let status = read_status_for_index(repo)?;
    let paths = selected_paths(repo, request, &status)?;
    let mut held = None;
    let mut args = match request.action {
        IndexAction::Unstage => match status.oid.as_deref() {
            Some(oid) => vec!["reset", "-q", oid, "--"],
            None => vec!["update-index", "--force-remove", "--"],
        },
        IndexAction::Stage | IndexAction::Resolve => {
            let absolute = repo.project.path.join(&request.entry.path);
            match fs::symlink_metadata(&absolute) {
                Ok(metadata) => {
                    if metadata.len() > 8 * 1024 * 1024 {
                        return Err("工作区文件超过 8 MiB，未执行暂存".into());
                    }
                    held = Some(GuardedPath::open(
                        &repo.project.path,
                        &request.entry.path,
                        false,
                    )?);
                    vec!["add", "--"]
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if request.entry.kind == EntryKind::Untracked {
                        return Err("新文件已消失，请刷新后重试".into());
                    }
                    // Remove only this index entry, even if a directory is recreated at its path.
                    vec!["update-index", "--force-remove", "--"]
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    };
    args.extend(paths.iter().map(String::as_str));
    transaction.run(repo, &args)?;
    transaction.commit(&repo.budget)?;
    drop(held);
    Ok(())
}

#[tauri::command]
pub async fn project_git_change_index(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    request: IndexRequest,
    operation_id: String,
) -> Result<bool, String> {
    if webview.label() != "main" {
        return Err("Git 写入需要主窗口".into());
    }
    // 确认弹窗在阻塞线程内仍需 AppHandle，因此保留一份再移交。
    let dialog_app = app.clone();
    run_git_command(app, project_id, operation_id, false, move |repo| {
        let paths = selected_paths(repo, &request, &read_status_for_index(repo)?)?;
        let (title, detail) = match request.action {
            IndexAction::Stage => (
                "暂存所选文件",
                "将执行时的工作区内容加入暂存区，不修改工作区文件。",
            ),
            IndexAction::Unstage => (
                "取消所选文件暂存",
                "将所选索引路径恢复到当前提交，保留工作区文件。",
            ),
            IndexAction::Resolve => (
                "标记冲突已解决",
                "以执行时的工作区内容替换冲突索引；缺失文件按删除处理。请先确认冲突已解决。",
            ),
        };
        let confirmed = dialog_app
            .dialog()
            .message(format!(
                "{}\n\n工作树：{}\n\n{}\n\nGit 会使用已信任的仓库配置和文件过滤器。",
                detail,
                repo.worktree.path.display(),
                paths.join("\n"),
            ))
            .title(title)
            .buttons(MessageDialogButtons::OkCancelCustom(
                title.into(),
                "取消".into(),
            ))
            .blocking_show();
        if !confirmed {
            return Ok(false);
        }
        change_index(repo, &request)?;
        Ok(true)
    })
    .await
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::git_repository::GitRepository;
    use crate::git_test_support::TestRepo;
    use std::fs;

    #[test]
    fn publishes_only_on_commit_and_preserves_unrelated_staging() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("existing.txt"), "already staged\n").unwrap();
        fs::write(fixture.root.join("-中文 file.txt"), "selected\n").unwrap();
        fixture.git(&["add", "--", "existing.txt"]);
        let index = fixture.root.join(".git/index");
        let before = fs::read(&index).unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let transaction = IndexTransaction::begin(&repo).unwrap();
        transaction
            .run(&repo, &["add", "--", "-中文 file.txt"])
            .unwrap();
        assert_eq!(fs::read(&index).unwrap(), before);
        transaction.commit(&repo.budget).unwrap();
        let output =
            crate::git_repository::run_git(repo.command().args(["ls-files", "-z"])).unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "-中文 file.txt\0existing.txt\0"
        );
        assert!(!fixture.root.join(".git/index.lock").exists());
    }

    #[test]
    fn dropping_or_cancelling_transaction_leaves_real_index_unchanged() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        let before = fs::read(fixture.root.join(".git/index")).unwrap();
        fs::write(fixture.root.join("file.txt"), "changed\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        {
            let transaction = IndexTransaction::begin(&repo).unwrap();
            transaction.run(&repo, &["add", "--", "file.txt"]).unwrap();
        }
        assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), before);
        let transaction = IndexTransaction::begin(&repo).unwrap();
        transaction.run(&repo, &["add", "--", "file.txt"]).unwrap();
        repo.budget.token.cancel();
        assert!(transaction.commit(&repo.budget).is_err());
        assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), before);
        assert!(!fixture.root.join(".git/index.lock").exists());
    }

    #[test]
    fn existing_lock_is_never_removed_or_overwritten() {
        let fixture = TestRepo::new();
        let path = fixture.root.join(".git/index.lock");
        fs::write(&path, "another Git process").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        assert!(IndexTransaction::begin(&repo).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "another Git process");
    }

    #[test]
    fn initializes_missing_index_and_cleans_failed_commands() {
        let fixture = TestRepo::new();
        let repo = GitRepository::open(&fixture.root).unwrap();
        {
            let transaction = IndexTransaction::begin(&repo).unwrap();
            assert!(transaction
                .run(&repo, &["add", "--", "missing.txt"])
                .is_err());
        }
        assert!(!fixture.root.join(".git/index").exists());
        assert!(!fixture.root.join(".git/index.lock").exists());
        assert!(!fs::read_dir(fixture.root.join(".git"))
            .unwrap()
            .any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("deeppi-index-")
            }));
    }

    #[test]
    fn blocks_other_git_writers_while_transaction_is_active() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "content\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let transaction = IndexTransaction::begin(&repo).unwrap();
        let output =
            crate::git_repository::run_git(&mut fixture.command(&["add", "--", "file.txt"]))
                .unwrap();
        assert!(!output.status.success());
        drop(transaction);
        fixture.git(&["add", "--", "file.txt"]);
    }

    #[test]
    fn linked_worktree_uses_its_own_index() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        let before = fs::read(fixture.root.join(".git/index")).unwrap();
        let linked = fixture.base.join("linked");
        fixture.git(&["worktree", "add", "-b", "feature", linked.to_str().unwrap()]);
        fs::write(linked.join("file.txt"), "linked change\n").unwrap();
        let repo = GitRepository::open(&linked).unwrap();
        let transaction = IndexTransaction::begin(&repo).unwrap();
        transaction.run(&repo, &["add", "--", "file.txt"]).unwrap();
        transaction.commit(&repo.budget).unwrap();
        assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), before);
        let output =
            crate::git_repository::run_git(repo.command().args(["diff", "--cached"])).unwrap();
        assert!(output.text().contains("+linked change"));
    }

    fn request(repo: &GitRepository, path: &str, action: IndexAction) -> IndexRequest {
        let entry = crate::git_status::read_status(repo)
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.path == path)
            .unwrap();
        IndexRequest { entry, action }
    }

    #[test]
    fn stages_and_unstages_new_file_without_touching_worktree() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("new.txt"), "first\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        change_index(&repo, &request(&repo, "new.txt", IndexAction::Stage)).unwrap();
        assert_eq!(
            crate::git_status::read_status(&repo).unwrap().entries[0].index_status,
            'A'
        );
        fs::write(fixture.root.join("new.txt"), "second\n").unwrap();
        change_index(&repo, &request(&repo, "new.txt", IndexAction::Unstage)).unwrap();
        assert_eq!(
            crate::git_status::read_status(&repo).unwrap().entries[0].kind,
            crate::git_status::EntryKind::Untracked
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("new.txt")).unwrap(),
            "second\n"
        );
    }

    #[test]
    fn unstages_rename_pair_and_preserves_outside_project_staging() {
        let fixture = TestRepo::new();
        fs::create_dir(fixture.root.join("nested")).unwrap();
        fs::write(fixture.root.join("nested/old.txt"), "same content\n").unwrap();
        fixture.git(&["add", "--", "nested/old.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fixture.git(&["mv", "--", "nested/old.txt", "nested/new.txt"]);
        fs::write(fixture.root.join("outside.txt"), "user staged\n").unwrap();
        fixture.git(&["add", "--", "outside.txt"]);
        let repo = GitRepository::open(&fixture.root.join("nested")).unwrap();
        change_index(&repo, &request(&repo, "new.txt", IndexAction::Unstage)).unwrap();
        let output = crate::git_repository::run_git(repo.command().args([
            "diff",
            "--cached",
            "--name-only",
        ]))
        .unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "outside.txt"
        );
        assert!(!fixture.root.join("nested/old.txt").exists());
        assert!(fixture.root.join("nested/new.txt").exists());
    }

    #[test]
    fn stages_deleted_file_and_rejects_stale_or_cross_boundary_selection() {
        let fixture = TestRepo::new();
        fs::create_dir(fixture.root.join("nested")).unwrap();
        fs::write(fixture.root.join("outside.txt"), "same content\n").unwrap();
        fixture.git(&["add", "--", "outside.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fixture.git(&["mv", "--", "outside.txt", "nested/moved.txt"]);
        let repo = GitRepository::open(&fixture.root.join("nested")).unwrap();
        let cross = request(&repo, "moved.txt", IndexAction::Unstage);
        assert!(change_index(&repo, &cross).unwrap_err().contains("边界"));
        drop(repo);
        fixture.git(&["commit", "-m", "move"]);
        fs::remove_file(fixture.root.join("nested/moved.txt")).unwrap();
        let repo = GitRepository::open(&fixture.root.join("nested")).unwrap();
        let deleted = request(&repo, "moved.txt", IndexAction::Stage);
        change_index(&repo, &deleted).unwrap();
        assert!(change_index(&repo, &deleted).is_err());
        assert_eq!(
            crate::git_status::read_status(&repo).unwrap().entries[0].index_status,
            'D'
        );
    }

    #[test]
    fn split_index_preserves_entries_and_can_be_unstaged() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("one.txt"), "one\n").unwrap();
        fs::write(fixture.root.join("two.txt"), "two\n").unwrap();
        fixture.git(&["add", "--", "one.txt", "two.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fixture.git(&["update-index", "--split-index"]);
        fs::write(fixture.root.join("one.txt"), "changed\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        change_index(&repo, &request(&repo, "one.txt", IndexAction::Stage)).unwrap();
        change_index(&repo, &request(&repo, "one.txt", IndexAction::Unstage)).unwrap();
        let output =
            crate::git_repository::run_git(repo.command().args(["ls-files", "-z"])).unwrap();
        assert_eq!(output.stdout, b"one.txt\0two.txt\0");
        assert_eq!(
            crate::git_status::read_status(&repo).unwrap().entries[0].index_status,
            '.'
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join("one.txt")).unwrap(),
            "changed\n"
        );
    }

    #[test]
    fn directory_replacement_never_recursively_stages_new_contents() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fs::remove_file(fixture.root.join("file.txt")).unwrap();
        fs::create_dir(fixture.root.join("file.txt")).unwrap();
        fs::write(fixture.root.join("file.txt/secret.txt"), "not selected\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let before = fs::read(fixture.root.join(".git/index")).unwrap();
        assert!(change_index(&repo, &request(&repo, "file.txt", IndexAction::Stage)).is_err());
        assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), before);
    }

    #[test]
    fn conflicts_require_explicit_resolution_action() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fixture.git(&["checkout", "-b", "feature"]);
        fs::write(fixture.root.join("file.txt"), "theirs\n").unwrap();
        fixture.git(&["commit", "-am", "theirs"]);
        fixture.git(&["checkout", "main"]);
        fs::write(fixture.root.join("file.txt"), "ours\n").unwrap();
        fixture.git(&["commit", "-am", "ours"]);
        let merge = crate::git_repository::run_git(&mut fixture.command(&[
            "merge",
            "--no-edit",
            "feature",
        ]))
        .unwrap();
        assert_eq!(merge.status.code(), Some(1));
        let before = fs::read(fixture.root.join(".git/index")).unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        assert!(change_index(&repo, &request(&repo, "file.txt", IndexAction::Stage)).is_err());
        assert!(change_index(&repo, &request(&repo, "file.txt", IndexAction::Unstage)).is_err());
        assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), before);
        fs::write(fixture.root.join("file.txt"), "resolved\n").unwrap();
        change_index(&repo, &request(&repo, "file.txt", IndexAction::Resolve)).unwrap();
        let output =
            crate::git_repository::run_git(repo.command().args(["ls-files", "-u"])).unwrap();
        assert!(output.stdout.is_empty());
        assert_eq!(
            fs::read_to_string(fixture.root.join("file.txt")).unwrap(),
            "resolved\n"
        );
    }

    #[test]
    fn index_transaction_does_not_run_repository_hooks() {
        let fixture = TestRepo::new();
        let hooks = fixture.root.join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        fs::write(
            hooks.join("post-index-change"),
            "#!/bin/sh\nprintf hook > hook-ran.txt\n",
        )
        .unwrap();
        fs::write(fixture.root.join("file.txt"), "content\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        change_index(&repo, &request(&repo, "file.txt", IndexAction::Stage)).unwrap();
        assert!(!fixture.root.join("hook-ran.txt").exists());
    }
}
