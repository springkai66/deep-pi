use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicU8>);

impl Cancellation {
    pub fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::SeqCst) == 1 {
            Err("operation cancelled".into())
        } else {
            Ok(())
        }
    }

    pub fn cancel(&self) -> bool {
        self.0
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn commit(&self) -> Result<(), String> {
        self.0
            .compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| "operation cancelled or already committing".into())
    }
}

struct Active {
    id: String,
    token: Cancellation,
}

#[derive(Default)]
pub struct OperationManager(Arc<Mutex<Option<Active>>>);

pub struct Operation {
    manager: Arc<Mutex<Option<Active>>>,
    pub token: Cancellation,
}

impl OperationManager {
    pub fn begin(&self, id: &str) -> Result<Operation, String> {
        if id.is_empty() || id.len() > 128 {
            return Err("invalid operation ID".into());
        }
        let mut active = self.0.lock().map_err(|error| error.to_string())?;
        if active.is_some() {
            return Err("another operation is in progress".into());
        }
        let token = Cancellation::default();
        *active = Some(Active {
            id: id.into(),
            token: token.clone(),
        });
        Ok(Operation {
            manager: self.0.clone(),
            token,
        })
    }

    pub fn cancel(&self, id: &str) -> Result<bool, String> {
        let active = self.0.lock().map_err(|error| error.to_string())?;
        Ok(active
            .as_ref()
            .is_some_and(|active| active.id == id && active.token.cancel()))
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut active) = self.manager.lock() {
            *active = None;
        }
    }
}

#[tauri::command]
pub fn cancel_operation(
    operations: tauri::State<'_, OperationManager>,
    operation_id: String,
) -> Result<bool, String> {
    operations.cancel(&operation_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_scoped_and_cannot_cross_the_commit_boundary() {
        let operations = OperationManager::default();
        let first = operations.begin("first").unwrap();
        assert!(operations.begin("second").is_err());
        assert!(!operations.cancel("old").unwrap());
        assert!(operations.cancel("first").unwrap());
        assert!(first.token.check().is_err());
        assert!(first.token.commit().is_err());
        drop(first);
        let second = operations.begin("second").unwrap();
        assert!(!operations.cancel("first").unwrap());
        second.token.commit().unwrap();
        assert!(!operations.cancel("second").unwrap());
    }
}
