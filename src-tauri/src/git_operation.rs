use std::{
    collections::{HashMap, VecDeque},
    process::Command,
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    thread,
    time::{Duration, Instant},
};

use crate::{operation::Cancellation, process_runner::ProcessOutput};

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
            .ok_or_else(|| "Git 读取已超过总等待预算，请重试".into())
    }

    pub(crate) fn run(&self, command: &mut Command) -> Result<ProcessOutput, String> {
        crate::process_runner::run_cancellable(
            command,
            self.remaining()?.min(Duration::from_secs(10)),
            Some(&self.token),
        )
    }

    pub(crate) fn lock<'a, T>(&self, mutex: &'a Mutex<T>) -> Result<MutexGuard<'a, T>, String> {
        loop {
            self.check()?;
            match mutex.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::Poisoned(_)) => return Err("Git 操作锁已失效，请重启应用".into()),
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
            return Err("Git 请求已取消、完成或正在运行，请使用新的请求 ID".into());
        }
        if registry.active.len() >= MAX_ACTIVE {
            return Err("Git 请求队列已满，请稍后重试".into());
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
