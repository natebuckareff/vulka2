use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::VulkanResource;

pub struct OwnedInstance {
    instance: Arc<vulkanalia::Instance>,
}

impl OwnedInstance {
    pub fn new(entry: &vulkanalia::Entry, info: &vk::InstanceCreateInfoBuilder) -> Result<Self> {
        let instance = unsafe { entry.create_instance(&info, None)? };
        let instance = Arc::new(instance);
        Ok(Self { instance })
    }
}

impl VulkanResource for OwnedInstance {
    type Raw = Arc<vulkanalia::Instance>;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.instance
    }
}

impl Drop for OwnedInstance {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}
