pub use crate::git_repository::GitStatusGate;
use crate::message::{msg, msg_with};
use crate::{git_operation::run_git_command, git_repository::GitRepository};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum EntryKind {
    Tracked,
    Untracked,
    Conflict,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitEntry {
    pub(crate) path: String,
    pub(crate) original_path: Option<String>,
    pub(crate) source_outside_project: bool,
    pub(crate) index_status: char,
    pub(crate) worktree_status: char,
    pub(crate) kind: EntryKind,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    branch: Option<String>,
    upstream: Option<String>,
    pub(crate) oid: Option<String>,
    unborn: bool,
    detached: bool,
    ahead: u32,
    behind: u32,
    repository_root: String,
    project_prefix: String,
    pub(crate) entries: Vec<GitEntry>,
}

fn parse_status(data: &[u8]) -> Result<GitStatus, String> {
    if !data.is_empty() && data.last() != Some(&0) {
        return Err("Git status output is incomplete".into());
    }
    let text = std::str::from_utf8(data).map_err(|_| "Git returned a non-Unicode path")?;
    let mut records = text.split_terminator('\0');
    let mut result = GitStatus::default();
    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            if let Some(value) = header.strip_prefix("branch.oid ") {
                result.unborn = value == "(initial)";
                result.oid = (!result.unborn).then(|| value.into());
            } else if let Some(value) = header.strip_prefix("branch.head ") {
                result.detached = value == "(detached)";
                result.branch = (!result.detached).then(|| value.into());
            } else if let Some(value) = header.strip_prefix("branch.upstream ") {
                result.upstream = Some(value.into());
            } else if let Some(value) = header.strip_prefix("branch.ab ") {
                let (ahead, behind) = value.split_once(' ').ok_or("Invalid Git branch counts")?;
                result.ahead = ahead
                    .strip_prefix('+')
                    .ok_or("Invalid ahead count")?
                    .parse()
                    .map_err(|_| "Invalid ahead count")?;
                result.behind = behind
                    .strip_prefix('-')
                    .ok_or("Invalid behind count")?
                    .parse()
                    .map_err(|_| "Invalid behind count")?;
            }
            continue;
        }
        if let Some(path) = record.strip_prefix("? ") {
            if path.is_empty() {
                return Err("Empty Git path".into());
            }
            result.entries.push(GitEntry {
                path: path.into(),
                original_path: None,
                source_outside_project: false,
                index_status: '?',
                worktree_status: '?',
                kind: EntryKind::Untracked,
            });
            continue;
        }
        let (fields, kind, renamed) = match record.as_bytes().first() {
            Some(b'1') => (9, EntryKind::Tracked, false),
            Some(b'2') => (10, EntryKind::Tracked, true),
            Some(b'u') => (11, EntryKind::Conflict, false),
            _ => return Err("Unsupported Git status record".into()),
        };
        // splitn preserves spaces and newlines in the final pathname.
        let parts: Vec<_> = record.splitn(fields, ' ').collect();
        if parts.len() != fields
            || parts.iter().any(|part| part.is_empty())
            || parts[1].len() != 2
            || !parts[1].bytes().all(|ch| b".MADRCUT".contains(&ch))
        {
            return Err("Malformed Git status record".into());
        }
        let original_path = if renamed {
            Some(
                records
                    .next()
                    .filter(|path| !path.is_empty())
                    .ok_or("Missing Git rename source")?
                    .to_owned(),
            )
        } else {
            None
        };
        result.entries.push(GitEntry {
            path: parts[fields - 1].into(),
            original_path,
            source_outside_project: false,
            index_status: parts[1].as_bytes()[0] as char,
            worktree_status: parts[1].as_bytes()[1] as char,
            kind,
        });
    }
    Ok(result)
}

pub(crate) fn read_status(repo: &GitRepository) -> Result<GitStatus, String> {
    read_status_scoped(repo, false)
}

pub(crate) fn read_status_for_index(repo: &GitRepository) -> Result<GitStatus, String> {
    read_status_scoped(repo, true)
}

fn read_status_scoped(
    repo: &GitRepository,
    detect_cross_boundary: bool,
) -> Result<GitStatus, String> {
    let output = repo.budget.run(repo.command().args([
        "status",
        "--porcelain=v2",
        "-z",
        "--branch",
        "--untracked-files=all",
        "--ignore-submodules=all",
        "--",
        if detect_cross_boundary || repo.prefix.is_empty() {
            "."
        } else {
            &repo.prefix
        },
    ]))?;
    if !output.status.success() {
        return Err(msg_with(
            "git.status.read_failed",
            &[("code", &format!("{:?}", output.status.code()))],
        ));
    }
    if output.truncated {
        return Err(msg("git.status.truncated"));
    }
    let mut status = parse_status(&output.stdout)?;
    if detect_cross_boundary {
        status
            .entries
            .retain(|entry| repo.to_project_path(&entry.path).is_some());
    }
    for entry in &mut status.entries {
        entry.path = repo
            .to_project_path(&entry.path)
            .ok_or(msg("git.status.outside_scope"))?
            .into();
        if let Some(original) = &entry.original_path {
            match repo.to_project_path(original) {
                Some(path) => entry.original_path = Some(path.into()),
                None => {
                    entry.original_path = None;
                    entry.source_outside_project = true;
                }
            }
        }
    }
    status.repository_root = repo.worktree.path.to_string_lossy().into();
    status.project_prefix = repo.prefix.clone();
    Ok(status)
}

#[tauri::command]
pub async fn project_git_status(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    // 需要用户信任时展示的确认文案；由前端按当前语言传入，`None` 表示不请求信任。
    trust_dialog: Option<crate::dialog_text::ConfirmDialog>,
    operation_id: String,
) -> Result<GitStatus, String> {
    if webview.label() != "main" {
        return Err(msg("git.status.window_required"));
    }
    if let Some(dialog) = trust_dialog.as_ref() {
        dialog.validate()?;
    }
    run_git_command(app, project_id, operation_id, trust_dialog, read_status).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_staged_unstaged_and_literal_paths() {
        let data = b"# branch.oid abc123\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -3\x001 MM N... 100644 100644 100644 abc def src/a b.ts\0? -option.txt\0";
        let state = parse_status(data).unwrap();
        assert_eq!(state.branch.as_deref(), Some("main"));
        assert_eq!(state.upstream.as_deref(), Some("origin/main"));
        assert_eq!((state.ahead, state.behind), (2, 3));
        assert_eq!(state.entries[0].path, "src/a b.ts");
        assert_eq!(state.entries[0].index_status, 'M');
        assert_eq!(state.entries[0].worktree_status, 'M');
        assert_eq!(state.entries[1].path, "-option.txt");
        assert_eq!(state.entries[1].kind, EntryKind::Untracked);
    }

    #[test]
    fn handles_renames_conflicts_unicode_and_unborn_branch() {
        let data = "# branch.oid (initial)\0# branch.head main\x002 R. N... 100644 100644 100644 abc def R100 新 文件.txt\0旧 文件.txt\0u UU N... 100644 100644 100644 100644 a b c conflict.txt\0";
        let state = parse_status(data.as_bytes()).unwrap();
        assert!(state.unborn);
        assert_eq!(
            state.entries[0].original_path.as_deref(),
            Some("旧 文件.txt")
        );
        assert_eq!(state.entries[0].path, "新 文件.txt");
        assert_eq!(state.entries[1].kind, EntryKind::Conflict);
    }

    #[test]
    fn detached_clean_and_unknown_headers_are_supported() {
        let state =
            parse_status(b"# branch.oid abc\0# branch.head (detached)\0# future.header value\0")
                .unwrap();
        assert!(state.detached);
        assert!(state.branch.is_none());
        assert!(state.entries.is_empty());
    }

    #[test]
    fn rejects_truncated_malformed_and_non_unicode_records() {
        for data in [
            &b"? no terminator"[..],
            &b"1 M. too short\0"[..],
            &b"2 R. N... 1 1 1 a b R100 new\0"[..],
            &b"? \xff\0"[..],
            &b"# branch.ab +invalid -0\0"[..],
            &b"unexpected\0"[..],
            &b"? \0"[..],
        ] {
            assert!(parse_status(data).is_err(), "{data:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn reads_real_repo_without_refreshing_index_and_scopes_nested_project() {
        use std::fs;
        use std::{process::Command, time::Duration};
        let root = std::env::temp_dir().join(format!("deeppi-git-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|path| path.join("git.exe"))
            .find(|path| path.is_file())
            .unwrap();
        let run = |args: &[&str]| {
            let mut command = Command::new(&git);
            command.current_dir(&root).args(args);
            let output = crate::process_runner::run(&mut command, Duration::from_secs(60)).unwrap();
            assert!(output.status.success(), "{}", output.text());
        };
        run(&["init", "--template=", "--initial-branch=main"]);
        run(&["config", "core.autocrlf", "false"]);
        fs::write(root.join("中文 file.txt"), "first\n").unwrap();
        let initial = read_status(&GitRepository::open(&root).unwrap()).unwrap();
        assert!(initial.unborn);
        assert_eq!(initial.entries[0].kind, EntryKind::Untracked);
        run(&["add", "--", "中文 file.txt"]);
        let index = fs::read(root.join(".git/index")).unwrap();
        fs::write(root.join("中文 file.txt"), "second and longer\n").unwrap();
        let status = read_status(&GitRepository::open(&root).unwrap()).unwrap();
        assert_eq!(status.entries[0].index_status, 'A');
        assert_eq!(status.entries[0].worktree_status, 'M');
        assert_eq!(status.entries[0].path, "中文 file.txt");
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        fs::create_dir(root.join("child")).unwrap();
        assert!(
            read_status(&GitRepository::open(&root.join("child")).unwrap())
                .unwrap()
                .entries
                .is_empty()
        );
        assert!(!root.join(".git/index.lock").exists());
    }
}
