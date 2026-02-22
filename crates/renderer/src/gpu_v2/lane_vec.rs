use anyhow::{Result, anyhow};
use smallvec::SmallVec;

use crate::gpu_v2::QueueGroupId;

pub const MAX_STATIC_LANES: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct LaneIndex {
    queue_group_id: QueueGroupId,
    index: usize,
}

impl Into<usize> for LaneIndex {
    fn into(self) -> usize {
        self.index
    }
}

#[derive(Clone)]
pub struct LaneVec<T> {
    queue_group_id: QueueGroupId,
    vec: SmallVec<[T; MAX_STATIC_LANES]>,
}

impl<T> LaneVec<T> {
    pub fn new(queue_group_id: QueueGroupId, capacity: usize) -> Self {
        let vec = SmallVec::with_capacity(capacity);
        Self {
            queue_group_id,
            vec,
        }
    }

    pub fn with(queue_group_id: QueueGroupId, len: usize, f: impl FnMut() -> T) -> Self {
        let mut vec = SmallVec::with_capacity(len);
        vec.resize_with(len, f);
        Self {
            queue_group_id,
            vec,
        }
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    pub fn index(&self, index: usize) -> Result<LaneIndex> {
        if index >= self.len() {
            return Err(anyhow!("lane index out of bounds"));
        }
        Ok(LaneIndex {
            index,
            queue_group_id: self.queue_group_id,
        })
    }

    pub fn get(&self, index: LaneIndex) -> &T {
        debug_assert!(
            self.queue_group_id == index.queue_group_id,
            "mismatched queue groups"
        );
        &self.vec[index.index]
    }

    pub fn get_mut(&mut self, index: LaneIndex) -> &mut T {
        debug_assert!(
            self.queue_group_id == index.queue_group_id,
            "mismatched queue groups"
        );
        &mut self.vec[index.index]
    }

    pub fn set(&mut self, index: LaneIndex, value: T) {
        debug_assert!(
            self.queue_group_id == index.queue_group_id,
            "mismatched queue groups"
        );
        debug_assert!(index.index < self.len(), "lane index out of bounds");
        self.vec[index.index] = value;
    }

    // TODO: this should be a seperate builder type, because then we can assume
    // OOB is impossible if push is not possible
    pub fn push(&mut self, value: T) {
        assert!(self.len() < self.capacity());
        self.vec.push(value);
    }

    pub fn each(&self) -> impl Iterator<Item = LaneIndex> {
        let queue_group_id = self.queue_group_id;
        (0..self.len()).map(move |index| LaneIndex {
            queue_group_id,
            index,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.vec.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.vec.iter_mut()
    }

    pub fn into_iter(self) -> impl Iterator<Item = T> {
        self.vec.into_iter()
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = (LaneIndex, &T)> {
        let queue_group_id = self.queue_group_id;
        self.vec.iter().enumerate().map(move |(index, value)| {
            let index = LaneIndex {
                queue_group_id,
                index,
            };
            (index, value)
        })
    }

    pub fn into_entries(self) -> impl Iterator<Item = (LaneIndex, T)> {
        let queue_group_id = self.queue_group_id;
        self.vec.into_iter().enumerate().map(move |(index, value)| {
            let index = LaneIndex {
                queue_group_id,
                index,
            };
            (index, value)
        })
    }

    pub fn retain_into(mut self, mut f: impl FnMut(&T) -> bool) -> SmallVec<[T; MAX_STATIC_LANES]> {
        self.vec.retain(|value| f(value));
        self.vec
    }
}
