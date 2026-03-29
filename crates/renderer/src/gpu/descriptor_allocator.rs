use std::{collections::VecDeque, sync::Arc};

use anyhow::{Context, Result, anyhow};
use generational_arena::{Arena, Index};
use vulkanalia::vk::{self, DeviceV1_0, HasBuilder};

use crate::gpu::{
    DescriptorSet, DescriptorSetHandle, DescriptorSetId, DescriptorSetLayout, Device,
    FreedDescriptorSet, OwnedDescriptorPool, RetireQueue, VulkanResource,
};

pub struct DescriptorAllocator {
    id: usize,
    device: Arc<Device>,
    set_layout: Arc<DescriptorSetLayout>,
    pools: Arena<DescriptorPool>,
    ready: VecDeque<DescriptorSetHandle>,
    retirement: RetireQueue<DescriptorSetHandle>,
    next_set_id: usize,
}

impl DescriptorAllocator {
    // TODO: this `id` situation is pretty messy currently and annoyingly
    // bespoke across the codebase
    pub(crate) fn new(
        id: usize, // TODO: reuse/generalize AllocId into EntityId  or something?
        device: Arc<Device>,
        set_layout: Arc<DescriptorSetLayout>,
    ) -> Result<Self> {
        let retirement = RetireQueue::new(device.clone())?;
        Ok(Self {
            id,
            device,
            set_layout,
            pools: Arena::new(),
            ready: VecDeque::new(),
            retirement,
            next_set_id: 0,
        })
    }

    pub fn acquire(&mut self) -> Result<DescriptorSet> {
        let id = DescriptorSetId::from(self.next_set_id);
        let index = self.acquire_or_create_pool()?;
        let device = self.device.clone();
        let pool = &self.pools[index];
        let set_layout = self.set_layout.clone();
        let set = DescriptorSet::new(id, device, &pool, set_layout)?;
        self.next_set_id += 1;
        Ok(set)
    }

    fn acquire_or_create_pool(&mut self) -> Result<Index> {
        if let Some(index) = self.acquire_unused_pool()? {
            return Ok(index);
        }
        let info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(self.set_layout.sizing().max_sets())
            .pool_sizes(&self.set_layout.sizing().sizes());
        let device = self.device.handle().clone();
        let owned = OwnedDescriptorPool::new(device, &info)?;
        let index = self.pools.insert_with(|index| {
            let id = DescriptorPoolId {
                allocator_id: self.id,
                index,
            };
            DescriptorPool {
                id,
                owned,
                unretired: 0,
            }
        });
        Ok(index)
    }

    fn acquire_unused_pool(&mut self) -> Result<Option<Index>> {
        loop {
            let Some(handle) = self.acquire_handle()? else {
                break;
            };
            let pool = &mut self.pools[handle.pool().index];
            pool.unretired -= 1;
            if pool.unretired == 0 {
                let device = self.device.handle();
                let flags = vk::DescriptorPoolResetFlags::empty();
                unsafe {
                    let descriptor_pool = *pool.owned.raw();
                    device.raw().reset_descriptor_pool(descriptor_pool, flags)?;
                }
                return Ok(Some(handle.pool().index));
            }
        }
        Ok(None)
    }

    fn acquire_handle(&mut self) -> Result<Option<DescriptorSetHandle>> {
        if let Some(handle) = self.ready.pop_front() {
            return Ok(Some(handle));
        }
        if let Some(handle) = self.retirement.acquire()? {
            return Ok(Some(handle));
        };
        Ok(None)
    }

    pub fn retire(&mut self, freed: FreedDescriptorSet) -> Result<()> {
        let (set, retire) = freed.into_parts();
        let handle = set.handle();

        if handle.pool().allocator_id != self.id {
            return Err(anyhow!("descriptor pool allocator mismatch"));
        }

        // TODO: why get_mut here by indexing elsewhere?
        let pool = self
            .pools
            .get_mut(handle.pool().index)
            .context("pool not found")?;

        if let Some(retire) = retire {
            if let Err(e) = self.retirement.retire(retire) {
                return Err(e);
            }
        } else {
            self.ready.push_back(*handle);
        }

        pool.unretired += 1;

        Ok(())
    }
}

pub(crate) struct DescriptorPool {
    id: DescriptorPoolId,
    owned: OwnedDescriptorPool, // TODO XXX: ResourceArena is kinda annoying
    unretired: usize,
}

impl DescriptorPool {
    pub(crate) fn id(&self) -> DescriptorPoolId {
        self.id
    }

    // TODO: ensure consistent naming everywhere when re-evaluating VulkanHandle
    // and ResourceArena
    pub(crate) fn owned(&self) -> &OwnedDescriptorPool {
        &self.owned
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DescriptorPoolId {
    allocator_id: usize, // TODO: AllocId?
    index: Index,
}
