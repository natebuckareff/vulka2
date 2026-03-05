use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use anyhow::{Context, Result, anyhow};

use crate::gpu_v2::{Device, EpochRef, EpochValue, LaneKey, ProgressTracker, QueueGroupVec};

struct RetireState<T: Copy> {
    // TODO: implicit max 64 total lanes; update Device/QueueGroupTable to
    // enforce this invariant
    dirty: AtomicUsize,
    last_epoch: AtomicU64,
    retired: AtomicBool,
    handle: T,
}

#[derive(Clone)]
pub struct RetireToken<T: Copy> {
    state: Arc<RetireState<T>>,
}

impl<T: Copy> RetireToken<T> {
    pub fn new(handle: T) -> Self {
        Self {
            state: Arc::new(RetireState {
                dirty: AtomicUsize::new(0),
                last_epoch: AtomicU64::new(u64::MAX), // XXX
                retired: AtomicBool::new(false),
                handle,
            }),
        }
    }

    pub fn handle(&self) -> T {
        self.state.handle
    }

    // called when a RetireToken is used in a CommandBuffer
    pub fn touch(&self, epoch: EpochValue, key: LaneKey) {
        debug_assert!(
            !self.state.retired.load(Ordering::Relaxed),
            "token already retired"
        );

        let bit: usize = key.into();
        self.state.dirty.fetch_or(1 << bit, Ordering::Relaxed);
        self.state.last_epoch.fetch_max(epoch, Ordering::Relaxed);

        debug_assert!(
            !self.state.retired.load(Ordering::Relaxed),
            "token already retired"
        );
    }
}

pub struct RetireQueue<T: Copy + Eq + Hash> {
    progress: ProgressTracker,
    counts: HashMap<T, i32>,
    retired: QueueGroupVec<Vec<(EpochValue, T)>>,
    ready: VecDeque<T>,
}

impl<T: Copy + Eq + Hash> RetireQueue<T> {
    pub fn new(device: Arc<Device>) -> Result<Self> {
        let queue_groups = device.queue_group_table();
        let retired = QueueGroupVec::new(queue_groups, Default::default);
        Ok(Self {
            progress: ProgressTracker::new(device)?,
            counts: HashMap::new(),
            retired,
            ready: VecDeque::new(),
        })
    }

    pub fn retire(&mut self, epoch: EpochRef, token: RetireToken<T>) -> Result<()> {
        // NOTE: While allocators that use RetireQueue internally will generally
        // always be called from the same thread, RetireToken does not
        // inherently care which RetireQueue retires it. Any RetireQueue will do
        if token.state.retired.swap(true, Ordering::Relaxed) {
            return Err(anyhow!("token already retired"));
        }

        let handle = token.state.handle;
        let last_epoch = token.state.last_epoch.load(Ordering::Relaxed);
        let mut dirty = token.state.dirty.load(Ordering::Relaxed);

        self.progress.push(epoch)?;
        self.progress.update()?;

        let mut count = 0;
        while dirty != 0 {
            let bit = dirty.trailing_zeros() as usize;
            let key = self.retired.key(bit).context("invalid lane key")?;
            dirty ^= 1 << bit;
            if self.progress.is_complete(key, last_epoch) {
                // ready in this lane, skip
                continue;
            }
            let (_, retired) = self.retired.get_mut(key);
            retired.push((last_epoch, handle));
            count += 1;
        }

        if count > 0 {
            let prev = self.counts.insert(handle, count);
            assert!(prev.is_none());
        } else {
            self.ready.push_back(handle);
        }

        Ok(())
    }

    pub fn acquire(&mut self) -> Result<Option<T>> {
        if let Some(handle) = self.ready.pop_front() {
            return Ok(Some(handle));
        }

        self.progress.update()?;

        for (key, retired) in self.retired.iter_mut() {
            let mut i = 0;
            while i < retired.len() {
                let (epoch, handle) = &retired[i];
                if self.progress.is_complete(key, *epoch) {
                    // SAFETY: if we have a handle in retired, it must have an
                    // element in counts
                    let count = self.counts.get_mut(handle).unwrap();
                    *count -= 1;
                    debug_assert!(*count >= 0);
                    if *count == 0 {
                        self.ready.push_back(*handle);
                        self.counts.remove(handle);
                    }
                    retired.swap_remove(i);
                    continue;
                }
                i += 1;
            }
        }

        // try returning from ready again
        Ok(self.ready.pop_front())
    }
}
