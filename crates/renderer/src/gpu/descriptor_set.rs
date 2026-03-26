use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu::{
    BufferSpan, DescriptorPool, DescriptorPoolId, DescriptorSetLayout, Device, ParameterBlock,
    ParameterWriter, VulkanResource,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DescriptorSetId(usize);

impl From<usize> for DescriptorSetId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

pub struct DescriptorSet {
    handle: DescriptorSetHandle,
    set_layout: Arc<DescriptorSetLayout>,
    set: vk::DescriptorSet,
}

impl DescriptorSet {
    pub(crate) fn new(
        id: DescriptorSetId,
        device: &Arc<Device>,
        pool: &DescriptorPool,
        set_layout: Arc<DescriptorSetLayout>,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let device = device.handle();

        let set_layouts = &[unsafe { *set_layout.owned().raw() }];
        let info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(unsafe { *pool.owned().raw() })
            .set_layouts(set_layouts);

        let descriptor_sets = unsafe { device.raw().allocate_descriptor_sets(&info)? };
        let set = descriptor_sets[0];

        let handle = DescriptorSetHandle {
            id,
            pool: pool.id(),
        };

        Ok(Self {
            handle,
            set_layout,
            set,
        })
    }

    pub fn handle(&self) -> &DescriptorSetHandle {
        &self.handle
    }

    pub fn set_layout(&self) -> &Arc<DescriptorSetLayout> {
        &self.set_layout
    }

    pub fn writer<Handle: Copy>(self, ubo: Option<BufferSpan<Handle>>) -> ParameterWriter<Handle> {
        let ubo = ubo.map(|span| span.writer());
        ParameterWriter::new(self, ubo)
    }

    pub fn object<Handle: Copy>(self, ubo: Option<BufferSpan<Handle>>) -> ParameterBlock<Handle> {
        let writer = self.writer(ubo);
        ParameterBlock::new(writer)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorSetHandle {
    id: DescriptorSetId,
    pool: DescriptorPoolId,
}

impl DescriptorSetHandle {
    pub fn id(&self) -> DescriptorSetId {
        self.id
    }

    pub fn pool(&self) -> DescriptorPoolId {
        self.pool
    }
}
