use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use anyhow::{Result, anyhow};

use crate::gpu_v2::{LaneKey, QueueGroupTable, QueueGroupVec};

pub type EpochValue = u64;

struct SubmissionEpochState {
    number: EpochValue,
    consumed: Mutex<bool>, // TODO: can probably replace with an atomic?
}

pub struct Epoch {
    state: Arc<SubmissionEpochState>,
    submissions: Arc<QueueGroupVec<AtomicU64>>,
}

impl Epoch {
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

    pub fn next(self, queue_groups: &QueueGroupTable) -> Result<Epoch> {
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

    pub fn number(&self) -> EpochValue {
        self.state.number
    }

    pub fn increment(&self, key: LaneKey) {
        let (_, counter) = self.submissions.get(key);
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reference(&self) -> EpochRef {
        EpochRef {
            parent: Arc::downgrade(&self.state),
            number: self.state.number,
            submissions: self.submissions.clone(),
        }
    }
}

pub struct EpochRef {
    parent: Weak<SubmissionEpochState>,
    number: EpochValue,
    submissions: Arc<QueueGroupVec<AtomicU64>>,
}

impl EpochRef {
    pub fn number(&self) -> EpochValue {
        self.number
    }

    pub fn is_complete(&self) -> bool {
        self.parent.upgrade().is_none()
    }

    pub fn submissions(&self) -> &QueueGroupVec<AtomicU64> {
        &self.submissions
    }

    pub fn submission_count(&self, key: LaneKey) -> u64 {
        let (_, counter) = self.submissions.get(key);
        counter.load(Ordering::Relaxed)
    }
}
