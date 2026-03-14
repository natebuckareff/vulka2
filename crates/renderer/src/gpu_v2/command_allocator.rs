use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use generational_arena::{Arena, Index};

use crate::gpu_v2::{
    CommandPool, CommandPoolState, Device, QueueGroupId, QueueGroupInfo, QueueGroupTable,
    RetireQueue, RetireToken,
};

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CommandPoolId {
    allocator_id: usize,
    index: Index,
}

pub struct CommandAllocator {
    id: usize,
    device: Arc<Device>,
    queue_group_table: QueueGroupTable,
    queue_info: QueueGroupInfo,
    capacity: usize,
    pools: Arena<Option<CommandPoolState>>,
    retirement: RetireQueue<CommandPoolId>,
}

impl CommandAllocator {
    pub(crate) fn new(
        id: usize,
        device: Arc<Device>,
        queue_group_id: QueueGroupId,
        capacity: usize,
    ) -> Result<Self> {
        if capacity == 0 {
            return Err(anyhow!("capacity must be greater than 0"));
        }
        let queue_group_table = device.queue_group_table().clone();
        let queue_info = queue_group_table
            .get_info(queue_group_id)
            .context("queue group not found")?
            .clone();
        let retirement = RetireQueue::new(device.clone())?;
        let allocator = Self {
            id,
            device,
            queue_group_table,
            queue_info,
            capacity,
            pools: Arena::new(),
            retirement,
        };
        Ok(allocator)
    }

    pub fn acquire(&mut self) -> Result<Option<CommandPool>> {
        if let Some(id) = self.retirement.acquire()? {
            let slot = self
                .pools
                .get_mut(id.index)
                .context("command pool not found")?;
            let mut state = std::mem::take(slot).context("command pool already acquired")?;
            unsafe {
                state.reset()?;
            }
            let token = RetireToken::new(id);
            let pool = CommandPool::new(token, state);
            return Ok(Some(pool));
        }

        if self.pools.len() < self.capacity {
            let index = self.pools.insert(None);
            let id = CommandPoolId {
                allocator_id: self.id,
                index,
            };
            let token = RetireToken::new(id);
            let device = self.device.clone();
            let state = CommandPoolState::new(device, &self.queue_info)?;
            let pool = CommandPool::new(token, state);
            return Ok(Some(pool));
        }

        Ok(None)
    }

    pub fn release(&mut self, pool: CommandPool) -> Result<()> {
        if pool.retire().handle().allocator_id != self.id {
            return Err(anyhow!("command allocator mismatch"));
        }

        let id = pool.retire().handle();
        let slot = self
            .pools
            .get_mut(id.index)
            .context("command pool not found")?;

        if slot.is_some() {
            return Err(anyhow!("inconsistent command allocator state"));
        }

        let (token, state) = pool.split()?;

        *slot = Some(state);
        self.retirement.retire(token)?;

        Ok(())
    }
}

impl Drop for CommandAllocator {
    fn drop(&mut self) {
        for (_, slot) in self.pools.drain() {
            debug_assert!(slot.is_none(), "unreleased command pools");
        }
    }
}
