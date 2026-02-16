use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::gpu_v2::QueueId;

// TODO: should standardize on `pub` for internal crates with only `pub(crate)`
// in mod.rs to simplify things

#[derive(Debug, Clone)]
pub struct GpuFuture {
    queue_id: QueueId,
    value: Arc<AtomicU64>,
}

impl GpuFuture {
    pub fn new(queue_id: QueueId) -> Self {
        let value = Arc::new(AtomicU64::new(u64::MAX));
        Self { queue_id, value }
    }

    pub fn queue_id(&self) -> QueueId {
        self.queue_id
    }

    pub fn uninitialized(&self) -> bool {
        self.value.load(Ordering::Acquire) == u64::MAX
    }

    pub fn get(&self) -> Option<u64> {
        let value = self.value.load(Ordering::Acquire);
        if value == u64::MAX {
            return None;
        }
        Some(value)
    }

    pub fn set(&self, value: u64) {
        self.value.store(value, Ordering::Release);
    }
}
