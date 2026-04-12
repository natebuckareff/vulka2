use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::{device::DeviceResource, image::Image};

pub struct ImageViewResource {
    device: Arc<DeviceResource>,
    image: Arc<Image>,
    handle: vk::ImageView,
}

impl ImageViewResource {
    pub(crate) fn new(
        device: Arc<DeviceResource>,
        image: Arc<Image>,
        info: &vk::ImageViewCreateInfoBuilder,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let handle = unsafe { device.handle().create_image_view(info, None)? };
        Ok(Self {
            device,
            image,
            handle,
        })
    }

    pub(crate) unsafe fn handle(&self) -> vk::ImageView {
        self.handle
    }
}

impl Drop for ImageViewResource {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.device.handle().destroy_image_view(self.handle, None);
        }
    }
}
