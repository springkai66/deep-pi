use crate::message::msg;
use crate::{
    dialog_text::ConfirmDialog, git_index::IndexTransaction, git_operation::run_git_command,
    git_repository::GitRepository, project_files::GuardedPath,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitPreview {
    reference: String,
    head: Option<String>,
    tree: String,
    paths: Vec<String>,
    author: String,
    committer: String,
    sign: bool,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommitOutcome {
    Committed,
    NotCommitted,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    outcome: CommitOutcome,
    oid: String,
    detail: String,
}

fn command(repo: &GitRepository) -> Command {
    let mut command = repo.command();
    command.env("GIT_NO_REPLACE_OBJECTS", "1");
    command
}

fn output_text(
    repo: &GitRepository,
    command: &mut Command,
    failure: &str,
) -> Result<String, String> {
    let output = repo.budget.run(command)?;
    if !output.status.success() || output.truncated {
        return Err(failure.into());
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim_end_matches(['\r', '\n']).into())
        .map_err(|_| msg("git.commit.unsupported_encoding"))
}

fn oid(text: String) -> Result<String, String> {
    if !matches!(text.len(), 40 | 64) || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(msg("git.commit.invalid_oid"));
    }
    Ok(text)
}

fn ensure_normal_commit(repo: &GitRepository) -> Result<(), String> {
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        match fs::symlink_metadata(repo.git_dir().join(name)) {
            Ok(_) => return Err(msg("git.commit.sequence_in_progress")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

pub(crate) fn head_state(repo: &GitRepository) -> Result<(String, Option<String>), String> {
    let symbolic = repo
        .budget
        .run(command(repo).args(["symbolic-ref", "--quiet", "HEAD"]))?;
    if symbolic.truncated {
        return Err(msg("git.commit.head_too_large"));
    }
    let reference = if symbolic.status.success() {
        let value =
            String::from_utf8(symbolic.stdout).map_err(|_| msg("git.commit.branch_encoding"))?;
        let value = value.trim_end_matches(['\r', '\n']).to_owned();
        if !value.starts_with("refs/heads/") || value.chars().any(char::is_whitespace) {
            return Err(msg("git.commit.unsupported_head"));
        }
        value
    } else if symbolic.status.code() == Some(1) {
        "HEAD".into()
    } else {
        return Err(msg("git.commit.head_read_failed"));
    };
    let resolved =
        repo.budget
            .run(command(repo).args(["rev-parse", "--verify", "--quiet", "HEAD"]))?;
    if resolved.truncated {
        return Err(msg("git.commit.head_too_large"));
    }
    let head = if resolved.status.success() {
        Some(oid(String::from_utf8(resolved.stdout)
            .map_err(|_| "Invalid HEAD")?
            .trim()
            .into())?)
    } else if resolved.status.code() == Some(1) && reference != "HEAD" {
        None
    } else {
        return Err(msg("git.commit.head_parse_failed"));
    };
    Ok((reference, head))
}

fn ident(repo: &GitRepository, variable: &str) -> Result<String, String> {
    let text = output_text(
        repo,
        command(repo).args(["var", variable]),
        &msg("git.commit.identity_missing"),
    )?;
    text.rsplitn(3, ' ')
        .nth(2)
        .filter(|value| !value.is_empty())
        .map(String::from)
        .ok_or_else(|| msg("git.commit.identity_invalid"))
}

fn snapshot(repo: &GitRepository, index: &IndexTransaction) -> Result<CommitPreview, String> {
    ensure_normal_commit(repo)?;
    let (reference, head) = head_state(repo)?;
    let tree = oid(output_text(
        repo,
        index
            .command(repo)
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .arg("write-tree"),
        &msg("git.commit.tree_failed"),
    )?)?;
    let mut diff = index.command(repo);
    diff.env("GIT_NO_REPLACE_OBJECTS", "1").args([
        "diff",
        "--cached",
        "--name-only",
        "-z",
        "--no-renames",
        "--no-ext-diff",
        "--no-textconv",
        "--no-relative",
        "--ignore-submodules=none",
    ]);
    if let Some(head) = &head {
        diff.arg(head);
    }
    diff.arg("--");
    let output = repo.budget.run(&mut diff)?;
    if !output.status.success() || output.truncated {
        return Err(msg("git.commit.staged_range_failed"));
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| msg("git.commit.paths_encoding"))?;
    if !text.is_empty() && !text.ends_with('\0') {
        return Err(msg("git.commit.paths_incomplete"));
    }
    let mut paths = Vec::new();
    for path in text.split_terminator('\0') {
        let relative = repo
            .to_project_path(path)
            .ok_or(msg("git.commit.paths_outside_scope"))?;
        repo.to_repo_path(relative)?;
        paths.push(relative.into());
    }
    if paths.is_empty() {
        return Err(msg("git.commit.no_staged"));
    }
    let signing = repo.budget.run(command(repo).args([
        "config",
        "--type=bool",
        "--get",
        "commit.gpgSign",
    ]))?;
    let sign = if signing.status.code() == Some(1) {
        false
    } else if signing.status.success() && !signing.truncated {
        match signing.stdout.as_slice() {
            b"true\n" => true,
            b"false\n" => false,
            _ => return Err(msg("git.commit.signing_invalid")),
        }
    } else {
        return Err(msg("git.commit.signing_read_failed"));
    };
    let author = ident(repo, "GIT_AUTHOR_IDENT")?;
    let committer = ident(repo, "GIT_COMMITTER_IDENT")?;
    if head_state(repo)? != (reference.clone(), head.clone()) {
        return Err(msg("git.commit.head_changed"));
    }
    Ok(CommitPreview {
        reference,
        head,
        tree,
        paths,
        author,
        committer,
        sign,
    })
}

fn prepare_commit(repo: &GitRepository) -> Result<CommitPreview, String> {
    snapshot(repo, &IndexTransaction::begin(repo)?)
}

fn validate_message(message: &str) -> Result<(), String> {
    if message.trim().is_empty() || message.len() > 64 * 1024 || message.contains('\0') {
        return Err(msg("git.commit.message_invalid"));
    }
    Ok(())
}

fn reference_command(repo: &GitRepository, preview: &CommitPreview, new_oid: &str) -> Command {
    let old = preview
        .head
        .clone()
        .unwrap_or_else(|| "0".repeat(new_oid.len()));
    let mut command = command(repo);
    command.args([
        "-c",
        "core.hooksPath=/dev/null",
        "update-ref",
        "--create-reflog",
        "-m",
        "commit: DeepPi",
        "--no-deref",
        &preview.reference,
        new_oid,
        &old,
    ]);
    command
}

fn publish_commit(
    repo: &GitRepository,
    preview: &CommitPreview,
    new_oid: String,
) -> Result<CommitResult, String> {
    if preview.reference == "HEAD" {
        return publish_detached(repo, preview, new_oid);
    }
    let _head = if preview.reference != "HEAD" {
        Some(GuardedPath::pin(&repo.git_dir().join("HEAD"), false)?)
    } else {
        None
    };
    if head_state(repo)? != (preview.reference.clone(), preview.head.clone()) {
        return Err(msg("git.commit.head_changed_before_publish"));
    }
    repo.budget.check()?;
    repo.budget.token.commit()?;
    // Reference publication is not interruptible by a late user cancellation.
    let result = crate::process_runner::run_cancellable(
        &mut reference_command(repo, preview, &new_oid),
        Duration::from_secs(10),
        None,
    );
    reconcile_publication(repo, preview, new_oid, result)
}

fn response(
    receiver: &mpsc::Receiver<String>,
    expected: &str,
    deadline: Instant,
    cancellation: Option<&crate::operation::Cancellation>,
) -> Result<(), String> {
    loop {
        if let Some(token) = cancellation {
            token.check()?;
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(msg("git.commit.transaction_timeout"))?;
        match receiver.recv_timeout(remaining.min(Duration::from_millis(20))) {
            Ok(line) if line.trim_end() == expected => return Ok(()),
            Ok(_) => return Err(msg("git.commit.transaction_response")),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(msg("git.commit.transaction_exited"))
            }
        }
    }
}

// A plain update-ref CAS also accepts a symbolic HEAD resolving to the old OID.
// Prepare first so Git holds HEAD.lock, then verify the raw HEAD is still detached.
fn publish_detached(
    repo: &GitRepository,
    preview: &CommitPreview,
    new_oid: String,
) -> Result<CommitResult, String> {
    let old = preview
        .head
        .as_deref()
        .ok_or(msg("git.commit.detached_head_missing"))?;
    let mut command = command(repo);
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "update-ref",
            "--no-deref",
            "--stdin",
            "--create-reflog",
            "-m",
            "commit: DeepPi",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    repo.budget.check()?;
    let (mut child, tree) = crate::process_runner::spawn_owned(&mut command)?;
    let mut input = child.stdin.take().expect("piped stdin");
    let output = child.stdout.take().expect("piped stdout");
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        // Only four short protocol acknowledgements are expected.
        for line in BufReader::new(output.take(1024)).lines() {
            let Ok(line) = line else {
                break;
            };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut attempted = false;
    let operation: Result<(), String> = (|| {
        let deadline = Instant::now() + repo.budget.remaining()?.min(Duration::from_secs(10));
        input
            .write_all(format!("start\nupdate HEAD {new_oid} {old}\nprepare\n").as_bytes())
            .and_then(|()| input.flush())
            .map_err(|error| error.to_string())?;
        response(&receiver, "start: ok", deadline, Some(&repo.budget.token))?;
        response(&receiver, "prepare: ok", deadline, Some(&repo.budget.token))?;
        let head = GuardedPath::pin(&repo.git_dir().join("HEAD"), false)?;
        let mut text = String::new();
        fs::File::open(&head.path)
            .map_err(|error| error.to_string())?
            .take(256)
            .read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
        if text.trim() != old {
            return Err(msg("git.commit.head_changed_abort"));
        }
        drop(head);
        repo.budget.check()?;
        repo.budget.token.commit()?;
        attempted = true;
        input
            .write_all(b"commit\n")
            .and_then(|()| input.flush())
            .map_err(|error| error.to_string())?;
        response(
            &receiver,
            "commit: ok",
            Instant::now() + Duration::from_secs(10),
            None,
        )
    })();
    if !attempted {
        let _ = input.write_all(b"abort\n");
        let _ = input.flush();
    }
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(5);
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => break Err(msg("git.commit.transaction_not_exited")),
            Err(error) => break Err(error.to_string()),
        }
    };
    drop(tree);
    if exit.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = reader.join();
    if !attempted {
        return Err(operation
            .err()
            .unwrap_or_else(|| msg("git.commit.reference_not_published")));
    }
    let result = operation
        .and(exit)
        .map(|status| crate::process_runner::ProcessOutput {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
            truncated: false,
        });
    reconcile_publication(repo, preview, new_oid, result)
}

fn reconcile_publication(
    repo: &GitRepository,
    preview: &CommitPreview,
    new_oid: String,
    result: Result<crate::process_runner::ProcessOutput, String>,
) -> Result<CommitResult, String> {
    if result.as_ref().is_ok_and(|output| output.status.success()) {
        return Ok(CommitResult {
            outcome: CommitOutcome::Committed,
            oid: new_oid,
            detail: String::new(),
        });
    }
    #[cfg(test)]
    eprintln!(
        "Reference transaction failure: {:?}",
        result.as_ref().map(|output| output.text())
    );
    // An IPC/process failure is not proof that a durable reference update failed.
    let observed = crate::process_runner::run_cancellable(
        command(repo).args(["rev-parse", "--verify", "--quiet", &preview.reference]),
        Duration::from_secs(5),
        None,
    );
    if observed.as_ref().is_ok_and(|output| {
        output.status.success()
            && !output.truncated
            && std::str::from_utf8(&output.stdout).is_ok_and(|text| text.trim() == new_oid)
    }) {
        return Ok(CommitResult {
            outcome: CommitOutcome::Committed,
            oid: new_oid,
            detail: msg("git.commit.detail_verified_after_error"),
        });
    }
    let outcome = if result.is_ok() && observed.is_ok() {
        CommitOutcome::NotCommitted
    } else {
        CommitOutcome::Unknown
    };
    Ok(CommitResult {
        outcome,
        oid: new_oid,
        detail: msg("git.commit.detail_unconfirmed"),
    })
}

fn commit_staged(
    repo: &GitRepository,
    expected: &CommitPreview,
    message: &str,
) -> Result<CommitResult, String> {
    validate_message(message)?;
    let index = IndexTransaction::begin(repo)?;
    let preview = snapshot(repo, &index)?;
    if &preview != expected {
        return Err(msg("git.commit.staged_changed"));
    }
    let input = index.input_file("commit-message", message.as_bytes())?;
    let mut command = command(repo);
    command.args([
        "-c",
        "core.hooksPath=/dev/null",
        "commit-tree",
        &preview.tree,
    ]);
    if let Some(head) = &preview.head {
        command.args(["-p", head]);
    }
    command
        .arg(if preview.sign { "-S" } else { "--no-gpg-sign" })
        .arg("-F")
        .arg(&input.path);
    let new_oid = oid(output_text(
        repo,
        &mut command,
        &msg("git.commit.object_failed"),
    )?)?;
    ensure_normal_commit(repo)?;
    publish_commit(repo, &preview, new_oid)
}

/// 暂存范围摘要：最多列出 12 条路径，超出部分用前端按当前语言传入的词汇模板。
fn commit_scope(dialog: &ConfirmDialog, paths: &[String]) -> String {
    let mut scope = paths
        .iter()
        .take(12)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if paths.len() > 12 {
        scope.push('\n');
        scope.push_str(&dialog.render_term("more_paths", &[("count", paths.len().to_string())]));
    }
    scope
}

#[tauri::command]
pub async fn project_git_prepare_commit(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
) -> Result<CommitPreview, String> {
    if webview.label() != "main" {
        return Err(msg("git.commit.prepare_window_required"));
    }
    run_git_command(app, project_id, operation_id, None, prepare_commit).await
}

#[tauri::command]
pub async fn project_git_commit(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
    expected: CommitPreview,
    dialog: ConfirmDialog,
    message: String,
) -> Result<Option<CommitResult>, String> {
    if webview.label() != "main" {
        return Err(msg("git.commit.window_required"));
    }
    dialog.validate()?;
    validate_message(&message)?;
    // 确认弹窗在阻塞线程内仍需 AppHandle，因此保留一份再移交。
    let dialog_app = app.clone();
    run_git_command(app, project_id, operation_id, None, move |repo| {
        if prepare_commit(repo)? != expected {
            return Err(msg("git.commit.scope_changed"));
        }
        let summary: String = message.chars().take(600).collect();
        let confirmed = dialog_app
            .dialog()
            .message(
                dialog.render(&[
                    ("worktree", repo.worktree.path.display().to_string()),
                    ("target", expected.reference.clone()),
                    ("author", expected.author.clone()),
                    (
                        "signature",
                        dialog
                            .term(if expected.sign {
                                "signature_on"
                            } else {
                                "signature_off"
                            })
                            .to_owned(),
                    ),
                    ("tree", expected.tree.clone()),
                    ("paths", commit_scope(&dialog, &expected.paths)),
                    ("summary", summary),
                ]),
            )
            .title(dialog.title.clone())
            .buttons(MessageDialogButtons::OkCancelCustom(
                dialog.confirm_label.clone(),
                dialog.cancel_label.clone(),
            ))
            .blocking_show();
        if !confirmed {
            return Ok(None);
        }
        commit_staged(repo, &expected, &message).map(Some)
    })
    .await
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::git_repository::{run_git, GitRepository};
    use crate::git_test_support::TestRepo;
    use std::fs;

    #[test]
    fn commits_exact_staged_tree_and_keeps_worktree_and_index_bytes() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("中文 file.txt"), "staged\n").unwrap();
        fixture.git(&["add", "--", "中文 file.txt"]);
        let index = fs::read(fixture.root.join(".git/index")).unwrap();
        fs::write(fixture.root.join("中文 file.txt"), "unstaged\n").unwrap();
        fs::write(fixture.root.join("not-selected.txt"), "untracked\n").unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_commit(&repo).unwrap();
        assert_eq!(preview.paths, ["中文 file.txt"]);
        let result = commit_staged(&repo, &preview, "首次提交\n\n保持工作区内容").unwrap();
        assert_eq!(result.outcome, CommitOutcome::Committed);
        let content = run_git(repo.command().args(["show", "HEAD:中文 file.txt"])).unwrap();
        assert_eq!(content.stdout, b"staged\n");
        let missing = run_git(
            repo.command()
                .args(["cat-file", "-e", "HEAD:not-selected.txt"]),
        )
        .unwrap();
        assert!(!missing.status.success());
        assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read_to_string(fixture.root.join("中文 file.txt")).unwrap(),
            "unstaged\n"
        );
        let message = run_git(repo.command().args(["log", "-1", "--format=%B"])).unwrap();
        assert!(String::from_utf8(message.stdout)
            .unwrap()
            .contains("首次提交\n\n保持工作区内容"));
    }

    #[test]
    fn rejects_changed_staged_content_even_when_status_letters_match() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "first\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_commit(&repo).unwrap();
        fs::write(fixture.root.join("file.txt"), "second\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        assert!(commit_staged(&repo, &preview, "must not commit stale selection").is_err());
        assert!(
            !run_git(repo.command().args(["rev-parse", "--verify", "HEAD"]))
                .unwrap()
                .status
                .success()
        );
    }

    #[test]
    fn nested_project_rejects_staging_outside_its_scope() {
        let fixture = TestRepo::new();
        fs::create_dir(fixture.root.join("nested")).unwrap();
        fs::write(fixture.root.join("nested/inside.txt"), "inside\n").unwrap();
        fs::write(fixture.root.join("outside.txt"), "outside\n").unwrap();
        fixture.git(&["add", "--", "nested/inside.txt", "outside.txt"]);
        let repo = GitRepository::open(&fixture.root.join("nested")).unwrap();
        assert!(prepare_commit(&repo)
            .unwrap_err()
            .contains("git.commit.paths_outside_scope"));
    }

    #[test]
    fn rejects_empty_message_empty_staging_and_in_progress_sequences() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "content\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_commit(&repo).unwrap();
        assert!(commit_staged(&repo, &preview, " \n ").is_err());
        assert!(commit_staged(&repo, &preview, "bad\0message").is_err());
        fs::write(fixture.root.join(".git/MERGE_HEAD"), "in progress\n").unwrap();
        assert!(prepare_commit(&repo).is_err());
        fs::remove_file(fixture.root.join(".git/MERGE_HEAD")).unwrap();
        commit_staged(&repo, &preview, "base").unwrap();
        assert!(prepare_commit(&repo)
            .unwrap_err()
            .contains("git.commit.no_staged"));
    }

    #[test]
    fn detached_head_commit_does_not_move_original_branch() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fixture.git(&["checkout", "--detach"]);
        fs::write(fixture.root.join("file.txt"), "change\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_commit(&repo).unwrap();
        assert_eq!(preview.reference, "HEAD");
        let result = commit_staged(&repo, &preview, "detached change").unwrap();
        assert_eq!(result.outcome, CommitOutcome::Committed);
        let branch = run_git(repo.command().args(["rev-parse", "refs/heads/main"])).unwrap();
        assert_eq!(
            String::from_utf8(branch.stdout).unwrap().trim(),
            preview.head.as_deref().unwrap()
        );
    }

    #[test]
    fn rejects_branch_movement_after_preview_and_preserves_new_head() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fs::write(fixture.root.join("file.txt"), "selected\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_commit(&repo).unwrap();
        fixture.git(&["commit", "-m", "external"]);
        fs::write(fixture.root.join("file.txt"), "after external\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        let external = head_state(&repo).unwrap();
        assert!(commit_staged(&repo, &preview, "must not overwrite external").is_err());
        assert_eq!(head_state(&repo).unwrap(), external);
    }

    #[test]
    fn signing_failure_does_not_publish_unsigned_commit() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "content\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["config", "commit.gpgsign", "true"]);
        fixture.git(&["config", "gpg.program", "deeppi-nonexistent-signer.exe"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_commit(&repo).unwrap();
        assert!(preview.sign);
        assert!(commit_staged(&repo, &preview, "signed or nothing").is_err());
        assert!(head_state(&repo).unwrap().1.is_none());
    }

    #[test]
    fn reference_hook_cannot_change_or_veto_the_confirmed_commit() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "content\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fs::create_dir_all(fixture.root.join(".git/hooks")).unwrap();
        fs::write(
            fixture.root.join(".git/hooks/reference-transaction"),
            "#!/bin/sh\nprintf invoked > hook-marker.txt\nexit 1\n",
        )
        .unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_commit(&repo).unwrap();
        assert_eq!(
            commit_staged(&repo, &preview, "exact scope")
                .unwrap()
                .outcome,
            CommitOutcome::Committed
        );
        assert!(!fixture.root.join("hook-marker.txt").exists());
    }

    #[test]
    fn hidden_submodule_pointer_changes_cannot_escape_scope_confirmation() {
        let fixture = TestRepo::new();
        fs::create_dir(fixture.root.join("nested")).unwrap();
        fs::write(fixture.root.join("nested/file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "nested/file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        let output = run_git(&mut fixture.command(&["rev-parse", "HEAD"])).unwrap();
        let head = String::from_utf8(output.stdout).unwrap();
        fixture.git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},outside-module", head.trim()),
        ]);
        fixture.git(&["config", "diff.ignoreSubmodules", "all"]);
        fs::write(fixture.root.join("nested/file.txt"), "inside change\n").unwrap();
        fixture.git(&["add", "--", "nested/file.txt"]);
        let repo = GitRepository::open(&fixture.root.join("nested")).unwrap();
        assert!(prepare_commit(&repo).is_err());
    }

    #[test]
    fn replacement_objects_cannot_hide_outside_project_staging() {
        let fixture = TestRepo::new();
        fs::create_dir(fixture.root.join("nested")).unwrap();
        fs::write(fixture.root.join("nested/file.txt"), "inside base\n").unwrap();
        fs::write(fixture.root.join("outside.txt"), "outside base\n").unwrap();
        fixture.git(&["add", "--", "nested/file.txt", "outside.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fs::write(fixture.root.join("outside.txt"), "outside changed\n").unwrap();
        fixture.git(&["commit", "-am", "replacement"]);
        let output = run_git(&mut fixture.command(&["rev-parse", "HEAD"])).unwrap();
        let replacement = String::from_utf8(output.stdout).unwrap();
        fixture.git(&["reset", "--soft", "HEAD^"]);
        fixture.git(&["replace", "HEAD", replacement.trim()]);
        fs::write(fixture.root.join("nested/file.txt"), "inside changed\n").unwrap();
        fixture.git(&["add", "--", "nested/file.txt"]);
        let repo = GitRepository::open(&fixture.root.join("nested")).unwrap();
        assert!(prepare_commit(&repo).is_err());
    }

    #[test]
    fn reference_publication_rejects_detached_head_becoming_symbolic() {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        fixture.git(&["checkout", "--detach"]);
        fs::write(fixture.root.join("file.txt"), "selected\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_commit(&repo).unwrap();
        let candidate = output_text(
            &repo,
            command(&repo).args([
                "commit-tree",
                &preview.tree,
                "-p",
                preview.head.as_deref().unwrap(),
                "-m",
                "candidate",
            ]),
            "candidate failed",
        )
        .unwrap();
        fixture.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        assert!(
            publish_detached(&repo, &preview, candidate).is_err(),
            "Publication must not overwrite a newly symbolic HEAD"
        );
        assert_eq!(head_state(&repo).unwrap().0, "refs/heads/main");
        assert!(!repo.git_dir().join("HEAD.lock").exists());
    }
}

/// 纯逻辑对话框测试：不调用 Git、不依赖临时仓库，任何平台都能编译执行。
#[cfg(test)]
mod dialog_tests {
    use super::*;
    use crate::dialog_text::ConfirmDialog;

    fn scope_dialog(more_paths: &str) -> ConfirmDialog {
        ConfirmDialog {
            title: "commit title".into(),
            message: "{paths}".into(),
            confirm_label: "ok".into(),
            cancel_label: "cancel".into(),
            terms: std::collections::BTreeMap::from([(
                "more_paths".to_owned(),
                more_paths.to_owned(),
            )]),
        }
    }

    #[test]
    fn lists_at_most_twelve_paths_and_renders_the_overflow_term() {
        let dialog = scope_dialog("and {count} more");
        let listed = (0..12)
            .map(|index| format!("path-{index}.txt"))
            .collect::<Vec<_>>();
        assert_eq!(commit_scope(&dialog, &listed), listed.join("\n"));

        let mut thirteen = listed.clone();
        thirteen.push("path-12.txt".into());
        assert_eq!(
            commit_scope(&dialog, &thirteen),
            format!("{}\nand 13 more", listed.join("\n"))
        );
    }
}
