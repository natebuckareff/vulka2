use anyhow::Result;

use crate::gpu::{LaneKey, QueueGroupTable};

pub struct QueueGroupVec<T> {
    vec: Vec<(LaneKey, T)>,
}

impl<T> QueueGroupVec<T> {
    pub fn new<F>(queue_groups: &QueueGroupTable, f: F) -> Self
    where
        F: Fn() -> T,
    {
        let len = queue_groups.total_lanes() as usize;
        let mut vec = Vec::with_capacity(len);
        for (key, _) in queue_groups.iter_bindings() {
            vec.push((key, f()));
        }
        Self { vec }
    }

    pub fn try_new<F>(queue_groups: &QueueGroupTable, f: F) -> Result<Self>
    where
        F: Fn(LaneKey) -> Result<T>,
    {
        let len = queue_groups.total_lanes() as usize;
        let mut vec = Vec::with_capacity(len);
        for (key, _) in queue_groups.iter_bindings() {
            vec.push((key, f(key)?));
        }
        Ok(Self { vec })
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn key(&self, index: usize) -> Option<LaneKey> {
        self.vec.get(index).map(|(key, _)| *key)
    }

    pub fn get(&self, key: LaneKey) -> (LaneKey, &T) {
        let idx: usize = key.into();
        let (index, value) = &self.vec[idx];
        (*index, value)
    }

    pub fn get_mut(&mut self, key: LaneKey) -> (LaneKey, &mut T) {
        let idx: usize = key.into();
        let (index, value) = &mut self.vec[idx];
        (*index, value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (LaneKey, &T)> {
        self.vec.iter().map(|(index, value)| (*index, value))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (LaneKey, &mut T)> {
        self.vec.iter_mut().map(|(index, value)| (*index, value))
    }
}
