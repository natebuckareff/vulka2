use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::gpu::{
    DescriptorSetLayout, Device, OwnedPipelineLayout, PushConstant, PushConstantData,
    VulkanResource,
};

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
        use vulkanalia::prelude::v1_0::*;

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

    pub(crate) fn validate_push_constant_data(&self, data: &PushConstantData) -> Result<()> {
        let range = data.range();
        let is_valid = self
            .constants
            .iter()
            .map(|constant| constant.matches_range(range))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .any(|matches| matches);

        if !is_valid {
            return Err(anyhow!(
                "push constant range is not declared on this pipeline layout"
            ));
        }

        Ok(())
    }

    pub(crate) fn owned(&self) -> &OwnedPipelineLayout {
        &self.handle
    }
}
