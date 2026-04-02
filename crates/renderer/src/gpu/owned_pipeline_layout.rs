use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0};

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedPipelineLayout {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    layout: vk::PipelineLayout,
}

impl OwnedPipelineLayout {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::PipelineLayoutCreateInfoBuilder,
    ) -> Result<Self> {
        let layout = unsafe { device.raw().create_pipeline_layout(info, None) }?;
        Ok(Self { device, layout })
    }
}

impl VulkanResource for OwnedPipelineLayout {
    type Raw = vk::PipelineLayout;

    unsafe fn raw(&self) -> &Self::Raw {
        &self.layout
    }
}

impl Drop for OwnedPipelineLayout {
    fn drop(&mut self) {
        unsafe {
            self.device.raw().destroy_pipeline_layout(self.layout, None);
        }
    }
}
