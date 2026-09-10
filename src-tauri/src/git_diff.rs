use crate::{
    git_repository::{with_repository, GitRepository},
    git_status::{read_status, EntryKind},
    project_files::GuardedPath,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use tauri::Manager;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffArea {
    Staged,
    Unstaged,
    Untracked,
    Conflict,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ConflictSide {
    Base,
    #[default]
    Ours,
    Theirs,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRequest {
    path: String,
    area: DiffArea,
    #[serde(default)]
    conflict_side: ConflictSide,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum DiffFormat {
    Text,
    Binary,
    UnsupportedEncoding,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    path: String,
    area: DiffArea,
    patch: String,
    format: DiffFormat,
    truncated: bool,
    source_outside_project: bool,
}

fn pin_worktree_entry(repo: &GitRepository, relative: &str) -> Result<GuardedPath, String> {
    let path = repo.project.path.join(relative);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.len() > 8 * 1024 * 1024 {
                return Err("工作区文件超过 8 MiB，请在外部 Git 工具中查看".into());
            }
            GuardedPath::open(&repo.project.path, relative, false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Deleted files may also have deleted parent directories.
            let mut parent = Path::new(relative).parent();
            while let Some(relative_parent) = parent {
                let absolute = repo.project.path.join(relative_parent);
                match fs::symlink_metadata(&absolute) {
                    Ok(_) => {
                        return GuardedPath::open(
                            &repo.project.path,
                            relative_parent.to_str().ok_or("Invalid path")?,
                            true,
                        )
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.to_string()),
                }
                parent = relative_parent.parent();
            }
            GuardedPath::open(&repo.project.path, "", true)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn read_diff(repo: &GitRepository, request: DiffRequest) -> Result<GitDiff, String> {
    let path = repo.to_repo_path(&request.path)?;
    let status = read_status(repo)?;
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path == request.path)
        .ok_or("文件状态已变化，请刷新变更列表")?;
    let eligible = match request.area {
        DiffArea::Staged => entry.kind == EntryKind::Tracked && entry.index_status != '.',
        DiffArea::Unstaged => entry.kind == EntryKind::Tracked && entry.worktree_status != '.',
        DiffArea::Untracked => entry.kind == EntryKind::Untracked,
        DiffArea::Conflict => entry.kind == EntryKind::Conflict,
    };
    if !eligible {
        return Err("文件已不属于所选变更分组，请刷新后重试".into());
    }
    let _file = if request.area != DiffArea::Staged {
        Some(pin_worktree_entry(repo, &request.path)?)
    } else {
        None
    };
    let mut command = repo.command();
    command.args([
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-relative",
        "--unified=3",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--ignore-submodules=all",
    ]);
    match request.area {
        DiffArea::Staged => {
            command.arg("--cached");
            command.arg(if entry.source_outside_project {
                "--no-renames"
            } else {
                "--find-renames"
            });
        }
        DiffArea::Conflict => {
            command
                .arg(match request.conflict_side {
                    ConflictSide::Base => "--base",
                    ConflictSide::Ours => "--ours",
                    ConflictSide::Theirs => "--theirs",
                })
                .arg("--no-renames");
        }
        DiffArea::Untracked => {
            command.args(["--no-index", "--no-renames"]);
        }
        DiffArea::Unstaged => {
            command.arg("--no-renames");
        }
    }
    command.arg("--");
    if request.area == DiffArea::Untracked {
        command.arg("/dev/null");
    }
    command.arg(&path);
    if request.area == DiffArea::Staged {
        if let Some(original) = &entry.original_path {
            command.arg(repo.to_repo_path(original)?);
        }
    }
    let output = repo.budget.run(&mut command)?;
    if !output.status.success()
        && !(request.area == DiffArea::Untracked && output.status.code() == Some(1))
    {
        return Err(format!(
            "Git 差异读取失败（退出码 {:?}），请刷新或在可信终端检查仓库",
            output.status.code()
        ));
    }
    let (mut patch, mut format) = match std::str::from_utf8(&output.stdout) {
        Ok(text) => (text.to_owned(), DiffFormat::Text),
        Err(error) if output.truncated && error.error_len().is_none() => (
            std::str::from_utf8(&output.stdout[..error.valid_up_to()])
                .unwrap_or("")
                .into(),
            DiffFormat::Text,
        ),
        Err(_) => (String::new(), DiffFormat::UnsupportedEncoding),
    };
    if patch
        .lines()
        .any(|line| line.starts_with("Binary files ") && line.ends_with(" differ"))
    {
        format = DiffFormat::Binary;
        patch.clear();
    }
    Ok(GitDiff {
        path: request.path,
        area: request.area,
        patch,
        format,
        truncated: output.truncated,
        source_outside_project: entry.source_outside_project,
    })
}

#[tauri::command]
pub async fn project_git_diff(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    request: DiffRequest,
    operation_id: String,
) -> Result<GitDiff, String> {
    if webview.label() != "main" {
        return Err("Git 差异读取需要主窗口".into());
    }
    let root = app
        .state::<crate::task::TaskStore>()
        .project_path(&project_id)?;
    let operation = app
        .state::<crate::git_operation::GitOperations>()
        .begin(&operation_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let running = operation;
        with_repository(
            &app,
            Path::new(&root),
            false,
            running.budget.clone(),
            |repo| read_diff(repo, request),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::git_repository::run_git;
    use crate::git_test_support::TestRepo;
    use std::fs;

    fn request(path: &str, area: DiffArea) -> DiffRequest {
        DiffRequest {
            path: path.into(),
            area,
            conflict_side: ConflictSide::Ours,
        }
    }

    #[test]
    fn compares_untracked_staged_unborn_and_unstaged_without_writing_index() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("new 中文.txt"), "first\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let new = read_diff(&repo, request("new 中文.txt", DiffArea::Untracked)).unwrap();
        assert!(new.patch.contains("+first"), "{new:?}");
        assert!(!fixture.root.join(".git/index").exists());
        drop(repo);
        fixture.git(&["add", "--", "new 中文.txt"]);
        let index = fs::read(fixture.root.join(".git/index")).unwrap();
        fs::write(fixture.root.join("new 中文.txt"), "second\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let staged = read_diff(&repo, request("new 中文.txt", DiffArea::Staged)).unwrap();
        assert!(staged.patch.contains("+first"));
        let unstaged = read_diff(&repo, request("new 中文.txt", DiffArea::Unstaged)).unwrap();
        assert!(unstaged.patch.contains("-first"));
        assert!(unstaged.patch.contains("+second"));
        assert_eq!(index, fs::read(fixture.root.join(".git/index")).unwrap());
    }

    #[test]
    fn handles_empty_binary_deleted_and_stale_entries() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("empty"), "").unwrap();
        fs::write(fixture.root.join("binary"), [0, 1, 2, 3]).unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let empty = read_diff(&repo, request("empty", DiffArea::Untracked)).unwrap();
        assert!(empty.patch.contains("new file mode"), "{empty:?}");
        assert!(!empty.patch.contains("@@"));
        let binary = read_diff(&repo, request("binary", DiffArea::Untracked)).unwrap();
        assert_eq!(binary.format, DiffFormat::Binary);
        assert!(read_diff(&repo, request("empty", DiffArea::Staged)).is_err());
        drop(repo);
        fs::write(fixture.root.join("deleted"), "old\n").unwrap();
        fixture.git(&["add", "--", "deleted"]);
        fs::remove_file(fixture.root.join("deleted")).unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let deleted = read_diff(&repo, request("deleted", DiffArea::Unstaged)).unwrap();
        assert!(deleted.patch.contains("-old"));
    }

    #[test]
    fn shows_rename_pair_and_keeps_nested_project_scope() {
        let fixture = TestRepo::new();
        fs::create_dir(fixture.root.join("nested")).unwrap();
        fs::write(fixture.root.join("nested/old.txt"), "same contents\n").unwrap();
        fixture.git(&["add", "--", "nested/old.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fixture.git(&["mv", "--", "nested/old.txt", "nested/new.txt"]);
        fs::write(fixture.root.join("outside.txt"), "private outside\n").unwrap();
        let repo = GitRepository::open(&fixture.root.join("nested")).unwrap();
        let renamed = read_diff(&repo, request("new.txt", DiffArea::Staged)).unwrap();
        assert!(
            renamed.patch.contains("rename from nested/old.txt"),
            "{renamed:?}"
        );
        assert!(renamed.patch.contains("rename to nested/new.txt"));
        assert!(read_diff(&repo, request("../outside.txt", DiffArea::Untracked)).is_err());
        assert!(read_diff(&repo, request("outside.txt", DiffArea::Untracked)).is_err());
    }

    #[test]
    fn disables_external_diff_and_textconv_drivers() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join(".gitattributes"), "*.txt diff=unsafe\n").unwrap();
        fs::write(fixture.root.join("file.txt"), "old\n").unwrap();
        fixture.git(&["add", "--", ".gitattributes", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fixture.git(&[
            "config",
            "diff.unsafe.command",
            "this-executable-must-never-run",
        ]);
        fixture.git(&[
            "config",
            "diff.unsafe.textconv",
            "this-executable-must-never-run",
        ]);
        fs::write(fixture.root.join("file.txt"), "new\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let diff = read_diff(&repo, request("file.txt", DiffArea::Unstaged)).unwrap();
        assert!(diff.patch.contains("+new"), "{diff:?}");
    }

    #[test]
    fn compares_each_conflict_stage_with_worktree_without_resolving_it() {
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
        let merge = run_git(&mut fixture.command(&["merge", "--no-edit", "feature"])).unwrap();
        assert_eq!(merge.status.code(), Some(1));
        let index = fs::read(fixture.root.join(".git/index")).unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        for (side, expected) in [
            (ConflictSide::Base, "-base"),
            (ConflictSide::Ours, "+<<<<<<<"),
            (ConflictSide::Theirs, "+<<<<<<<"),
        ] {
            let diff = read_diff(
                &repo,
                DiffRequest {
                    path: "file.txt".into(),
                    area: DiffArea::Conflict,
                    conflict_side: side,
                },
            )
            .unwrap();
            assert!(diff.patch.contains(expected), "{diff:?}");
        }
        assert_eq!(index, fs::read(fixture.root.join(".git/index")).unwrap());
        assert_eq!(
            read_status(&repo).unwrap().entries[0].kind,
            EntryKind::Conflict
        );
    }

    #[test]
    fn reads_linked_worktree_diff_without_using_main_worktree_index() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        let linked = fixture.base.join("linked");
        fixture.git(&["worktree", "add", "-b", "feature", linked.to_str().unwrap()]);
        fs::write(linked.join("file.txt"), "linked change\n").unwrap();
        let repo = GitRepository::open(&linked).unwrap();
        let diff = read_diff(&repo, request("file.txt", DiffArea::Unstaged)).unwrap();
        assert!(diff.patch.contains("+linked change"));
        let main = GitRepository::open(&fixture.root).unwrap();
        assert!(read_status(&main).unwrap().entries.is_empty());
    }

    #[test]
    fn reports_bounded_patch_and_unsupported_encoding() {
        let fixture = TestRepo::new();
        fs::write(
            fixture.root.join("large.txt"),
            "a reasonably sized line\n".repeat(10_000),
        )
        .unwrap();
        fs::write(fixture.root.join("encoded.txt"), [b'h', b'i', 0xff, b'\n']).unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let large = read_diff(&repo, request("large.txt", DiffArea::Untracked)).unwrap();
        assert!(large.truncated);
        assert!(large.patch.len() <= 64 * 1024);
        let encoded = read_diff(&repo, request("encoded.txt", DiffArea::Untracked)).unwrap();
        assert_eq!(encoded.format, DiffFormat::UnsupportedEncoding);
        assert!(encoded.patch.is_empty());
    }
}
