use smallvec::SmallVec;

use crate::gpu_v2::{LaneKey, QueueGroupId};

pub const MAX_STATIC_LANES: usize = 4;

pub struct LaneVecBuilder<T> {
    queue_group_id: QueueGroupId,
    len: usize,
    vec: SmallVec<[T; MAX_STATIC_LANES]>,
}

impl<T> LaneVecBuilder<T> {
    pub fn new(queue_group_id: QueueGroupId, len: usize) -> Self {
        Self {
            queue_group_id,
            len,
            vec: SmallVec::with_capacity(len),
        }
    }

    pub fn with_lanes<U>(lanes: &LaneVec<U>) -> Self {
        Self::new(lanes.queue_group_id(), lanes.len())
    }

    pub fn push(&mut self, value: T) {
        assert!(self.vec.len() < self.len);
        self.vec.push(value);
    }

    pub fn build(self) -> LaneVec<T> {
        assert!(self.vec.len() == self.len);
        LaneVec {
            queue_group_id: self.queue_group_id,
            vec: self.vec,
        }
    }
}

#[derive(Clone)]
pub struct LaneVec<T> {
    queue_group_id: QueueGroupId,
    vec: SmallVec<[T; MAX_STATIC_LANES]>,
}

impl<T> LaneVec<T> {
    pub fn filled<U>(lanes: &LaneVec<U>, f: impl FnMut() -> T) -> Self {
        let mut vec = SmallVec::with_capacity(lanes.len());
        vec.resize_with(lanes.len(), f);
        Self {
            queue_group_id: lanes.queue_group_id(),
            vec,
        }
    }

    pub fn queue_group_id(&self) -> QueueGroupId {
        self.queue_group_id
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn get(&self, key: LaneKey) -> &T {
        debug_assert!(
            self.queue_group_id == key.queue_group(),
            "mismatched queue groups"
        );
        &self.vec[key.index()]
    }

    pub fn get_mut(&mut self, key: LaneKey) -> &mut T {
        debug_assert!(
            self.queue_group_id == key.queue_group(),
            "mismatched queue groups"
        );
        &mut self.vec[key.index()]
    }

    pub fn set(&mut self, key: LaneKey, value: T) {
        debug_assert!(
            self.queue_group_id == key.queue_group(),
            "mismatched queue groups"
        );
        self.vec[key.index()] = value;
    }

    pub fn each(&self) -> impl Iterator<Item = LaneKey> {
        let queue_group_id = self.queue_group_id;
        (0..self.len()).map(move |index| LaneKey::from((queue_group_id, index)))
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

    pub fn iter_entries(&self) -> impl Iterator<Item = (LaneKey, &T)> {
        let queue_group_id = self.queue_group_id;
        self.vec.iter().enumerate().map(move |(index, value)| {
            let key = LaneKey::from((queue_group_id, index));
            (key, value)
        })
    }

    pub fn iter_entries_mut(&mut self) -> impl Iterator<Item = (LaneKey, &mut T)> {
        let queue_group_id = self.queue_group_id;
        self.vec.iter_mut().enumerate().map(move |(index, value)| {
            let key = LaneKey::from((queue_group_id, index));
            (key, value)
        })
    }

    pub fn into_entries(self) -> impl Iterator<Item = (LaneKey, T)> {
        let queue_group_id = self.queue_group_id;
        self.vec.into_iter().enumerate().map(move |(index, value)| {
            let key = LaneKey::from((queue_group_id, index));
            (key, value)
        })
    }

    pub fn find<P>(&mut self, mut predicate: P) -> Option<&T>
    where
        P: FnMut(&T) -> bool,
    {
        self.vec.iter().find(|value| predicate(value))
    }

    pub fn retain_into(mut self, mut f: impl FnMut(&T) -> bool) -> SmallVec<[T; MAX_STATIC_LANES]> {
        self.vec.retain(|value| f(value));
        self.vec
    }
}
