use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk::{self, HasBuilder};

use crate::gpu::{DescriptorSetLayout, Device, OwnedPipelineLayout, PushConstant, VulkanResource};

pub struct PipelineLayout {
    sets: Box<[DescriptorSetLayout]>,
    constants: Box<[PushConstant]>,
    handle: OwnedPipelineLayout,
}

impl PipelineLayout {
    pub fn new(
        device: Arc<Device>,
        sets: Box<[DescriptorSetLayout]>,
        constants: Box<[PushConstant]>,
    ) -> Result<Self> {
        let set_layouts = sets
            .iter()
            .map(|set| unsafe { *set.owned().raw() })
            .collect::<Vec<_>>();

        let push_constant_ranges = constants
            .iter()
            .map(PushConstant::vk_range)
            .collect::<Result<Vec<_>>>()?;

        let info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_constant_ranges);

        let handle = OwnedPipelineLayout::new(device.handle().clone(), &info)?;

        Ok(Self {
            sets,
            constants,
            handle,
        })
    }

    pub fn sets(&self) -> &[DescriptorSetLayout] {
        &self.sets
    }

    pub fn constants(&self) -> &[PushConstant] {
        &self.constants
    }

    pub(crate) fn owned(&self) -> &OwnedPipelineLayout {
        &self.handle
    }
}
