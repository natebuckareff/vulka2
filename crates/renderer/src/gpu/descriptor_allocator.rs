use std::{rc::Rc, sync::Arc};

use anyhow::{Context, Result, anyhow};
use generational_arena::{Arena, Index};
use vulkanalia::vk::{self, DeviceV1_0, HasBuilder};

use crate::gpu::{
    DescriptorSet, DescriptorSetId, DescriptorSetLayout, DescriptorUsage, Device,
    FinishedDescriptorSet, OwnedDescriptorPool, RetireQueue, VulkanResource,
};

pub struct DescriptorAllocator {
    id: usize,
    device: Arc<Device>,
    layout: Arc<DescriptorSetLayout>,
    pools: Arena<DescriptorPool>,
    retirement: RetireQueue<DescriptorUsage>,
    next_set_id: usize,
}

impl DescriptorAllocator {
    // TODO: this `id` situation is pretty messy currently and annoyingly
    // bespoke across the codebase
    pub(crate) fn new(
        id: usize,
        device: Arc<Device>,
        layout: Arc<DescriptorSetLayout>,
    ) -> Result<Self> {
        let retirement = RetireQueue::new(device.clone())?;
        Ok(Self {
            id,
            device,
            layout,
            pools: Arena::new(),
            retirement,
            next_set_id: 0,
        })
    }

    pub fn acquire(&mut self) -> Result<DescriptorSet> {
        let id = DescriptorSetId::from(self.next_set_id);
        let index = self.acquire_or_create_pool()?;
        let pool = &self.pools[index];
        let set = DescriptorSet::new(id, &self.device, pool, &self.layout)?;
        self.next_set_id += 1;
        Ok(set)
    }

    fn acquire_or_create_pool(&mut self) -> Result<Index> {
        if let Some(index) = self.acquire_unused_pool()? {
            return Ok(index);
        }
        let info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(self.layout.sizing().max_sets())
            .pool_sizes(&self.layout.sizing().sizes());
        let device = self.device.handle().clone();
        let handle = Rc::new(OwnedDescriptorPool::new(device, &info)?);
        let index = self.pools.insert_with(|index| {
            let id = DescriptorPoolId {
                allocator_id: self.id,
                index,
            };
            DescriptorPool {
                id,
                handle,
                unretired: 0,
            }
        });
        Ok(index)
    }

    fn acquire_unused_pool(&mut self) -> Result<Option<Index>> {
        loop {
            let Some(usage) = self.retirement.acquire()? else {
                break;
            };
            let pool = &mut self.pools[usage.pool.index];
            pool.unretired -= 1;
            if pool.unretired == 0 {
                let device = self.device.handle();
                let flags = vk::DescriptorPoolResetFlags::empty();
                unsafe {
                    let descriptor_pool = *pool.handle.raw();
                    device.raw().reset_descriptor_pool(descriptor_pool, flags)?;
                }
                return Ok(Some(usage.pool.index));
            }
        }
        Ok(None)
    }

    pub fn release(&mut self, set: FinishedDescriptorSet) -> Result<()> {
        if set.pool_id.allocator_id != self.id {
            return Err(anyhow!("descriptor pool allocator mismatch"));
        };

        let pool = self
            .pools
            .get_mut(set.pool_id.index)
            .context("pool not found")?;

        pool.unretired += 1;

        if let Err(e) = self.retirement.retire(set.token.retire) {
            pool.unretired -= 1;
            return Err(e);
        }

        Ok(())
    }
}

pub(crate) struct DescriptorPool {
    id: DescriptorPoolId,
    handle: Rc<OwnedDescriptorPool>, // TODO XXX: ResourceArena is kinda annoying
    unretired: usize,
}

impl DescriptorPool {
    pub(crate) fn id(&self) -> DescriptorPoolId {
        self.id
    }

    pub(crate) fn handle(&self) -> &Rc<OwnedDescriptorPool> {
        &self.handle
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DescriptorPoolId {
    allocator_id: usize,
    index: Index,
}
