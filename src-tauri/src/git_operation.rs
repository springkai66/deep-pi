use std::{
    collections::{HashMap, VecDeque},
    process::Command,
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    thread,
    time::{Duration, Instant},
};

use crate::{message::msg, operation::Cancellation, process_runner::ProcessOutput};

const MAX_ACTIVE: usize = 16;
const MAX_RECENT: usize = 256;
const READ_BUDGET: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Registry {
    active: HashMap<uuid::Uuid, Cancellation>,
    recent: VecDeque<(uuid::Uuid, Instant)>,
}

impl Registry {
    fn prune(&mut self) {
        while self
            .recent
            .front()
            .is_some_and(|(_, time)| time.elapsed() > Duration::from_secs(120))
        {
            self.recent.pop_front();
        }
    }

    fn remember(&mut self, id: uuid::Uuid) {
        if self.recent.iter().any(|(known, _)| *known == id) {
            return;
        }
        if self.recent.len() == MAX_RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back((id, Instant::now()));
    }
}

#[derive(Clone, Default)]
pub struct GitOperations(Arc<Mutex<Registry>>);

#[derive(Clone)]
pub(crate) struct GitBudget {
    pub(crate) token: Cancellation,
    deadline: Instant,
}

impl GitBudget {
    pub(crate) fn new(duration: Duration) -> Self {
        Self {
            token: Cancellation::default(),
            deadline: Instant::now() + duration,
        }
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        self.remaining().map(|_| ())
    }

    pub(crate) fn remaining(&self) -> Result<Duration, String> {
        self.token.check()?;
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| msg("git.operation.budget_exhausted"))
    }

    pub(crate) fn run(&self, command: &mut Command) -> Result<ProcessOutput, String> {
        // 单条命令的运行时间由整体等待预算限制；不再叠加更短的命令级上限，
        // 否则慢盘或安全软件拦截进程创建时会把正常的 git 调用误判为超时。
        crate::process_runner::run_cancellable(command, self.remaining()?, Some(&self.token))
    }

    pub(crate) fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> Result<MutexGuard<'a, T>, String> {
        loop {
            self.check()?;
            match mutex.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::Poisoned(_)) => return Err(msg("git.operation.lock_poisoned")),
                Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

pub(crate) struct GitOperation {
    manager: GitOperations,
    id: uuid::Uuid,
    pub(crate) budget: GitBudget,
}

impl GitOperation {
    #[cfg(test)]
    fn check(&self) -> Result<(), String> {
        self.budget.check()
    }
}

impl GitOperations {
    pub(crate) fn begin(&self, id: &str) -> Result<GitOperation, String> {
        self.begin_with_budget(id, READ_BUDGET)
    }

    pub(crate) fn begin_with_budget(
        &self,
        id: &str,
        duration: Duration,
    ) -> Result<GitOperation, String> {
        let id = uuid::Uuid::parse_str(id).map_err(|_| "Invalid Git operation ID")?;
        let mut registry = self
            .0
            .lock()
            .map_err(|_| "Git request registry unavailable")?;
        registry.prune();
        if registry.active.contains_key(&id)
            || registry.recent.iter().any(|(known, _)| *known == id)
        {
            return Err(msg("git.operation.request_reused"));
        }
        if registry.active.len() >= MAX_ACTIVE {
            return Err(msg("git.operation.queue_full"));
        }
        let budget = GitBudget::new(duration);
        registry.active.insert(id, budget.token.clone());
        Ok(GitOperation {
            manager: self.clone(),
            id,
            budget,
        })
    }

    pub(crate) fn cancel(&self, id: &str) -> Result<(), String> {
        let id = uuid::Uuid::parse_str(id).map_err(|_| "Invalid Git operation ID")?;
        let mut registry = self
            .0
            .lock()
            .map_err(|_| "Git request registry unavailable")?;
        registry.prune();
        if let Some(token) = registry.active.get(&id) {
            token.cancel();
        }
        // A cancel IPC may arrive before the read IPC is registered.
        registry.remember(id);
        Ok(())
    }
}

impl Drop for GitOperation {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.manager.0.lock() {
            registry.active.remove(&self.id);
            registry.remember(self.id);
        }
    }
}

#[tauri::command]
pub fn cancel_git_read(
    webview: tauri::Webview,
    manager: tauri::State<'_, GitOperations>,
    operation_id: String,
) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("Git cancellation requires the main Webview".into());
    }
    manager.cancel(&operation_id)
}

/// 在主窗口持有项目路径与操作预算的前提下，于阻塞线程池中执行一次 Git 读取。
///
/// 七个 Git 命令此前各自重复同一段“解析项目路径 → 登记操作 ID → 转入阻塞线程 →
/// 打开仓库并执行”的样板；其中信任确认已出现分化（只有状态读取会请求用户信任，
/// 其余入口直接按未信任处理），靠人工保持一致性。集中到一处后，信任策略成为显式
/// 参数（`None` 表示不请求信任，文案由前端按当前语言传入），新增命令不会再漏掉
/// 主窗口校验、操作 ID 登记或超时预算。
///
/// 调用方负责在主线程完成 `webview.label() != "main"` 检查与请求参数校验；
/// 此处只做与所有 Git 入口共享的公共准备。
pub(crate) async fn run_git_command<T, F>(
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
    trust_dialog: Option<crate::dialog_text::ConfirmDialog>,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&crate::git_repository::GitRepository) -> Result<T, String> + Send + 'static,
{
    prepare_and_run(app, project_id, operation_id, trust_dialog, None, operation).await
}

/// 同 [`run_git_command`]，但覆盖默认的读取预算。
///
/// 远程跟踪引用的同步需要下载可达对象，远超普通仓库读取；调用方必须显式
/// 声明更长的预算，而不是默默沿用默认值导致正常同步被判超时。
pub(crate) async fn run_git_command_with_budget<T, F>(
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
    trust_dialog: Option<crate::dialog_text::ConfirmDialog>,
    budget: std::time::Duration,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&crate::git_repository::GitRepository) -> Result<T, String> + Send + 'static,
{
    prepare_and_run(
        app,
        project_id,
        operation_id,
        trust_dialog,
        Some(budget),
        operation,
    )
    .await
}

async fn prepare_and_run<T, F>(
    app: tauri::AppHandle,
    project_id: String,
    operation_id: String,
    trust_dialog: Option<crate::dialog_text::ConfirmDialog>,
    budget: Option<std::time::Duration>,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&crate::git_repository::GitRepository) -> Result<T, String> + Send + 'static,
{
    use tauri::Manager;
    let root = app
        .state::<crate::task::TaskStore>()
        .project_path(&project_id)?;
    let manager = app.state::<GitOperations>();
    let running = match budget {
        Some(budget) => manager.begin_with_budget(&operation_id, budget)?,
        None => manager.begin(&operation_id)?,
    };
    tauri::async_runtime::spawn_blocking(move || {
        crate::git_repository::with_repository(
            &app,
            std::path::Path::new(&root),
            trust_dialog,
            running.budget.clone(),
            operation,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_scoped_and_duplicate_ids_are_rejected() {
        let manager = GitOperations::default();
        let first = uuid::Uuid::new_v4().to_string();
        let second = uuid::Uuid::new_v4().to_string();
        let a = manager.begin(&first).unwrap();
        let b = manager.begin(&second).unwrap();
        assert!(manager.begin(&first).is_err());
        manager.cancel(&first).unwrap();
        assert!(a.check().is_err());
        assert!(b.check().is_ok());
    }

    #[test]
    fn cancellation_before_registration_prevents_work() {
        let manager = GitOperations::default();
        let id = uuid::Uuid::new_v4().to_string();
        manager.cancel(&id).unwrap();
        assert!(manager.begin(&id).is_err());
        assert!(manager.cancel("not-a-request-id").is_err());
    }

    #[test]
    fn queue_capacity_is_released_on_drop_and_ids_are_not_reused() {
        let manager = GitOperations::default();
        let requests: Vec<_> = (0..MAX_ACTIVE)
            .map(|_| manager.begin(&uuid::Uuid::new_v4().to_string()).unwrap())
            .collect();
        assert!(manager.begin(&uuid::Uuid::new_v4().to_string()).is_err());
        drop(requests);
        let id = uuid::Uuid::new_v4().to_string();
        drop(manager.begin(&id).unwrap());
        assert!(manager.begin(&id).is_err());
        assert!(manager.begin(&uuid::Uuid::new_v4().to_string()).is_ok());
    }

    #[test]
    fn deadline_covers_queue_wait_and_remaining_command_budget() {
        let budget = GitBudget::new(Duration::ZERO);
        assert!(budget.check().is_err());
        let budget = GitBudget::new(Duration::from_secs(30));
        assert!(budget.remaining().unwrap() <= Duration::from_secs(30));
        budget.token.cancel();
        assert!(budget.remaining().is_err());
    }

    #[test]
    fn waiting_for_repository_lock_can_be_cancelled() {
        let mutex = Arc::new(Mutex::new(()));
        let held = mutex.lock().unwrap();
        let budget = GitBudget::new(Duration::from_secs(30));
        let token = budget.token.clone();
        let waiting = mutex.clone();
        let worker = thread::spawn(move || budget.lock(&waiting).map(|_| ()));
        token.cancel();
        assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
        drop(held);
    }

    #[cfg(windows)]
    #[test]
    fn running_git_filter_process_tree_is_cancellable() {
        let budget = GitBudget::new(Duration::from_secs(30));
        let token = budget.token.clone();
        let worker = thread::spawn(move || {
            budget.run(Command::new("powershell.exe").args([
                "-NoProfile",
                "-Command",
                "Start-Sleep -Seconds 30",
            ]))
        });
        thread::sleep(Duration::from_millis(150));
        token.cancel();
        let started = Instant::now();
        assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
