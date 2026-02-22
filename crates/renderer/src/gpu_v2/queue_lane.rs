use std::ops::{Deref, DerefMut};

use anyhow::Result;
use smallvec::SmallVec;

use crate::gpu_v2::{GpuFuture, QueueBinding, QueueId, QueueRoleFlags};

pub const MAX_STATIC_LANES: usize = 4;

#[derive(Clone, Copy)]
pub struct QueueLane {
    pub id: QueueId,
    pub roles: QueueRoleFlags,
}

#[derive(Clone)]
pub struct PoolLane {
    pub queue: QueueLane,
    pub future: GpuFuture,
}

impl PoolLane {
    pub fn reset(&mut self) -> Result<()> {
        self.future.reset()?;
        Ok(())
    }
}

// TODO: need a more generic LaneVec
#[derive(Clone)]
pub struct PoolLanes(SmallVec<[PoolLane; MAX_STATIC_LANES]>);

impl PoolLanes {
    pub(crate) fn new(bindings: &[QueueBinding]) -> Result<Self> {
        let mut lanes = SmallVec::with_capacity(MAX_STATIC_LANES);
        for binding in bindings {
            let queue = QueueLane {
                id: binding.id,
                roles: binding.roles,
            };
            let lane = PoolLane {
                queue,
                future: GpuFuture::new(),
            };
            lanes.push(lane);
        }
        Ok(Self(lanes))
    }

    pub(crate) fn reset(&mut self) -> Result<()> {
        for lane in self.0.iter_mut() {
            lane.reset()?;
        }
        Ok(())
    }
}

// TODO: replace with better API
impl Deref for PoolLanes {
    type Target = [PoolLane];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// TODO: replace with better API
impl DerefMut for PoolLanes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
