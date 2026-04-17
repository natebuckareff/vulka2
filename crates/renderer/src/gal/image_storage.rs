use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;
use vulkanalia_vma as vma;

use crate::gal::{Device, swapchain_resource::SwapchainResource};

pub enum ImageStorage {
    Device {
        device: Arc<Device>,
        allocation: vma::Allocation,
        image: vk::Image,
    },
    Swapchain {
        resource: Arc<SwapchainResource>,
        image: vk::Image,
    },
}

impl ImageStorage {
    pub(crate) fn from_device(
        device: Arc<Device>,
        info: vk::ImageCreateInfoBuilder,
        flags: vma::AllocationCreateFlags,
    ) -> Result<Self> {
        use vulkanalia_vma::Alloc;
        let options = vma::AllocationOptions {
            flags,
            ..Default::default()
        };
        let allocator = device.allocator();
        let (image, allocation) = unsafe { allocator.handle().create_image(info, &options)? };
        Ok(Self::Device {
            device,
            allocation,
            image,
        })
    }

    pub(crate) fn from_swapchain(resource: Arc<SwapchainResource>, image: vk::Image) -> Self {
        Self::Swapchain { resource, image }
    }

    pub(crate) unsafe fn handle(&self) -> vk::Image {
        match self {
            ImageStorage::Device { image, .. } => *image,
            ImageStorage::Swapchain { image, .. } => *image,
        }
    }
}

impl PartialEq for ImageStorage {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.handle() == other.handle() }
    }
}

impl Drop for ImageStorage {
    fn drop(&mut self) {
        match self {
            ImageStorage::Device {
                device,
                allocation,
                image,
            } => {
                let allocator = device.allocator();
                unsafe {
                    allocator.handle().destroy_image(*image, *allocation);
                }
            }
            ImageStorage::Swapchain { .. } => {}
        }
    }
}
