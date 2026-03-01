use crate::gpu_v2::{LaneIndex, QueueGroupTable, QueueLaneKey};

// TODO: `LaneIndex` and `QueueLaneKey` are a bit messy now. We have both
// `LaneVec` and `QueueGroupVec`. Should probably refactor `QueueGroupTable`
// around `QueueLaneVec` and have stronger ordering guarantees. Also some of the
// naming is not great; see `LaneIndex` TODOs

pub struct QueueGroupVec<T> {
    vec: Vec<(LaneIndex, Option<T>)>,
}

impl<T> QueueGroupVec<T> {
    pub fn new(queue_groups: &QueueGroupTable) -> Self {
        let len = queue_groups.total_lanes() as usize;
        let mut vec = Vec::with_capacity(len);
        for (index, _) in queue_groups.iter_bindings() {
            vec.push((index, None::<T>));
        }
        Self { vec }
    }

    // OPTIMIZE: improve this to not be O(n); should be fast since N=1 is the
    // most common case anyways
    pub fn len(&self) -> usize {
        let mut len = 0;
        for _ in self.iter() {
            len += 1;
        }
        len
    }

    pub fn get(&self, lane: QueueLaneKey) -> (LaneIndex, Option<&T>) {
        let (index, value) = &self.vec[lane.key() as usize];
        (*index, value.as_ref())
    }

    pub fn get_mut(&mut self, lane: QueueLaneKey) -> (LaneIndex, &mut Option<T>) {
        let (index, value) = &mut self.vec[lane.key() as usize];
        (*index, value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (LaneIndex, &T)> {
        self.vec
            .iter()
            .filter_map(|(index, value)| value.as_ref().map(|value| (*index, value)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (LaneIndex, &mut T)> {
        self.vec
            .iter_mut()
            .filter_map(|(index, value)| value.as_mut().map(|value| (*index, value)))
    }
}

impl<T: Default> QueueGroupVec<T> {
    pub fn get_mut_or_default(&mut self, lane: QueueLaneKey) -> (LaneIndex, &mut T) {
        let (index, entry) = &mut self.vec[lane.key() as usize];
        match entry {
            Some(value) => (*index, value),
            None => {
                let value = T::default();
                *entry = Some(value);
                // SAFETY: we just initialized the entry
                let entry = unsafe { entry.as_mut().unwrap_unchecked() };
                (*index, entry)
            }
        }
    }
}
