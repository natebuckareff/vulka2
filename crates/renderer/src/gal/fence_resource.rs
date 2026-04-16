use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;

pub struct FenceResource {
    device: Arc<DeviceResource>,
    handle: vk::Fence,
}

impl FenceResource {
    fn new(device: Arc<DeviceResource>, info: &vk::FenceCreateInfoBuilder) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let handle = unsafe { device.handle().create_fence(info, None)? };
        Ok(Self { device, handle })
    }

    pub(crate) fn unsignalled(device: Arc<DeviceResource>) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        let info = vk::FenceCreateInfo::builder();
        Self::new(device, &info)
    }

    pub(crate) fn device(&self) -> &Arc<DeviceResource> {
        &self.device
    }

    pub(crate) unsafe fn handle(&self) -> vk::Fence {
        self.handle
    }

    pub(crate) fn is_signalled(&self) -> Result<bool> {
        use vulkanalia::prelude::v1_0::*;
        let status = unsafe { self.device.handle().get_fence_status(self.handle)? };
        Ok(status == vk::SuccessCode::SUCCESS)
    }

    pub(crate) fn reset(&self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            let fences = [self.handle];
            self.device.handle().reset_fences(&fences)?;
        }
        Ok(())
    }
}

impl Drop for FenceResource {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;

        unsafe {
            self.device.handle().destroy_fence(self.handle, None);
        }
    }
}
