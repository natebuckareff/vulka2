use std::ops::Deref;
use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::VulkanDevice;

pub struct OwnedImageView {
    device: Arc<VulkanDevice>,
    handle: vk::ImageView,
}

impl OwnedImageView {
    pub fn new(device: Arc<VulkanDevice>, info: &vk::ImageViewCreateInfoBuilder) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let handle = unsafe { device.create_image_view(info, None)? };
        Ok(Self { device, handle })
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
            self.device.destroy_image_view(self.handle, None);
        }
    }
}
