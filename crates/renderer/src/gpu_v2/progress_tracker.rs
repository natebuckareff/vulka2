use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use vulkanalia::vk;

use crate::gpu_v2::{Device, EpochValue, LaneKey, QueueGroupVec, EpochRef, VulkanHandle};

type Timeline = u64;

pub struct ProgressTracker {
    device: Arc<Device>,
    blocked: HashMap<EpochValue, EpochRef>,
    epochs: VecDeque<EpochRef>,
    lanes: QueueGroupVec<LaneProgress>,
    next_epoch: EpochValue,
    scratch: Vec<Timeline>,
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

    fn next_epoch_number(&mut self) -> EpochValue {
        if let Some(epoch) = self.epochs.back() {
            self.next_epoch = epoch.number() + 1;
        }
        self.next_epoch
    }

    // push an epoch, wait for it to host-complete, read the number of total
    // submissions for that epoch, and track when lanes signal those epochs are
    // device-complete
    pub fn push(&mut self, epoch: EpochRef) -> Result<()> {
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
        use vulkanalia::prelude::v1_2::*;

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
            for (i, (key, count)) in epoch.submissions().iter().enumerate() {
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
    pub fn is_complete(&self, key: LaneKey, epoch: EpochValue) -> bool {
        let (_, progress) = self.lanes.get(key);
        let Some((signaled_epoch, _)) = progress.signaled else {
            // no epochs signaled yet
            return false;
        };
        epoch <= signaled_epoch
    }
}

struct LaneProgress {
    semaphore: VulkanHandle<vk::Semaphore>,
    signaled: Option<(EpochValue, Timeline)>, // the last epoch that was signaled and the signal value
    unsignaled: VecDeque<(EpochValue, u64)>, // the epochs that have been completed and their counts
}

impl LaneProgress {
    fn push(&mut self, epoch: EpochValue, count: u64, value: Timeline) {
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
