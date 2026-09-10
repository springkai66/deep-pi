use std::{
    sync::{Condvar, Mutex},
    time::Duration,
};

use tauri::{AppHandle, Manager, State};

#[derive(Default)]
pub struct StartupState {
    result: Mutex<Option<Result<(), String>>>,
    changed: Condvar,
}

impl StartupState {
    pub fn finish(&self, result: Result<(), String>) {
        if let Ok(mut current) = self.result.lock() {
            *current = Some(result);
            self.changed.notify_all();
        }
    }

    pub fn ready(&self) -> bool {
        self.result
            .lock()
            .is_ok_and(|result| matches!(*result, Some(Ok(()))))
    }

    fn wait(&self) -> Result<(), String> {
        let result = self
            .result
            .lock()
            .map_err(|_| "Initialization state unavailable")?;
        let (result, _) = self
            .changed
            .wait_timeout_while(result, Duration::from_secs(120), |result| result.is_none())
            .map_err(|_| "Initialization state unavailable")?;
        result
            .clone()
            .unwrap_or_else(|| Err("初始化仍在进行，请稍后重试".into()))
    }
}

#[tauri::command]
pub async fn await_startup(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<StartupState>().wait())
        .await
        .map_err(|error| format!("Initialization worker failed: {error}"))?
}

pub fn is_ready(state: State<'_, StartupState>) -> bool {
    state.ready()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn initialization_notifies_all_waiters_only_after_publication() {
        let state = Arc::new(StartupState::default());
        assert!(!state.ready());
        let threads: Vec<_> = (0..3)
            .map(|_| {
                let state = state.clone();
                std::thread::spawn(move || state.wait())
            })
            .collect();
        state.finish(Ok(()));
        assert!(state.ready());
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
    }

    #[test]
    fn initialization_failure_is_stable_for_late_waiters() {
        let state = StartupState::default();
        state.finish(Err("database unavailable".into()));
        assert!(!state.ready());
        assert_eq!(state.wait(), Err("database unavailable".into()));
        assert_eq!(state.wait(), Err("database unavailable".into()));
    }
}
