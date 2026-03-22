use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0};

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedDescriptorSetLayout {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    layout: vk::DescriptorSetLayout,
}

impl OwnedDescriptorSetLayout {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::DescriptorSetLayoutCreateInfoBuilder,
    ) -> Result<Self> {
        let layout = unsafe { device.raw().create_descriptor_set_layout(info, None) }?;
        Ok(Self { device, layout })
    }
}

impl VulkanResource for OwnedDescriptorSetLayout {
    type Raw = vk::DescriptorSetLayout;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.layout
    }
}

impl Drop for OwnedDescriptorSetLayout {
    fn drop(&mut self) {
        unsafe {
            self.device
                .raw()
                .destroy_descriptor_set_layout(self.layout, None);
        }
    }
}
