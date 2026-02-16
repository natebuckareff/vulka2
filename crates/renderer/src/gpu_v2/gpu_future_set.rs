use crate::gpu_v2::{GpuFuture, QueueId};

#[derive(Debug, Clone)]
pub struct GpuFutureSet {
    // OPTIMIZE: use SmallVec since most will be 1 item
    futures: Vec<GpuFuture>,
}

impl GpuFutureSet {
    pub fn new() -> Self {
        Self {
            futures: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            futures: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.futures.len()
    }

    pub fn has(&self, id: QueueId) -> bool {
        self.futures.iter().any(|f| f.queue_id() == id)
    }

    pub fn add_if_not_present(&mut self, future: GpuFuture) {
        if self.has(future.queue_id()) {
            return;
        }
        self.futures.push(future);
    }

    pub fn get(&self, id: QueueId) -> &GpuFuture {
        let Some(future) = self.futures.iter().find(|f| f.queue_id() == id) else {
            panic!("invalid queue id");
        };
        future
    }

    pub fn get_mut(&mut self, id: QueueId) -> &mut GpuFuture {
        let Some(future) = self.futures.iter_mut().find(|f| f.queue_id() == id) else {
            panic!("invalid queue id");
        };
        future
    }

    pub fn iter(&self) -> impl Iterator<Item = &GpuFuture> {
        self.futures.iter()
    }

    pub fn into_futures(self) -> Vec<GpuFuture> {
        self.futures
    }

    pub fn take_futures(&mut self) -> Vec<GpuFuture> {
        std::mem::take(&mut self.futures)
    }
}
