use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};

const GPU_FUTURE_UNSET: u64 = u64::MAX;
const GPU_FUTURE_WAITING: u64 = u64::MAX - 1;
const GPU_FUTURE_POISONED: u64 = u64::MAX - 2;

pub struct TimelineValue(u64);

impl Into<u64> for TimelineValue {
    fn into(self) -> u64 {
        self.0
    }
}

pub enum GpuFutureState {
    Unset,
    Waiting,
    Set(TimelineValue),
}

#[derive(Clone)]
pub struct GpuFuture {
    value: Arc<AtomicU64>,
}

impl GpuFuture {
    pub fn unset() -> Self {
        Self {
            value: Arc::new(AtomicU64::new(GPU_FUTURE_UNSET)),
        }
    }

    fn load(&self) -> u64 {
        self.value.load(Ordering::Acquire)
    }

    pub fn is_unset(&self) -> bool {
        self.load() == GPU_FUTURE_UNSET
    }

    pub fn is_waiting(&self) -> bool {
        self.load() == GPU_FUTURE_WAITING
    }

    pub fn is_poisoned(&self) -> bool {
        self.load() == GPU_FUTURE_POISONED
    }

    pub fn is_set(&self) -> Result<bool> {
        let value = self.load();
        if value == GPU_FUTURE_POISONED {
            return Err(anyhow!("gpu future poisoned"));
        }
        return Ok(value != GPU_FUTURE_UNSET && value != GPU_FUTURE_WAITING);
    }

    pub fn get(&self) -> Result<GpuFutureState> {
        let value = self.load();
        if value == GPU_FUTURE_UNSET {
            return Ok(GpuFutureState::Unset);
        }
        if value == GPU_FUTURE_WAITING {
            return Ok(GpuFutureState::Waiting);
        }
        if value == GPU_FUTURE_POISONED {
            return Err(anyhow!("gpu future poisoned"));
        }
        Ok(GpuFutureState::Set(TimelineValue(value)))
    }

    pub fn get_or_err(&self) -> Result<TimelineValue> {
        let Ok(GpuFutureState::Set(value)) = self.get() else {
            return Err(anyhow!("gpu future not set or waiting"));
        };
        Ok(value)
    }

    // get a GpuFutureSender to send to another thread that will eventually
    // write the future's final value
    pub fn send(&self) -> Result<GpuFutureWriter> {
        if self.is_poisoned() {
            return Err(anyhow!("gpu future poisoned"));
        }

        // signal intent that this future is now waiting to be set from another
        // thread
        //
        // relaxed ordering is fine since this is set from the same thread that
        // later reads the future
        self.value.store(GPU_FUTURE_WAITING, Ordering::Relaxed);

        Ok(GpuFutureWriter {
            future: self.clone(),
        })
    }

    pub fn reset(&self) -> Result<()> {
        if self.is_poisoned() {
            return Err(anyhow!("gpu future poisoned"));
        }
        self.value.store(GPU_FUTURE_UNSET, Ordering::Release);
        Ok(())
    }
}

pub struct GpuFutureWriter {
    future: GpuFuture,
}

impl GpuFutureWriter {
    pub fn set(self, value: u64) -> Result<()> {
        if value == GPU_FUTURE_UNSET {
            return Err(anyhow!("value reserved for unset gpu future"));
        }
        if value == GPU_FUTURE_WAITING {
            return Err(anyhow!("value reserved for waiting gpu future"));
        }
        if value == GPU_FUTURE_POISONED {
            return Err(anyhow!("value reserved for poisoned gpu future"));
        }
        self.future.value.store(value, Ordering::Release);
        Ok(())
    }
}

impl Drop for GpuFutureWriter {
    fn drop(&mut self) {
        let value = self.future.load();
        if value == GPU_FUTURE_UNSET || value == GPU_FUTURE_WAITING {
            self.future
                .value
                .store(GPU_FUTURE_POISONED, Ordering::Release);
        }
    }
}
