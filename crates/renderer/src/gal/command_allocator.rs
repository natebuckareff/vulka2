use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::{Result, bail};

use crate::gal::command_pool::CommandPool;
use crate::gal::command_pool_resource::CommandPoolResource;
use crate::gal::queue::Lane;
use crate::gal::{device::Device, usage_token::UsageToken};

pub struct CommandAllocator {
    device: Arc<Device>,
    lane: Lane,
    capacity: usize,
    pools: VecDeque<PoolEntry>,
}

impl CommandAllocator {
    pub fn new(device: Arc<Device>, lane: Lane, capacity: usize) -> Self {
        Self {
            device,
            lane,
            capacity,
            pools: VecDeque::with_capacity(capacity),
        }
    }

    pub fn acquire(&mut self) -> Result<CommandPool> {
        // first check if the oldest pool is reclaimable
        if let Some(entry) = self.pools.front() {
            if entry.usage.is_reclaimable(&self.device)? {
                // SAFETY: pools is not empty
                let entry = self.pools.pop_front().unwrap();
                return Ok(entry.pool);
            }
        }

        if let Some(pool) = self.allocate_pool()? {
            // otherwise, attempt to allocate a new pool
            Ok(pool)
        } else {
            // if the allocator is at max capacity, wait for the oldest pool to
            // become reclaimable
            self.wait_acquire()
        }
    }

    fn allocate_pool(&mut self) -> Result<Option<CommandPool>> {
        if self.pools.len() == self.capacity {
            return Ok(None);
        }
        let device = self.device.resource();
        let resource = CommandPoolResource::new(device)?;
        let pool = CommandPool::new(resource, self.lane);
        Ok(Some(pool))
    }

    fn wait_acquire(&mut self) -> Result<CommandPool> {
        let Some(entry) = self.pools.pop_front() else {
            bail!("cannot wait on empty command allocator")
        };
        entry.usage.wait_until_reclaimable(&self.device)?;
        Ok(entry.pool)
    }

    pub fn retire(&mut self, pool: CommandPool) -> Result<()> {
        if pool.lane().family() != self.lane.family() {
            bail!("command pool queue family does not match allocator");
        }
        let entry = PoolEntry::new(pool);
        self.pools.push_back(entry);
        Ok(())
    }
}

struct PoolEntry {
    pool: CommandPool,
    usage: UsageToken,
}

impl PoolEntry {
    fn new(mut pool: CommandPool) -> Self {
        let usage = pool.swap_usage();
        Self { pool, usage }
    }
}
