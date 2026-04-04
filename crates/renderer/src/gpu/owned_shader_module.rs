use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0};

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedShaderModule {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    module: vk::ShaderModule,
}

impl OwnedShaderModule {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::ShaderModuleCreateInfoBuilder,
    ) -> Result<Self> {
        let module = unsafe { device.raw().create_shader_module(info, None) }?;
        Ok(Self { device, module })
    }
}

impl VulkanResource for OwnedShaderModule {
    type Raw = vk::ShaderModule;

    unsafe fn raw(&self) -> &Self::Raw {
        &self.module
    }
}

impl Drop for OwnedShaderModule {
    fn drop(&mut self) {
        unsafe {
            self.device.raw().destroy_shader_module(self.module, None);
        }
    }
}
