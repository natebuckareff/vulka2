//! Lifecycle:
//! - reclaim
//! - acquire
//! - hold
//! - release
//! - retire
//!
//! Engine has RetireQueues internal to each large allocator-like struct, for
//! example command pools, descriptor pools, etc. At the start of a frame or
//! another large engine "tick", engine users call `SomeAllocator::reclaim()`
//! which internall calls `RetireQueue::reclaim()`
//
//! Then, when allocating / getting the next resource, the allocator internally
//! calls `RetireQueue::acquire()` to try and get the next resource handle that
//! can be recycled
//
//! If `RetireQueue::acquire()` returns `None` then there are no handles ready
//! for recycling, and the allocator must intead create a new resource, and wrap
//! it in a `RetireToken` by calling `RetireQueue::produce(handle)`
//!
//! The acquired/produced `RetireToken` is then held for as long as the resource
//! is in-use by the GPU. Once the user is sure the resource will no longer be
//! used after some submission, they retire the resource by "releasing" the
//! associated `RetireToken` to that submission with `Submission::release(token)`
//
//! When the `QueueGroup` processes that submission it will retire all released
//! tokens by looking up the associated `RetireQueue`s using the `RetireTable`
//! and pushing `RetireBatch`s to them. This closes the loop and will eventually
//! recycle the handle after the next reclaim

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context, Result};
use generational_arena::{Arena, Index};
use smallvec::SmallVec;

use crate::gpu_v2::{Device, QueueGroupTable, QueueGroupVec, QueueLaneKey};

#[derive(Clone, Copy)]
pub struct RetireQueueId(Index);

// TODO: need a ref count for cross-lane resource tracking
struct RetireToken<T: Copy> {
    id: RetireQueueId,
    handle: T,
}

#[derive(Clone)]
struct RetireBatch<T: Copy> {
    key: QueueLaneKey,         // queue that retired this batch
    value: u64,                // submission timeline value
    handles: SmallVec<[T; 1]>, // all the token handles
}

impl<T: Copy> Default for RetireBatch<T> {
    fn default() -> Self {
        Self {
            key: QueueLaneKey::default(),
            value: u64::MAX, // fail loudly
            handles: SmallVec::new(),
        }
    }
}

struct RetireQueue<T: Copy> {
    device: Arc<Device>,
    id: RetireQueueId,
    queue_groups: QueueGroupTable,
    pending: Arc<Mutex<Vec<RetireBatch<T>>>>,
    reclaimed: QueueGroupVec<VecDeque<RetireBatch<T>>>,
    ready: VecDeque<RetireBatch<T>>,
    current: Option<RetireBatch<T>>,
    index: usize,
}

impl<T: Copy> RetireQueue<T> {
    pub fn new(
        device: Arc<Device>,
        queue_groups: QueueGroupTable,
        retire_queues: &RetireTable<T>,
    ) -> Self {
        let pending: Arc<Mutex<Vec<RetireBatch<T>>>> = Arc::new(Mutex::new(Vec::new()));
        let id = retire_queues.insert(pending.clone());
        let reclaimed = QueueGroupVec::new(&queue_groups);
        Self {
            device,
            id,
            queue_groups,
            pending,
            reclaimed,
            ready: VecDeque::new(),
            current: None,
            index: 0,
        }
    }

    pub fn id(&self) -> RetireQueueId {
        self.id
    }

    pub fn produce(&self, handle: T) -> RetireToken<T> {
        RetireToken {
            id: self.id,
            handle,
        }
    }

    pub fn reclaim(&mut self) -> Result<()> {
        // NOTE: currently only flush on reclaim which should be on frame
        // boundaries, but may want to expose as public API
        self.reclaim_without_flush()?;
        self.flush()?;
        Ok(())
    }

    fn reclaim_without_flush(&mut self) -> Result<()> {
        let mut pending = {
            let mut locked = self.pending.lock().unwrap();
            if locked.is_empty() {
                return Ok(());
            };
            std::mem::take(&mut *locked)
        };

        for batch in pending.drain(..) {
            let (_, entry) = self.reclaimed.get_mut_or_default(batch.key);
            // NOTE: sanity check, order should be guaranteed by queue submission
            debug_assert!(entry.back().map_or(true, |b| b.value <= batch.value));
            entry.push_back(batch);
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        use vulkanalia::prelude::v1_2::*;
        let device = self.device.handle();
        for (index, batches) in self.reclaimed.iter_mut() {
            if batches.is_empty() {
                continue;
            };

            let binding = self.queue_groups.get_binding(index)?;
            let semaphore = unsafe { *binding.semaphore.raw() };
            let value = unsafe { device.raw().get_semaphore_counter_value(semaphore) }?;

            while let Some(batch) = batches.front() {
                if value < batch.value {
                    break;
                }
                // SAFETY: already checked that batches is not empty
                let batch = unsafe { batches.pop_front().unwrap_unchecked() };
                self.ready.push_back(batch);
            }
        }
        Ok(())
    }

    pub fn acquire(&mut self) -> Result<Option<T>> {
        // check if the current batch is done
        if let Some(batch) = &self.current {
            if self.index >= batch.handles.len() {
                self.current = None;
            }
        }

        // if the current batch was done, get the next one
        if self.current.is_none() {
            self.current = self.ready.pop_front();
            self.index = 0;
        }

        // read the next handle from the current batch
        if let Some(batch) = &self.current {
            let handle = batch.handles[self.index];
            self.index += 1;
            return Ok(Some(handle));
        }

        // nothing to read
        Ok(None)
    }
}

type RetireWriter<T: Copy> = Arc<Mutex<Vec<RetireBatch<T>>>>;

// NOTE: writers are never removed for now, the RetireTable is write-once since
// allocators will be very long-lived (device-coupled probably) and not that
// many

struct RetireTable<T: Copy> {
    arena: Arc<RwLock<Arena<RetireWriter<T>>>>,
}

impl<T: Copy> RetireTable<T> {
    pub fn insert(&self, writer: RetireWriter<T>) -> RetireQueueId {
        let index = {
            let mut guard = self.arena.write().unwrap();
            guard.insert(writer)
        };
        RetireQueueId(index)
    }

    // NOTE OPTIMIZE: will want to pre-aggregate in queue submission to minimize
    // contention here
    pub fn retire(&self, id: RetireQueueId, batch: RetireBatch<T>) -> Result<()> {
        let r = self.arena.read().unwrap();
        let writer = r.get(id.0).context("retire queue not found")?;
        let mut guard = writer.lock().unwrap();
        guard.push(batch);
        Ok(())
    }
}

impl<T: Copy> Clone for RetireTable<T> {
    fn clone(&self) -> Self {
        Self {
            arena: self.arena.clone(),
        }
    }
}
