use std::{rc::Rc, sync::Arc};

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu::{
    BufferRef, DescriptorPool, DescriptorPoolId, DescriptorSetLayout, Device, OwnedDescriptorPool,
    OwnedDescriptorSetLayout, ParameterBlock, RetireToken, VulkanResource,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DescriptorSetId(usize);

impl From<usize> for DescriptorSetId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

pub struct DescriptorToken {
    pub(crate) retire: RetireToken<DescriptorUsage>,
}

pub struct DescriptorSet {
    pool_id: DescriptorPoolId,
    pool_handle: Rc<OwnedDescriptorPool>,
    layout_handle: Rc<OwnedDescriptorSetLayout>,
    token: DescriptorToken,
    set: vk::DescriptorSet,
}

impl DescriptorSet {
    pub(crate) fn new(
        id: DescriptorSetId,
        device: &Arc<Device>,
        pool: &DescriptorPool,
        layout: &DescriptorSetLayout,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let device = device.handle();

        let set_layouts = &[unsafe { *layout.handle().raw() }];
        let info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(unsafe { *pool.handle().raw() })
            .set_layouts(set_layouts);

        let descriptor_sets = unsafe { device.raw().allocate_descriptor_sets(&info)? };
        let set = descriptor_sets[0];

        let usage = DescriptorUsage {
            id,
            pool: pool.id(),
        };
        let retire = RetireToken::new(usage);
        let token = DescriptorToken { retire };
        let pool_id = pool.id();
        let pool_handle = pool.handle().clone();
        let layout_handle = layout.handle().clone();

        Ok(Self {
            pool_id,
            pool_handle,
            layout_handle,
            token,
            set,
        })
    }

    pub fn parameter_block(self, ubo: Option<BufferRef>) -> ParameterBlock {
        ParameterBlock::new(self, ubo)
    }

    pub fn finish(self) -> FinishedDescriptorSet {
        FinishedDescriptorSet::new(self)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DescriptorUsage {
    pub(crate) id: DescriptorSetId,
    pub(crate) pool: DescriptorPoolId,
}

pub struct FinishedDescriptorSet {
    pub(crate) pool_id: DescriptorPoolId,
    pool_handle: Rc<OwnedDescriptorPool>,
    layout_handle: Rc<OwnedDescriptorSetLayout>,
    pub(crate) token: DescriptorToken,
    set: vk::DescriptorSet,
}

impl FinishedDescriptorSet {
    fn new(set: DescriptorSet) -> Self {
        Self {
            // TODO: do I need all of this?
            pool_id: set.pool_id,
            pool_handle: set.pool_handle,
            layout_handle: set.layout_handle,
            token: set.token,
            set: set.set,
        }
    }
}
