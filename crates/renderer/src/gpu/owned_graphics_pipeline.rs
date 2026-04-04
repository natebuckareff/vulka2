use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0};

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedGraphicsPipeline {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    pipeline: vk::Pipeline,
}

impl OwnedGraphicsPipeline {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: vk::GraphicsPipelineCreateInfoBuilder,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let infos = &[info];
        // ignore code; not using CREATE_FAIL_ON_PIPELINE_COMPILE_REQUIRED
        let (pipelines, _) = unsafe {
            device
                .raw()
                .create_graphics_pipelines(vk::PipelineCache::null(), infos, None)?
        };
        let pipeline = pipelines[0];
        Ok(Self { device, pipeline })
    }
}

impl VulkanResource for OwnedGraphicsPipeline {
    type Raw = vk::Pipeline;

    unsafe fn raw(&self) -> &Self::Raw {
        &self.pipeline
    }
}

impl Drop for OwnedGraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.raw().destroy_pipeline(self.pipeline, None);
        }
    }
}
