use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu::{VulkanHandle, VulkanResource};

pub struct OwnedImageView {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    handle: vk::ImageView,
}

impl OwnedImageView {
    pub fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        info: &vk::ImageViewCreateInfoBuilder,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let handle = unsafe { device.raw().create_image_view(info, None)? };
        Ok(Self { device, handle })
    }
}

impl VulkanResource for OwnedImageView {
    type Raw = vk::ImageView;
    unsafe fn raw(&self) -> &Self::Raw {
        &self.handle
    }
}

impl Deref for OwnedImageView {
    type Target = vk::ImageView;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl Drop for OwnedImageView {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.device.raw().destroy_image_view(self.handle, None);
        }
    }
}
