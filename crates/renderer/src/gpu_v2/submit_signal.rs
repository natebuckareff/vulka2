use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use anyhow::Result;

#[derive(Clone)]
pub struct SubmitSignal {
    pair: Arc<(Mutex<u64>, Condvar)>,
}

impl SubmitSignal {
    pub fn new() -> Self {
        let mutex = Mutex::new(0);
        let condvar = Condvar::new();
        Self {
            pair: Arc::new((mutex, condvar)),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, u64> {
        self.pair.0.lock().expect("failed to lock notify mutex")
    }

    pub fn wait<'a>(&self, guard: MutexGuard<'a, u64>) -> MutexGuard<'a, u64> {
        self.pair
            .1
            .wait(guard)
            .expect("failed to wait on notify condvar")
    }

    pub fn notify(&self) -> Result<()> {
        let mut guard = self.lock();
        *guard += 1;
        self.pair.1.notify_one();
        Ok(())
    }
}
