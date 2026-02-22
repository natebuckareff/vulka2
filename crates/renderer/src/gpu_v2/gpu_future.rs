//! GpuFuture is a state machine with the following allowed transitions:
//!
//!   UNSET -> WAITING  send()
//! WAITING -> SET      set()
//! WAITING -> POISONED owner panicked / forgot to set

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
    pub fn new() -> Self {
        Self {
            value: Arc::new(AtomicU64::new(GPU_FUTURE_UNSET)),
        }
    }

    pub fn get(&self) -> Result<GpuFutureState> {
        let value = self.value.load(Ordering::Acquire);
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
        match self.get()? {
            GpuFutureState::Set(value) => Ok(value),
            GpuFutureState::Unset => Err(anyhow!("gpu future unset")),
            GpuFutureState::Waiting => Err(anyhow!("gpu future waiting")),
        }
    }

    // get a GpuFutureWriter to send to another thread that will eventually
    // write the future's final value
    pub fn send(&self) -> Result<GpuFutureWriter> {
        match self.value.compare_exchange(
            GPU_FUTURE_UNSET,
            GPU_FUTURE_WAITING,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                let future = self.clone();
                Ok(GpuFutureWriter { future })
            }
            Err(current) if current == GPU_FUTURE_WAITING => Err(anyhow!("gpu future dupe send")),
            Err(current) if current == GPU_FUTURE_POISONED => Err(anyhow!("gpu future poisoned")),
            Err(_) => Err(anyhow!("gpu future is written and not reset")),
        }
    }

    pub fn reset(&self) -> Result<()> {
        loop {
            let current = self.value.load(Ordering::Acquire);
            if current == GPU_FUTURE_UNSET {
                return Ok(());
            }
            if current == GPU_FUTURE_WAITING {
                return Err(anyhow!("gpu future cannot be reset with pending write"));
            }
            if current == GPU_FUTURE_POISONED {
                return Err(anyhow!("gpu future poisoned"));
            }
            match self.value.compare_exchange(
                current,
                GPU_FUTURE_UNSET,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(_) => continue,
            }
        }
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
        match self.future.value.compare_exchange(
            GPU_FUTURE_WAITING,
            value,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(current) if current == GPU_FUTURE_UNSET => Err(anyhow!("gpu future unset")),
            Err(current) if current == GPU_FUTURE_POISONED => Err(anyhow!("gpu future poisoned")),
            Err(_) => Err(anyhow!("gpu future writer already set")),
        }
    }
}

impl Drop for GpuFutureWriter {
    fn drop(&mut self) {
        // if the future writer is dropped before it is set, assume this means
        // that the writer thread panicked, or forgot to call set(), which is a
        // bug in either case
        let _ = self.future.value.compare_exchange(
            GPU_FUTURE_WAITING,
            GPU_FUTURE_POISONED,
            Ordering::Release,
            Ordering::Acquire,
        );
    }
}
