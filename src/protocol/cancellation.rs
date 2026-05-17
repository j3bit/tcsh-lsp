use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Minimal cancellation registry for M0. Expensive providers will consult this in later milestones.
#[derive(Debug, Clone, Default)]
pub struct CancellationRegistry {
    inner: Arc<Mutex<HashSet<String>>>,
}

impl CancellationRegistry {
    pub fn mark_cancelled(&self, id: impl ToString) {
        if let Ok(mut cancelled) = self.inner.lock() {
            cancelled.insert(id.to_string());
        }
    }

    pub fn is_cancelled(&self, id: impl ToString) -> bool {
        self.inner
            .lock()
            .map(|cancelled| cancelled.contains(&id.to_string()))
            .unwrap_or(false)
    }

    pub fn clear(&self, id: impl ToString) {
        if let Ok(mut cancelled) = self.inner.lock() {
            cancelled.remove(&id.to_string());
        }
    }
}
