use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk::{self, DeviceV1_2};

use crate::gpu_v2::{Device, LaneKey, QueueGroupTable, QueueGroupVec, VulkanHandle};

struct SubmissionEpochState {
    number: i32,
    consumed: Mutex<bool>, // TODO: can probably replace with an atomic?
}

pub struct SubmissionEpoch {
    state: Arc<SubmissionEpochState>,
    submissions: Arc<QueueGroupVec<AtomicU64>>,
}

impl SubmissionEpoch {
    pub(crate) fn new(queue_groups: &QueueGroupTable) -> Self {
        let state = SubmissionEpochState {
            number: 0,
            consumed: Mutex::new(false),
        };
        let submissions = QueueGroupVec::new(queue_groups, Default::default);
        Self {
            state: Arc::new(state),
            submissions: Arc::new(submissions),
        }
    }

    pub fn next(self, queue_groups: &QueueGroupTable) -> Result<SubmissionEpoch> {
        let mut guard = self.state.consumed.lock().unwrap();
        if *guard {
            return Err(anyhow!("submission epoch already consumed"));
        }
        *guard = true;
        drop(guard);
        let state = SubmissionEpochState {
            number: self.state.number + 1,
            consumed: Mutex::new(false),
        };
        let submissions = QueueGroupVec::new(queue_groups, Default::default);
        Ok(Self {
            state: Arc::new(state),
            submissions: Arc::new(submissions),
        })
    }

    pub fn number(&self) -> i32 {
        self.state.number
    }

    pub fn increment(&self, key: LaneKey) {
        let (_, counter) = self.submissions.get(key);
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reference(&self) -> SubmissionEpochRef {
        SubmissionEpochRef {
            parent: Arc::downgrade(&self.state),
            number: self.state.number,
            submissions: self.submissions.clone(),
        }
    }
}

pub struct SubmissionEpochRef {
    parent: Weak<SubmissionEpochState>,
    number: i32,
    submissions: Arc<QueueGroupVec<AtomicU64>>,
}

impl SubmissionEpochRef {
    pub fn number(&self) -> i32 {
        self.number
    }

    pub fn is_complete(&self) -> bool {
        self.parent.upgrade().is_none()
    }

    pub fn submissions(&self, key: LaneKey) -> u64 {
        let (_, counter) = self.submissions.get(key);
        counter.load(Ordering::Relaxed)
    }
}

struct LaneProgress {
    semaphore: VulkanHandle<vk::Semaphore>,
    signaled: Option<(i32, u64)>, // the last epoch that was signaled and the signal value
    unsignaled: VecDeque<(i32, u64)>, // the epochs that have been completed and their counts
}

impl LaneProgress {
    fn push(&mut self, epoch: i32, count: u64, value: u64) {
        debug_assert!(self.signaled.is_some() || (self.signaled.is_none() && epoch == 0));
        self.unsignaled.push_back((epoch, count));
        self.update(value);
    }

    fn update(&mut self, value: u64) {
        let mut required = self.signaled.map(|(_, value)| value).unwrap_or(0);
        while let Some((epoch, count)) = self.unsignaled.front() {
            required += count;
            if value >= required {
                self.signaled = Some((*epoch, required));
                self.unsignaled.pop_front();
            } else {
                break;
            }
        }
    }
}

pub struct ProgressTracker {
    device: Arc<Device>,
    blocked: HashMap<i32, SubmissionEpochRef>,
    epochs: VecDeque<SubmissionEpochRef>,
    lanes: QueueGroupVec<LaneProgress>,
    next_epoch: i32,
    scratch: Vec<u64>,
}

impl ProgressTracker {
    pub fn new(device: Arc<Device>) -> Result<Self> {
        let queue_groups = device.queue_group_table();
        let lanes = QueueGroupVec::try_new(queue_groups, |key| {
            let binding = queue_groups.get_binding(key).context("invalid lane key")?;
            let semaphore = binding.semaphore.clone();
            Ok(LaneProgress {
                semaphore,
                signaled: None,
                unsignaled: VecDeque::new(),
            })
        })?;
        let mut scratch = Vec::with_capacity(lanes.len());
        scratch.resize_with(lanes.len(), Default::default);
        Ok(Self {
            device,
            blocked: HashMap::new(),
            epochs: VecDeque::new(),
            lanes,
            next_epoch: 0,
            scratch,
        })
    }

    fn next_epoch_number(&mut self) -> i32 {
        if let Some(epoch) = self.epochs.back() {
            self.next_epoch = epoch.number() + 1;
        }
        self.next_epoch
    }

    // push an epoch, wait for it to host-complete, read the number of total
    // submissions for that epoch, and track when lanes signal those epochs are
    // device-complete
    pub fn push(&mut self, epoch: SubmissionEpochRef) -> Result<()> {
        let next_epoch_number = self.next_epoch_number();

        if epoch.number() < next_epoch_number {
            // already applied this epoch
            return Ok(());
        }

        if epoch.number() == next_epoch_number {
            // this is the next epoch, apply it
            self.epochs.push_back(epoch);
        } else {
            // haven't seen the epochs that come before this one, stash it in
            // blocked and try again later
            let prev = self.blocked.insert(epoch.number(), epoch);
            assert!(prev.is_none())
        }

        // loop through blocked to see if we can apply any of the future epochs
        loop {
            let next_epoch_number = self.next_epoch_number();
            if let Some(epoch) = self.blocked.remove(&next_epoch_number) {
                self.epochs.push_back(epoch);
            } else {
                break;
            }
        }

        Ok(())
    }

    pub fn update(&mut self) -> Result<()> {
        let device = self.device.handle();
        let mut epoch_completed = false;

        // read semaphore values first
        for (i, (_, progress)) in self.lanes.iter().enumerate() {
            let value = unsafe {
                let semaphore = *progress.semaphore.raw();
                device.raw().get_semaphore_counter_value(semaphore)?
            };
            self.scratch[i] = value;
        }

        // update lane progress with newly completed epochs
        while let Some(epoch) = self.epochs.front() {
            if !epoch.is_complete() {
                break;
            }
            let epoch_number = epoch.number();
            epoch_completed = true;

            // update progress
            for (i, (key, count)) in epoch.submissions.iter().enumerate() {
                let value = self.scratch[i];
                let (_, progress) = self.lanes.get_mut(key);
                let count = count.load(Ordering::Relaxed);
                progress.push(epoch_number, count, value);
            }

            self.epochs.pop_front();
        }

        // update progress to ensure that lanes always progress
        if !epoch_completed {
            for (i, (_, progress)) in self.lanes.iter_mut().enumerate() {
                let value = self.scratch[i];
                progress.update(value);
            }
        }

        Ok(())
    }

    // is some lane complete in the nth epoch
    pub fn is_complete(&self, key: LaneKey, epoch: i32) -> bool {
        let (_, progress) = self.lanes.get(key);
        let Some((signaled_epoch, _)) = progress.signaled else {
            // no epochs signaled yet
            return false;
        };
        epoch <= signaled_epoch
    }
}

struct RetireState<T: Copy> {
    // TODO: implicit max 64 total lanes; update Device/QueueGroupTable to
    // enforce this invariant
    dirty: AtomicUsize,
    last_epoch: AtomicI32,
    retired: AtomicBool,
    handle: T,
}

#[derive(Clone)]
struct RetireToken<T: Copy> {
    state: Arc<RetireState<T>>,
}

impl<T: Copy> RetireToken<T> {
    // called when a RetireToken is used in a CommandBuffer
    pub fn touch(&self, epoch: i32, key: LaneKey) {
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

struct RetireQueue<T: Copy + Eq + Hash> {
    progress: ProgressTracker,
    counts: HashMap<T, i32>,
    retired: QueueGroupVec<Vec<(i32, T)>>,
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

    pub fn retire(&mut self, epoch: SubmissionEpochRef, token: RetireToken<T>) -> Result<()> {
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
