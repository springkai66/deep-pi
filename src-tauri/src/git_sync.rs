use crate::{
    git_push::{check_destination, isolated_command, verify_remote, PushConfig, PushPreview},
    git_repository::{with_repository, GitRepository},
    project_files::GuardedPath,
};
use serde::Serialize;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::Stdio,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackingRef {
    reference: String,
    old_oid: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
struct SyncPreview {
    oid: String,
    references: Vec<TrackingRef>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncOutcome {
    Synced,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    outcome: SyncOutcome,
    oid: String,
    references: Vec<String>,
}

fn pattern_match<'a>(pattern: &str, target: &'a str) -> Option<&'a str> {
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        target.strip_prefix(prefix)?.strip_suffix(suffix)
    } else {
        (pattern == target).then_some("")
    }
}

fn mapped_refs(specs: &[String], target: &str) -> Result<Vec<String>, String> {
    let mut refs = Vec::new();
    let mut excluded = false;
    for spec in specs {
        if spec.chars().any(char::is_whitespace) || spec.chars().any(char::is_control) {
            return Err("不支持的 fetch 引用映射".into());
        }
        if let Some(negative) = spec.strip_prefix('^') {
            if !negative.starts_with("refs/")
                || negative.contains(':')
                || negative.matches('*').count() > 1
            {
                return Err("负 fetch 引用映射无效".into());
            }
            excluded |= pattern_match(negative, target).is_some();
            continue;
        }
        let positive = spec.strip_prefix('+').unwrap_or(spec);
        let (source, destination) = positive.split_once(':').unwrap_or((positive, ""));
        if !source.starts_with("refs/")
            || source.matches('*').count() > 1
            || source.matches('*').count() != destination.matches('*').count()
        {
            return Err("不支持或不完整的 fetch 引用映射".into());
        }
        if let Some(matched) = pattern_match(source, target) {
            if destination.is_empty() {
                continue;
            }
            let reference = destination.replace('*', matched);
            if !reference.starts_with("refs/remotes/") {
                return Err("fetch 映射不是远程跟踪引用，不能从此入口改写本地分支或标签".into());
            }
            if !refs.contains(&reference) {
                refs.push(reference);
            }
            if refs.len() > 64 {
                return Err("远程跟踪映射超过 64 个".into());
            }
        }
    }
    refs.sort();
    Ok(if excluded { Vec::new() } else { refs })
}

fn read_text(repo: &GitRepository, args: &[&str], optional: bool) -> Result<String, String> {
    let output = repo.budget.run(repo.command().args(args))?;
    if output.truncated {
        return Err("Git 同步配置或引用输出超限".into());
    }
    if optional && output.status.code() == Some(1) {
        return Ok(String::new());
    }
    if !output.status.success() {
        return Err("无法读取 Git 同步配置或引用".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "Git 同步配置或引用不是 UTF-8".into())
}

fn mapping(repo: &GitRepository, target: &PushPreview) -> Result<Vec<String>, String> {
    let urls = read_text(repo, &["remote", "get-url", "--all", &target.remote], false)?;
    // Git fetch uses the first fetch URL, whereas push may have multiple destinations.
    if urls.lines().next() != Some(target.destination.as_str()) {
        return Err("所选推送目的地与该远程的拉取地址不同，不能更新它的跟踪引用".into());
    }
    let specs = read_text(
        repo,
        &[
            "config",
            "--get-all",
            &format!("remote.{}.fetch", target.remote),
        ],
        true,
    )?;
    let references = mapped_refs(
        &specs.lines().map(String::from).collect::<Vec<_>>(),
        &target.target_ref,
    )?;
    if references.is_empty() {
        return Err("目标分支没有可用的远程跟踪映射，可能被负 refspec 排除".into());
    }
    for reference in &references {
        read_text(repo, &["check-ref-format", reference], false)?;
    }
    Ok(references)
}

fn direct_ref(repo: &GitRepository, reference: &str) -> Result<Option<String>, String> {
    let symbolic = read_text(repo, &["symbolic-ref", "--quiet", reference], true)?;
    if !symbolic.is_empty() {
        return Err("跟踪引用已是符号引用，未改写其目标分支".into());
    }
    let oid = read_text(repo, &["rev-parse", "--verify", "--quiet", reference], true)?;
    let oid = oid.trim_end_matches(['\n', '\r']);
    if oid.is_empty() {
        return Ok(None);
    }
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("跟踪引用的对象 ID 无效".into());
    }
    Ok(Some(oid.into()))
}

fn tracking_refs(repo: &GitRepository, target: &PushPreview) -> Result<Vec<TrackingRef>, String> {
    mapping(repo, target)?
        .into_iter()
        .map(|reference| {
            let old_oid = direct_ref(repo, &reference)?;
            Ok(TrackingRef { reference, old_oid })
        })
        .collect()
}

fn prepare(repo: &GitRepository, target: &PushPreview) -> Result<SyncPreview, String> {
    let references = tracking_refs(repo, target)?;
    let oid = verify_remote(repo, target)?
        .remote_oid
        .ok_or("远程查询未返回目标分支，不会删除本地跟踪引用")?;
    Ok(SyncPreview { oid, references })
}

fn acknowledgement(
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
            .ok_or("跟踪引用事务超时")?;
        match receiver.recv_timeout(remaining.min(Duration::from_millis(20))) {
            Ok(line) if line.trim_end() == expected => return Ok(()),
            Ok(_) => return Err("跟踪引用事务响应异常".into()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err("跟踪引用事务提前退出".into()),
        }
    }
}

fn publish(
    repo: &GitRepository,
    target: &PushPreview,
    preview: &SyncPreview,
) -> Result<SyncResult, String> {
    let mut command = repo.command();
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "update-ref",
            "--no-deref",
            "--stdin",
            "--create-reflog",
            "-m",
            "fetch: DeepPi tracking sync",
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
            .write_all(b"start\n")
            .map_err(|error| error.to_string())?;
        for reference in &preview.references {
            let old = reference
                .old_oid
                .clone()
                .unwrap_or_else(|| "0".repeat(preview.oid.len()));
            writeln!(
                input,
                "update {} {} {}",
                reference.reference, preview.oid, old
            )
            .map_err(|error| error.to_string())?;
        }
        input
            .write_all(b"prepare\n")
            .and_then(|()| input.flush())
            .map_err(|error| error.to_string())?;
        acknowledgement(&receiver, "start: ok", deadline, Some(&repo.budget.token))?;
        acknowledgement(&receiver, "prepare: ok", deadline, Some(&repo.budget.token))?;
        // Under Git's prepared locks, reject a symbolic ref even if its resolved OID matched CAS.
        if tracking_refs(repo, target)? != preview.references {
            return Err("跟踪引用或映射已变化，事务已中止".into());
        }
        repo.budget.check()?;
        repo.budget.token.commit()?;
        attempted = true;
        input
            .write_all(b"commit\n")
            .and_then(|()| input.flush())
            .map_err(|error| error.to_string())?;
        acknowledgement(
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
            Ok(None) => break Err("跟踪引用事务未及时退出".to_string()),
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
        return Err(operation.err().unwrap_or_else(|| "未发布跟踪引用".into()));
    }
    let outcome = if operation.is_ok() && exit.is_ok_and(|status| status.success()) {
        SyncOutcome::Synced
    } else {
        SyncOutcome::Unknown
    };
    Ok(SyncResult {
        outcome,
        oid: preview.oid.clone(),
        references: preview
            .references
            .iter()
            .map(|value| value.reference.clone())
            .collect(),
    })
}

fn synchronize(
    repo: &GitRepository,
    target: &PushPreview,
    expected: &SyncPreview,
) -> Result<SyncResult, String> {
    if prepare(repo, target)? != *expected {
        return Err("远程目标、映射或本地跟踪引用已变化，请重新确认同步".into());
    }
    let _local = if Path::new(&target.destination).is_absolute() {
        Some(GuardedPath::pin(Path::new(&target.destination), true)?)
    } else {
        None
    };
    let config = PushConfig::new(repo, &target.destination)?;
    check_destination(repo, &config, &target.destination)?;
    let output = crate::process_runner::run_cancellable(
        isolated_command(repo, &config).args([
            "-c",
            "fetch.writeCommitGraph=false",
            "fetch",
            "--no-tags",
            "--no-prune",
            "--no-prune-tags",
            "--no-recurse-submodules",
            "--no-write-fetch-head",
            "--no-auto-maintenance",
            "--refmap=",
            "--",
            &config.remote,
            &expected.oid,
        ]),
        repo.budget.remaining()?.min(Duration::from_secs(120)),
        Some(&repo.budget.token),
    )?;
    if !output.status.success() || output.truncated {
        return Err("获取所选提交对象失败或输出超限，未发布跟踪引用".into());
    }
    let kind = read_text(
        repo,
        &["--no-replace-objects", "cat-file", "-t", &expected.oid],
        false,
    )?;
    if kind.trim() != "commit" {
        return Err("远程目标不是提交对象，未发布跟踪引用".into());
    }
    publish(repo, target, expected)
}

#[tauri::command]
pub async fn project_git_sync_tracking(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
    expected: PushPreview,
) -> Result<Option<SyncResult>, String> {
    if webview.label() != "main" {
        return Err("同步远程跟踪引用需要主窗口".into());
    }
    let root = app
        .state::<crate::task::TaskStore>()
        .project_path(&project_id)?;
    let operation = app
        .state::<crate::git_operation::GitOperations>()
        .begin_with_budget(&operation_id, Duration::from_secs(180))?;
    tauri::async_runtime::spawn_blocking(move || {
        let running = operation;
        with_repository(&app, Path::new(&root), false, running.budget.clone(), |repo| {
            let preview = prepare(repo, &expected)?;
            let refs = preview.references.iter().map(|value| format!("{}: {}", value.reference,
                value.old_oid.as_deref().unwrap_or("(new)"))).collect::<Vec<_>>().join("\n");
            let confirmed = app.dialog().message(format!(
                "工作树：{}\n远程：{}\n目的地：{}\n目标分支：{}\n查询提交：{}\n\n本地跟踪引用：\n{}\n\n下载此提交的可达对象，并将以上引用更新到查询提交，包括远程回退。\n不合并、不变基本地分支，不修改工作区、索引、标签或上游设置；不运行客户端 hooks。\n远程查询未返回目标时不删除本地引用。使用现有凭据，不交互登录。",
                repo.worktree.path.display(), expected.remote, expected.destination, expected.target_ref, preview.oid, refs,
            )).title("确认同步远程跟踪引用")
                .buttons(MessageDialogButtons::OkCancelCustom("同步".into(), "取消".into())).blocking_show();
            if !confirmed { return Ok(None); }
            synchronize(repo, &expected, &preview).map(Some)
        })
    }).await.map_err(|error| error.to_string())?
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::{git_repository::GitRepository, git_test_support::TestRepo};
    use std::fs;

    fn git_text(fixture: &TestRepo, args: &[&str]) -> String {
        let output = crate::git_repository::run_git(&mut fixture.command(args)).unwrap();
        assert!(output.status.success(), "{}", output.text());
        String::from_utf8(output.stdout).unwrap()
    }

    fn fixture() -> (TestRepo, crate::git_push::PushPreview) {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "base\n").unwrap();
        fixture.git(&["add", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        let remote = fixture.base.join("remote.git");
        fixture.git(&[
            "init",
            "--bare",
            "--template=",
            "--initial-branch=main",
            remote.to_str().unwrap(),
        ]);
        fixture.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        fixture.git(&["push", "origin", "main:main"]);
        let oid = git_text(&fixture, &["rev-parse", "HEAD"]).trim().to_owned();
        let preview = serde_json::from_value(serde_json::json!({
            "remote": "origin", "destination": remote.to_str().unwrap(),
            "sourceRef": "refs/heads/main", "sourceOid": oid, "targetRef": "refs/heads/main",
        }))
        .unwrap();
        (fixture, preview)
    }

    #[test]
    fn maps_exact_wildcard_and_negative_fetch_specs_without_expanding_scope() {
        let specs = vec![
            "+refs/heads/*:refs/remotes/origin/*".into(),
            "refs/heads/main:refs/remotes/review/current".into(),
            "^refs/heads/private/*".into(),
        ];
        assert_eq!(
            mapped_refs(&specs, "refs/heads/main").unwrap(),
            vec!["refs/remotes/origin/main", "refs/remotes/review/current"]
        );
        assert!(mapped_refs(&specs, "refs/heads/private/secret")
            .unwrap()
            .is_empty());
        assert!(mapped_refs(&["refs/heads/*:refs/heads/*".into()], "refs/heads/main").is_err());
        assert!(mapped_refs(
            &["refs/heads/*:refs/remotes/origin/no-star".into()],
            "refs/heads/main"
        )
        .is_err());
    }

    #[test]
    fn synchronizes_only_tracking_refs_and_preserves_worktree_index_tags_and_fetch_head() {
        let (fixture, target) = fixture();
        fixture.git(&["update-ref", "-d", "refs/remotes/origin/main"]);
        fs::write(fixture.root.join("file.txt"), "local changes\n").unwrap();
        fixture.git(&["add", "file.txt"]);
        fs::write(fixture.root.join("file.txt"), "unstaged changes\n").unwrap();
        fs::write(fixture.root.join(".git/FETCH_HEAD"), "keep\n").unwrap();
        let index = fs::read(fixture.root.join(".git/index")).unwrap();
        let head = git_text(&fixture, &["rev-parse", "HEAD"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let snapshot = prepare(&repo, &target).unwrap();
        let result = synchronize(&repo, &target, &snapshot).unwrap();
        assert_eq!(result.outcome, SyncOutcome::Synced);
        assert_eq!(
            git_text(&fixture, &["rev-parse", "refs/remotes/origin/main"]),
            head
        );
        assert_eq!(git_text(&fixture, &["rev-parse", "HEAD"]), head);
        assert_eq!(fs::read(fixture.root.join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read_to_string(fixture.root.join("file.txt")).unwrap(),
            "unstaged changes\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.root.join(".git/FETCH_HEAD")).unwrap(),
            "keep\n"
        );
        assert!(git_text(&fixture, &["tag", "--list"]).is_empty());
    }

    #[test]
    fn refuses_different_fetch_destination_and_negative_mapping() {
        let (fixture, target) = fixture();
        fixture.git(&[
            "config",
            "remote.origin.url",
            fixture.base.join("other.git").to_str().unwrap(),
        ]);
        fixture.git(&[
            "config",
            "remote.origin.pushurl",
            target.destination.as_str(),
        ]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        assert!(prepare(&repo, &target).unwrap_err().contains("拉取地址"));
        fixture.git(&["config", "remote.origin.url", target.destination.as_str()]);
        fixture.git(&["config", "--add", "remote.origin.fetch", "^refs/heads/main"]);
        assert!(prepare(&repo, &target).unwrap_err().contains("映射"));
    }

    #[test]
    fn refuses_changed_tracking_ref_and_symbolic_tracking_ref_without_touching_branch() {
        let (fixture, target) = fixture();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let snapshot = prepare(&repo, &target).unwrap();
        fixture.git(&["commit", "--allow-empty", "-m", "next"]);
        let head = git_text(&fixture, &["rev-parse", "HEAD"]);
        fixture.git(&["update-ref", "refs/remotes/origin/main", head.trim()]);
        assert!(synchronize(&repo, &target, &snapshot).is_err());
        fixture.git(&[
            "symbolic-ref",
            "refs/remotes/origin/main",
            "refs/heads/main",
        ]);
        assert!(prepare(&repo, &target).is_err());
        assert_eq!(git_text(&fixture, &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn downloads_a_new_remote_commit_and_updates_custom_mappings_without_other_refs() {
        let (fixture, target) = fixture();
        let other = TestRepo::new();
        other.git(&["fetch", target.destination.as_str(), "main"]);
        other.git(&["reset", "--hard", "FETCH_HEAD"]);
        fs::write(other.root.join("remote-only.txt"), "remote content\n").unwrap();
        other.git(&["add", "remote-only.txt"]);
        other.git(&["commit", "-m", "remote advance"]);
        other.git(&["tag", "-a", "remote-tag", "-m", "not selected"]);
        other.git(&["push", target.destination.as_str(), "main", "--tags"]);
        let new_oid = git_text(&other, &["rev-parse", "HEAD"]);
        let old_head = git_text(&fixture, &["rev-parse", "HEAD"]);
        fixture.git(&[
            "config",
            "--add",
            "remote.origin.fetch",
            "refs/heads/main:refs/remotes/review/current",
        ]);
        fixture.git(&["config", "fetch.pruneTags", "true"]);
        fixture.git(&["config", "fetch.prune", "true"]);
        fixture.git(&["tag", "local-tag"]);
        fs::create_dir_all(fixture.root.join(".git/hooks")).unwrap();
        fs::write(
            fixture.root.join(".git/hooks/reference-transaction"),
            "#!/bin/sh\nprintf invoked > sync-hook-ran.txt\nexit 1\n",
        )
        .unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare(&repo, &target).unwrap();
        assert_eq!(preview.oid, new_oid.trim());
        assert_eq!(
            synchronize(&repo, &target, &preview).unwrap().outcome,
            SyncOutcome::Synced
        );
        assert_eq!(
            git_text(&fixture, &["rev-parse", "refs/remotes/origin/main"]),
            new_oid
        );
        assert_eq!(
            git_text(&fixture, &["rev-parse", "refs/remotes/review/current"]),
            new_oid
        );
        assert_eq!(git_text(&fixture, &["rev-parse", "HEAD"]), old_head);
        assert_eq!(git_text(&fixture, &["tag", "--list"]), "local-tag\n");
        assert!(!fixture.root.join(".git/FETCH_HEAD").exists());
        assert!(!fixture.root.join("remote-only.txt").exists());
        assert!(!fixture.root.join("sync-hook-ran.txt").exists());
    }

    #[test]
    fn prepared_transaction_rejects_a_symbolic_ref_resolving_to_the_same_old_oid() {
        let (fixture, target) = fixture();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare(&repo, &target).unwrap();
        fixture.git(&[
            "symbolic-ref",
            "refs/remotes/origin/main",
            "refs/heads/main",
        ]);
        assert!(publish(&repo, &target, &preview)
            .unwrap_err()
            .contains("符号引用"));
        assert_eq!(
            git_text(&fixture, &["symbolic-ref", "refs/remotes/origin/main"]),
            "refs/heads/main\n"
        );
        assert_eq!(
            git_text(&fixture, &["rev-parse", "HEAD"]).trim(),
            preview.oid
        );
        assert!(!fixture
            .root
            .join(".git/refs/remotes/origin/main.lock")
            .exists());
    }

    #[test]
    fn an_existing_lock_prevents_all_tracking_updates_and_is_not_deleted() {
        let (fixture, target) = fixture();
        fixture.git(&[
            "config",
            "--add",
            "remote.origin.fetch",
            "refs/heads/main:refs/remotes/review/current",
        ]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare(&repo, &target).unwrap();
        let lock = fixture.root.join(".git/refs/remotes/origin/main.lock");
        fs::write(&lock, "external lock").unwrap();
        assert!(publish(&repo, &target, &preview).is_err());
        assert_eq!(fs::read_to_string(&lock).unwrap(), "external lock");
        assert!(!fixture
            .root
            .join(".git/refs/remotes/review/current")
            .exists());
    }

    #[test]
    fn missing_remote_and_cancelled_sync_never_delete_tracking_refs() {
        let (fixture, target) = fixture();
        let old = git_text(&fixture, &["rev-parse", "refs/remotes/origin/main"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare(&repo, &target).unwrap();
        repo.budget.token.cancel();
        assert!(synchronize(&repo, &target, &preview).is_err());
        fixture.git(&[
            "--git-dir",
            target.destination.as_str(),
            "update-ref",
            "-d",
            "refs/heads/main",
        ]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        assert!(prepare(&repo, &target).unwrap_err().contains("未返回"));
        assert_eq!(
            git_text(&fixture, &["rev-parse", "refs/remotes/origin/main"]),
            old
        );
    }

    #[test]
    fn linked_worktree_updates_shared_tracking_refs_without_modifying_either_head() {
        let (fixture, target) = fixture();
        let linked = fixture.base.join("linked worktree");
        fixture.git(&["worktree", "add", "-b", "feature", linked.to_str().unwrap()]);
        fixture.git(&["update-ref", "-d", "refs/remotes/origin/main"]);
        let main_head = fs::read(fixture.root.join(".git/HEAD")).unwrap();
        let repo = GitRepository::open(&linked).unwrap();
        let linked_head = fs::read(repo.git_dir().join("HEAD")).unwrap();
        let preview = prepare(&repo, &target).unwrap();
        assert_eq!(
            synchronize(&repo, &target, &preview).unwrap().outcome,
            SyncOutcome::Synced
        );
        assert_eq!(
            git_text(&fixture, &["rev-parse", "refs/remotes/origin/main"]).trim(),
            preview.oid
        );
        assert_eq!(fs::read(fixture.root.join(".git/HEAD")).unwrap(), main_head);
        assert_eq!(fs::read(repo.git_dir().join("HEAD")).unwrap(), linked_head);
        assert!(!repo.git_dir().join("FETCH_HEAD").exists());
        assert!(!fixture.root.join(".git/FETCH_HEAD").exists());
    }
}
