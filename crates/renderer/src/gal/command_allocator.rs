use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::{Result, bail};

use crate::gal::Device;
use crate::gal::command_pool::CommandPool;
use crate::gal::command_pool_resource::CommandPoolResource;
use crate::gal::queue::QueueFamily;
use crate::gal::usage_token::UsageToken;

pub struct CommandAllocator {
    device: Arc<Device>,
    family: QueueFamily,
    capacity: usize,
    pools: VecDeque<PoolEntry>,
}

impl CommandAllocator {
    pub fn new(device: Arc<Device>, family: QueueFamily, capacity: usize) -> Self {
        Self {
            device,
            family,
            capacity,
            pools: VecDeque::with_capacity(capacity),
        }
    }

    pub fn acquire(&mut self, frame: u32) -> Result<Option<CommandPool>> {
        // first check if the oldest pool is reclaimable
        if let Some(entry) = self.pools.front() {
            if entry.usage.is_reclaimable(&self.device)? {
                // SAFETY: pools is not empty
                let entry = self.pools.pop_front().unwrap();
                return Ok(Some(entry.reclaim(frame)?));
            }
        }
        if let Some(pool) = self.allocate_pool(frame)? {
            // otherwise, attempt to allocate a new pool
            Ok(Some(pool))
        } else {
            // if the allocator is at max capacity, wait for the oldest pool to
            // become reclaimable
            self.wait_acquire(frame)
        }
    }

    fn allocate_pool(&mut self, frame: u32) -> Result<Option<CommandPool>> {
        if self.pools.len() == self.capacity {
            return Ok(None);
        }
        let device = self.device.resource().clone();
        let resource = CommandPoolResource::new(device, self.family)?;
        let pool = CommandPool::new(frame, resource);
        Ok(Some(pool))
    }

    fn wait_acquire(&mut self, frame: u32) -> Result<Option<CommandPool>> {
        let Some(entry) = self.pools.front() else {
            bail!("cannot wait on empty command allocator")
        };
        if entry.usage.wait_until_reclaimable(&self.device)? {
            // SAFETY: pools is not empty
            let entry = self.pools.pop_front().unwrap();
            Ok(Some(entry.reclaim(frame)?))
        } else {
            Ok(None)
        }
    }

    pub fn retire(&mut self, pool: CommandPool) -> Result<()> {
        if pool.resource().family() != self.family {
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

    fn reclaim(mut self, frame: u32) -> Result<CommandPool> {
        self.pool.reset(frame)?;
        Ok(self.pool)
    }
}
