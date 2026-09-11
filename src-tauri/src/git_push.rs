use crate::{
    git_repository::{with_repository, GitRepository},
    project_files::GuardedPath,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PushTarget {
    remote: String,
    destination: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushTargets {
    source_ref: String,
    source_oid: String,
    targets: Vec<PushTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PushPreview {
    pub(crate) remote: String,
    pub(crate) destination: String,
    pub(crate) target_ref: String,
    source_ref: String,
    source_oid: String,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PushOutcome {
    Pushed,
    UpToDate,
    Rejected,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    outcome: PushOutcome,
    source_oid: String,
    target_ref: String,
    detail: String,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RemoteOutcome {
    Matches,
    Different,
    Missing,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteVerification {
    outcome: RemoteOutcome,
    pub(crate) remote_oid: Option<String>,
    source_oid: String,
    target_ref: String,
}

fn validate_destination(value: &str) -> Result<(), String> {
    let invalid = || {
        "不支持或包含敏感参数的推送地址；请使用无内嵌凭据的 HTTPS、SSH 或本地绝对路径".to_string()
    };
    if value.is_empty()
        || value.len() > 4096
        || value.chars().any(char::is_control)
        || value.starts_with('-')
        || value.starts_with("\\\\")
        || value.starts_with("//")
        || value.contains('?')
        || value.contains('#')
    {
        return Err(invalid());
    }
    if Path::new(value).is_absolute() {
        return if value.as_bytes().get(1) == Some(&b':') {
            Ok(())
        } else {
            Err(invalid())
        };
    }
    if value.starts_with("https://") || value.starts_with("ssh://") {
        let url = tauri::Url::parse(value).map_err(|_| invalid())?;
        if url.host_str().is_none()
            || url.password().is_some()
            || url.scheme() == "https" && !url.username().is_empty()
            || value.contains('\\')
            || value.chars().any(char::is_whitespace)
        {
            return Err(invalid());
        }
        return Ok(());
    }
    if !value.contains("://") && !value.contains("::") {
        if let Some((host, path)) = value.split_once(':') {
            if !host.is_empty()
                && !path.is_empty()
                && !path.starts_with('-')
                && host
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || b"@._-".contains(&ch))
                && !host.starts_with('-')
                && !value.chars().any(char::is_whitespace)
                && !value.contains('\\')
            {
                return Ok(());
            }
        }
    }
    Err(invalid())
}

fn text(repo: &GitRepository, command: &mut Command) -> Result<String, String> {
    let output = repo.budget.run(command)?;
    if !output.status.success() || output.truncated {
        return Err("无法完整读取 Git 推送配置".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "Git 推送配置不是有效 UTF-8".into())
}

fn configured_targets(repo: &GitRepository) -> Result<Vec<PushTarget>, String> {
    let names = text(repo, repo.command().arg("remote"))?;
    let mut targets = Vec::new();
    for remote in names.lines() {
        if remote.is_empty() || remote.starts_with('-') || remote.chars().any(char::is_control) {
            return Err("不支持的 Git 远程名称".into());
        }
        let urls = text(
            repo,
            repo.command()
                .args(["remote", "get-url", "--push", "--all", remote]),
        )?;
        for destination in urls.lines() {
            validate_destination(destination)?;
            let target = PushTarget {
                remote: remote.into(),
                destination: destination.into(),
            };
            if !targets.contains(&target) {
                targets.push(target);
            }
            if targets.len() > 64 {
                return Err("推送目的地超过 64 个，未展示不完整列表".into());
            }
        }
    }
    Ok(targets)
}

fn list_targets(repo: &GitRepository) -> Result<PushTargets, String> {
    let (source_ref, head) = crate::git_commit::head_state(repo)?;
    let source_oid = head.ok_or("仓库尚无提交，不能推送")?;
    Ok(PushTargets {
        source_ref,
        source_oid,
        targets: configured_targets(repo)?,
    })
}

fn prepare_push(
    repo: &GitRepository,
    remote: &str,
    destination: &str,
    branch: &str,
) -> Result<PushPreview, String> {
    validate_destination(destination)?;
    if branch.is_empty()
        || branch.len() > 512
        || branch.starts_with('-')
        || branch.starts_with("refs/")
        || branch.chars().any(char::is_control)
    {
        return Err("目标分支名称无效".into());
    }
    let target_ref = format!("refs/heads/{branch}");
    let check = repo
        .budget
        .run(repo.command().args(["check-ref-format", &target_ref]))?;
    if !check.status.success() {
        return Err("目标分支名称无效".into());
    }
    let targets = list_targets(repo)?;
    if !targets
        .targets
        .iter()
        .any(|target| target.remote == remote && target.destination == destination)
    {
        return Err("远程推送地址已变化，请重新加载并选择目的地".into());
    }
    Ok(PushPreview {
        remote: remote.into(),
        destination: destination.into(),
        target_ref,
        source_ref: targets.source_ref,
        source_oid: targets.source_oid,
    })
}

pub(crate) struct PushConfig {
    path: PathBuf,
    guard: Option<GuardedPath>,
    pub(crate) remote: String,
}

impl PushConfig {
    pub(crate) fn new(repo: &GitRepository, destination: &str) -> Result<Self, String> {
        let remote = format!("deeppi-{}", uuid::Uuid::new_v4().simple());
        let path = repo.git_dir().join(format!("{remote}.config"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| "无法创建临时推送配置")?;
        let mut config = Self {
            path,
            guard: None,
            remote,
        };
        let quoted = serde_json::to_string(destination).map_err(|_| "无法编码推送地址")?;
        file.write_all(format!("[remote \"{}\"]\nurl = {}\npushurl = {}\nmirror = false\nreceivepack = git-receive-pack\n",
            config.remote, quoted, quoted).as_bytes()).map_err(|_| "无法写入临时推送配置")?;
        drop(file);
        config.guard = Some(GuardedPath::pin(&config.path, false)?);
        Ok(config)
    }
}

impl Drop for PushConfig {
    fn drop(&mut self) {
        self.guard.take();
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn isolated_command(repo: &GitRepository, config: &PushConfig) -> Command {
    let mut command = repo.command();
    command
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_ALLOW_PROTOCOL", "https:ssh:file")
        .env("GCM_INTERACTIVE", "never")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "push.pushOption=",
            "-c",
            "push.followTags=false",
            "-c",
            "push.autoSetupRemote=false",
            "-c",
            "push.gpgSign=false",
            "-c",
            "http.followRedirects=false",
        ])
        .arg("-c")
        .arg(format!("include.path={}", config.path.display()));
    command
}

fn classify(output: &crate::process_runner::ProcessOutput, target: &str) -> PushOutcome {
    if output.truncated {
        return PushOutcome::Unknown;
    }
    let Ok(text) = std::str::from_utf8(&output.stdout) else {
        return PushOutcome::Unknown;
    };
    let mut flags = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.splitn(3, '\t').collect();
        if fields.len() == 3 && fields[0].len() == 1 {
            let Some((_, destination)) = fields[1].rsplit_once(':') else {
                return PushOutcome::Unknown;
            };
            if destination != target {
                return PushOutcome::Unknown;
            }
            flags.push(fields[0]);
        }
    }
    match flags.as_slice() {
        ["="] if output.status.success() => PushOutcome::UpToDate,
        [" " | "*"] if output.status.success() => PushOutcome::Pushed,
        ["!"] => PushOutcome::Rejected,
        _ => PushOutcome::Unknown,
    }
}

fn push_snapshot(repo: &GitRepository, expected: &PushPreview) -> Result<PushResult, String> {
    let branch = expected
        .target_ref
        .strip_prefix("refs/heads/")
        .ok_or("Invalid push target")?;
    if prepare_push(repo, &expected.remote, &expected.destination, branch)? != *expected {
        return Err("源提交、分支或推送目的地已变化，请重新确认".into());
    }
    let _local = if Path::new(&expected.destination).is_absolute() {
        Some(GuardedPath::pin(Path::new(&expected.destination), true)?)
    } else {
        None
    };
    let config = PushConfig::new(repo, &expected.destination)?;
    check_destination(repo, &config, &expected.destination)?;
    let mut command = isolated_command(repo, &config);
    command
        .args([
            "push",
            "--porcelain",
            "--no-force",
            "--no-follow-tags",
            "--no-signed",
            "--recurse-submodules=no",
            "--no-verify",
            "--receive-pack=git-receive-pack",
        ])
        .arg(&config.remote)
        .arg(format!("{}:{}", expected.source_oid, expected.target_ref));
    repo.budget.check()?;
    let output = crate::process_runner::run_cancellable(
        &mut command,
        Duration::from_secs(120),
        Some(&repo.budget.token),
    );
    let outcome = match &output {
        Ok(output) => classify(output, &expected.target_ref),
        Err(_) => PushOutcome::Unknown,
    };
    let detail = match outcome {
        PushOutcome::Pushed => "远程已接受所选提交。",
        PushOutcome::UpToDate => "远程目标已是所选提交。",
        PushOutcome::Rejected => "远程拒绝推送，请检查非快进、分支保护或服务器策略；未强推。",
        PushOutcome::Unknown => {
            "推送结果未确认。可能是认证、网络、取消或服务端错误；请核对远程目标，勿直接重复推送。"
        }
    };
    Ok(PushResult {
        outcome,
        source_oid: expected.source_oid.clone(),
        target_ref: expected.target_ref.clone(),
        detail: detail.into(),
    })
}

pub(crate) fn check_destination(
    repo: &GitRepository,
    config: &PushConfig,
    destination: &str,
) -> Result<(), String> {
    let resolved = text(
        repo,
        isolated_command(repo, config).args(["ls-remote", "--get-url", &config.remote]),
    )?;
    let push_urls = text(
        repo,
        isolated_command(repo, config).args([
            "config",
            "--get-all",
            &format!("remote.{}.pushurl", config.remote),
        ]),
    )?;
    if resolved.lines().collect::<Vec<_>>() != [destination] {
        return Err("地址重写改变了已确认的目的地，未执行推送".into());
    }
    if push_urls.lines().collect::<Vec<_>>() != [destination] {
        return Err("推送地址不再是唯一目的地，未执行推送".into());
    }
    Ok(())
}

fn parse_remote(bytes: &[u8], target: &str, source: &str) -> Result<RemoteVerification, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "远程响应不是有效 UTF-8")?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err("远程响应不完整，未确认目标状态".into());
    }
    let mut remote_oid = None;
    for line in text.lines() {
        let (oid, reference) = line.split_once('\t').ok_or("远程引用格式异常")?;
        if reference != target
            || remote_oid.is_some()
            || !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|ch| ch.is_ascii_hexdigit())
        {
            return Err("远程引用格式或范围异常".into());
        }
        remote_oid = Some(oid.to_owned());
    }
    let outcome = match remote_oid.as_deref() {
        Some(value) if value == source => RemoteOutcome::Matches,
        Some(_) => RemoteOutcome::Different,
        None => RemoteOutcome::Missing,
    };
    Ok(RemoteVerification {
        outcome,
        remote_oid,
        source_oid: source.into(),
        target_ref: target.into(),
    })
}

pub(crate) fn verify_remote(
    repo: &GitRepository,
    expected: &PushPreview,
) -> Result<RemoteVerification, String> {
    validate_destination(&expected.destination)?;
    if !matches!(expected.source_oid.len(), 40 | 64)
        || !expected.source_oid.bytes().all(|ch| ch.is_ascii_hexdigit())
        || !expected.target_ref.starts_with("refs/heads/")
        || expected.target_ref.len() > 523
    {
        return Err("无效的远程核对请求".into());
    }
    if !repo
        .budget
        .run(
            repo.command()
                .args(["check-ref-format", &expected.target_ref]),
        )?
        .status
        .success()
    {
        return Err("目标分支名称无效".into());
    }
    if !configured_targets(repo)?.iter().any(|target| {
        target.remote == expected.remote && target.destination == expected.destination
    }) {
        return Err("远程地址已变化，不能用新地址核对旧推送".into());
    }
    let _local = if Path::new(&expected.destination).is_absolute() {
        Some(GuardedPath::pin(Path::new(&expected.destination), true)?)
    } else {
        None
    };
    let config = PushConfig::new(repo, &expected.destination)?;
    check_destination(repo, &config, &expected.destination)?;
    repo.budget.check()?;
    let output = repo.budget.run(isolated_command(repo, &config).args([
        "ls-remote",
        "--refs",
        "--",
        &config.remote,
        &expected.target_ref,
    ]))?;
    if !output.status.success() || output.truncated {
        return Err("远程核对失败或响应超限；不能判断目标是否存在，请检查认证和连接".into());
    }
    parse_remote(&output.stdout, &expected.target_ref, &expected.source_oid)
}

#[tauri::command]
pub async fn project_git_verify_remote(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
    expected: PushPreview,
) -> Result<RemoteVerification, String> {
    if webview.label() != "main" {
        return Err("远程核对需要主窗口".into());
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
            |repo| verify_remote(repo, &expected),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn project_git_push_targets(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
) -> Result<PushTargets, String> {
    if webview.label() != "main" {
        return Err("推送配置需要主窗口".into());
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
            list_targets,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn project_git_push(
    webview: tauri::Webview,
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
    expected: PushPreview,
) -> Result<Option<PushResult>, String> {
    if webview.label() != "main" {
        return Err("Git 推送需要主窗口".into());
    }
    let root = app
        .state::<crate::task::TaskStore>()
        .project_path(&project_id)?;
    let operation = app
        .state::<crate::git_operation::GitOperations>()
        .begin(&operation_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let running = operation;
        with_repository(&app, Path::new(&root), false, running.budget.clone(), |repo| {
            let branch = expected.target_ref.strip_prefix("refs/heads/").ok_or("Invalid push target")?;
            if prepare_push(repo, &expected.remote, &expected.destination, branch)? != expected {
                return Err("推送配置或源提交已变化，请重新加载".into());
            }
            let confirmed = app.dialog().message(format!(
                "工作树：{}\n远程：{}\n目的地：{}\n目标：{}\n源提交：{}\n\n推送会传输此提交及其可达历史，不限于项目子目录。\n仅更新上述分支，不强推、不附带标签、不递归推送子模块、不运行客户端 hooks。\n使用系统现有凭据；本入口不交互登录、不签署推送证书。",
                repo.worktree.path.display(), expected.remote, expected.destination, expected.target_ref, expected.source_oid,
            )).title("确认推送")
                .buttons(MessageDialogButtons::OkCancelCustom("推送".into(), "取消".into())).blocking_show();
            if !confirmed { return Ok(None); }
            push_snapshot(repo, &expected).map(Some)
        })
    }).await.map_err(|error| error.to_string())?
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::{
        git_repository::{run_git, GitRepository},
        git_test_support::TestRepo,
    };
    use std::{fs, path::Path};

    fn fixture() -> (TestRepo, std::path::PathBuf) {
        let fixture = TestRepo::new();
        fs::write(fixture.root.join("file.txt"), "content\n").unwrap();
        fixture.git(&["add", "--", "file.txt"]);
        fixture.git(&["commit", "-m", "base"]);
        let remote = fixture.base.join("remote space.git");
        fixture.git(&[
            "init",
            "--bare",
            "--template=",
            "--initial-branch=main",
            remote.to_str().unwrap(),
        ]);
        fixture.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        (fixture, remote)
    }

    fn remote_oid(fixture: &TestRepo, remote: &Path, reference: &str) -> Option<String> {
        let output = run_git(&mut fixture.command(&[
            "--git-dir",
            remote.to_str().unwrap(),
            "rev-parse",
            "--verify",
            "--quiet",
            reference,
        ]))
        .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
    }

    #[test]
    fn pushes_exact_commit_to_selected_branch_and_reports_up_to_date() {
        let (fixture, remote) = fixture();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview =
            prepare_push(&repo, "origin", remote.to_str().unwrap(), "feature/中文").unwrap();
        let result = push_snapshot(&repo, &preview).unwrap();
        assert_eq!(result.outcome, PushOutcome::Pushed);
        assert_eq!(
            remote_oid(&fixture, &remote, "refs/heads/feature/中文").as_deref(),
            Some(preview.source_oid.as_str())
        );
        assert!(remote_oid(&fixture, &remote, "refs/heads/main").is_none());
        let repo = GitRepository::open(&fixture.root).unwrap();
        assert_eq!(
            push_snapshot(
                &repo,
                &prepare_push(&repo, "origin", remote.to_str().unwrap(), "feature/中文").unwrap()
            )
            .unwrap()
            .outcome,
            PushOutcome::UpToDate
        );
    }

    #[test]
    fn isolates_one_push_url_and_ignores_mirror_extra_refspecs_and_tags() {
        let (fixture, remote) = fixture();
        let other = fixture.base.join("other.git");
        fixture.git(&["init", "--bare", "--template=", other.to_str().unwrap()]);
        fixture.git(&[
            "config",
            "--add",
            "remote.origin.pushurl",
            remote.to_str().unwrap(),
        ]);
        fixture.git(&[
            "config",
            "--add",
            "remote.origin.pushurl",
            other.to_str().unwrap(),
        ]);
        fixture.git(&["config", "remote.origin.mirror", "true"]);
        fixture.git(&["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"]);
        fixture.git(&["config", "push.followTags", "true"]);
        fixture.git(&["branch", "unselected"]);
        fixture.git(&["tag", "-a", "v-test", "-m", "not selected"]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        assert_eq!(list_targets(&repo).unwrap().targets.len(), 2);
        let preview = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        assert_eq!(
            push_snapshot(&repo, &preview).unwrap().outcome,
            PushOutcome::Pushed
        );
        assert!(remote_oid(&fixture, &remote, "refs/heads/unselected").is_none());
        assert!(remote_oid(&fixture, &remote, "refs/tags/v-test").is_none());
        assert!(remote_oid(&fixture, &other, "refs/heads/main").is_none());
    }

    #[test]
    fn rejects_non_fast_forward_without_overwriting_remote() {
        let (fixture, remote) = fixture();
        let other = TestRepo::new();
        fs::write(other.root.join("other.txt"), "unrelated\n").unwrap();
        other.git(&["add", "--", "other.txt"]);
        other.git(&["commit", "-m", "independent"]);
        other.git(&["push", remote.to_str().unwrap(), "main:main"]);
        let before = remote_oid(&fixture, &remote, "refs/heads/main");
        let repo = GitRepository::open(&fixture.root).unwrap();
        let result = push_snapshot(
            &repo,
            &prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap(),
        )
        .unwrap();
        assert_eq!(result.outcome, PushOutcome::Rejected);
        assert_eq!(remote_oid(&fixture, &remote, "refs/heads/main"), before);
    }

    #[test]
    fn refuses_changed_source_destination_or_cancelled_operation() {
        let (fixture, remote) = fixture();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        fixture.git(&["commit", "--allow-empty", "-m", "new head"]);
        assert!(push_snapshot(&repo, &preview).is_err());
        let preview = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        fixture.git(&[
            "remote",
            "set-url",
            "--push",
            "origin",
            fixture.base.join("another.git").to_str().unwrap(),
        ]);
        assert!(push_snapshot(&repo, &preview).is_err());
        fixture.git(&[
            "remote",
            "set-url",
            "--push",
            "origin",
            remote.to_str().unwrap(),
        ]);
        repo.budget.token.cancel();
        assert!(push_snapshot(&repo, &preview).is_err());
        assert!(remote_oid(&fixture, &remote, "refs/heads/main").is_none());
    }

    #[test]
    fn rejects_credential_bearing_and_executable_destinations_without_echoing_them() {
        for destination in [
            "https://token-secret@example.com/repo.git",
            "https://example.com/repo.git?token=token-secret",
            "ssh://git:token-secret@example.com/repo.git",
            "ext::command token-secret",
            "http://example.com/repo.git",
            "\\\\host\\share\\repo.git",
        ] {
            let error = validate_destination(destination).unwrap_err();
            assert!(!error.contains("token-secret"));
        }
        assert!(validate_destination("https://github.com/example/repo.git").is_ok());
        assert!(validate_destination("git@github.com:example/repo.git").is_ok());
    }

    #[test]
    fn rejects_second_url_rewrite_before_contacting_another_destination() {
        let (fixture, remote) = fixture();
        let redirect = fixture.base.join("redirect.git");
        fixture.git(&["init", "--bare", "--template=", redirect.to_str().unwrap()]);
        fixture.git(&["remote", "set-url", "origin", "alias:repo"]);
        fixture.git(&[
            "config",
            &format!("url.{}.insteadOf", remote.to_str().unwrap()),
            "alias:repo",
        ]);
        fixture.git(&[
            "config",
            &format!("url.{}.insteadOf", redirect.to_str().unwrap()),
            remote.to_str().unwrap(),
        ]);
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        assert!(push_snapshot(&repo, &preview).unwrap_err().contains("重写"));
        assert!(remote_oid(&fixture, &remote, "refs/heads/main").is_none());
        assert!(remote_oid(&fixture, &redirect, "refs/heads/main").is_none());
    }

    #[test]
    fn does_not_run_client_hooks_or_send_implicit_push_options() {
        let (fixture, remote) = fixture();
        fixture.git(&["config", "push.pushOption", "do-not-send"]);
        fs::create_dir_all(fixture.root.join(".git/hooks")).unwrap();
        fs::write(
            fixture.root.join(".git/hooks/pre-push"),
            "#!/bin/sh\nprintf invoked > hook-ran.txt\nexit 1\n",
        )
        .unwrap();
        let before = fs::read(fixture.root.join(".git/config")).unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        assert_eq!(
            push_snapshot(&repo, &preview).unwrap().outcome,
            PushOutcome::Pushed
        );
        assert!(!fixture.root.join("hook-ran.txt").exists());
        assert_eq!(fs::read(fixture.root.join(".git/config")).unwrap(), before);
        assert!(!fs::read_dir(fixture.root.join(".git"))
            .unwrap()
            .any(|entry| {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                name.starts_with("deeppi-") && name.ends_with(".config")
            }));
    }

    #[test]
    fn cancelling_a_running_push_reports_unknown_and_releases_owned_processes() {
        let (fixture, remote) = fixture();
        fs::create_dir_all(remote.join("hooks")).unwrap();
        fs::write(
            remote.join("hooks/pre-receive"),
            "#!/bin/sh\nprintf started > push-started\nsleep 15\n",
        )
        .unwrap();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        let token = repo.budget.token.clone();
        let marker = remote.join("push-started");
        let cancellation = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            while !marker.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            let started = marker.exists();
            token.cancel();
            started
        });
        let started = std::time::Instant::now();
        let result = push_snapshot(&repo, &preview).unwrap();
        assert!(
            cancellation.join().unwrap(),
            "Server hook must be reached before cancellation"
        );
        assert_eq!(result.outcome, PushOutcome::Unknown);
        assert!(started.elapsed() < Duration::from_secs(20));
        assert!(remote_oid(&fixture, &remote, "refs/heads/main").is_none());
        assert!(!remote.join("refs/heads/main.lock").exists());
    }

    #[test]
    fn verifies_missing_equal_and_different_remote_refs_without_modifying_local_refs() {
        let (fixture, remote) = fixture();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let preview = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        assert_eq!(
            verify_remote(&repo, &preview).unwrap().outcome,
            RemoteOutcome::Missing
        );
        push_snapshot(&repo, &preview).unwrap();
        assert_eq!(
            verify_remote(&repo, &preview).unwrap().outcome,
            RemoteOutcome::Matches
        );
        fixture.git(&["commit", "--allow-empty", "-m", "local moved"]);
        let next = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        let check = verify_remote(&repo, &next).unwrap();
        assert_eq!(check.outcome, RemoteOutcome::Different);
        assert_eq!(
            check.remote_oid.as_deref(),
            Some(preview.source_oid.as_str())
        );
        assert_eq!(
            verify_remote(&repo, &preview).unwrap().outcome,
            RemoteOutcome::Matches
        );
        assert!(!fixture.root.join(".git/FETCH_HEAD").exists());
        assert!(!fixture.root.join(".git/refs/remotes").exists());
    }

    #[test]
    fn remote_verification_rejects_destination_changes_and_malformed_requests() {
        let (fixture, remote) = fixture();
        let repo = GitRepository::open(&fixture.root).unwrap();
        let mut preview = prepare_push(&repo, "origin", remote.to_str().unwrap(), "main").unwrap();
        preview.source_oid = "invalid".into();
        assert!(verify_remote(&repo, &preview).is_err());
        preview.source_oid = "a".repeat(40);
        preview.target_ref = "refs/tags/not-a-branch".into();
        assert!(verify_remote(&repo, &preview).is_err());
        preview.target_ref = "refs/heads/main".into();
        fixture.git(&["remote", "remove", "origin"]);
        assert!(verify_remote(&repo, &preview).is_err());
    }

    #[test]
    fn parses_remote_results_strictly_and_does_not_treat_errors_as_missing() {
        let expected = "a".repeat(40);
        let target = "refs/heads/main";
        assert_eq!(
            parse_remote(b"", target, &expected).unwrap().outcome,
            RemoteOutcome::Missing
        );
        for output in [
            format!("{expected}\trefs/heads/other\n"),
            format!("{expected}\t{target}\n{expected}\t{target}\n"),
            format!("invalid\t{target}\n"),
            format!("{expected}\t{target}"),
        ] {
            assert!(parse_remote(output.as_bytes(), target, &expected).is_err());
        }
    }
}
