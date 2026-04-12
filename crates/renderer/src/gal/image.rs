use std::sync::Arc;

use anyhow::{Result, anyhow};
use vulkanalia::vk;
use vulkanalia_vma as vma;

use crate::gal::{Device, image_storage::ImageStorage, swapchain_resource::SwapchainResource};

pub struct Image {
    storage: ImageStorage,
    image_type: vk::ImageType,
    format: vk::Format,
    extent: vk::Extent3D,
    mip_levels: u32,
    array_layers: u32,
    samples: SampleCount,
    tiling: vk::ImageTiling,
    usage: vk::ImageUsageFlags,
}

impl Image {
    pub fn new(
        device: Arc<Device>,
        flags: vma::AllocationCreateFlags,
        image_type: vk::ImageType,
        format: vk::Format,
        extent: vk::Extent3D,
        mip_levels: u32,
        array_layers: u32,
        samples: SampleCount,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
        initial_layout: vk::ImageLayout,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let info = vk::ImageCreateInfo::builder()
            .image_type(image_type)
            .format(format)
            .extent(extent)
            .mip_levels(mip_levels)
            .array_layers(array_layers)
            .samples(samples.flags())
            .tiling(tiling)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(initial_layout);

        let storage = ImageStorage::from_device(device, info, flags)?;

        Ok(Self {
            storage,
            image_type,
            format,
            extent,
            mip_levels,
            array_layers,
            samples,
            tiling,
            usage,
        })
    }

    pub(crate) fn from_swapchain(
        resource: Arc<SwapchainResource>,
        image: vk::Image,
        image_type: vk::ImageType,
        format: vk::Format,
        extent: vk::Extent3D,
        mip_levels: u32,
        array_layers: u32,
        samples: SampleCount,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
    ) -> Self {
        let storage = ImageStorage::from_swapchain(resource, image);
        Self {
            storage,
            image_type,
            format,
            extent,
            mip_levels,
            array_layers,
            samples,
            tiling,
            usage,
        }
    }

    pub(crate) unsafe fn storage(&self) -> &ImageStorage {
        &self.storage
    }

    pub fn image_type(&self) -> vk::ImageType {
        self.image_type
    }

    pub fn dimensions(&self) -> Result<u32> {
        let value = match self.image_type {
            vk::ImageType::_1D => 1,
            vk::ImageType::_2D => 2,
            vk::ImageType::_3D => 3,
            _ => return Err(anyhow!("invalid view type")),
        };
        Ok(value)
    }

    pub fn format(&self) -> vk::Format {
        self.format
    }

    pub fn extent(&self) -> vk::Extent3D {
        self.extent
    }

    pub fn mip_levels(&self) -> u32 {
        self.mip_levels
    }

    pub fn array_layers(&self) -> u32 {
        self.array_layers
    }

    pub fn samples(&self) -> SampleCount {
        self.samples
    }

    pub fn tiling(&self) -> vk::ImageTiling {
        self.tiling
    }

    pub fn usage(&self) -> vk::ImageUsageFlags {
        self.usage
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SampleCount {
    flags: vk::SampleCountFlags,
    count: u32,
}

impl SampleCount {
    pub fn new(count: u32) -> Result<Self> {
        let flags = match count {
            1 => vk::SampleCountFlags::_1,
            2 => vk::SampleCountFlags::_2,
            4 => vk::SampleCountFlags::_4,
            8 => vk::SampleCountFlags::_8,
            16 => vk::SampleCountFlags::_16,
            32 => vk::SampleCountFlags::_32,
            64 => vk::SampleCountFlags::_64,
            _ => return Err(anyhow!("invalid sample count")),
        };
        Ok(Self { flags, count })
    }

    pub fn flags(&self) -> vk::SampleCountFlags {
        self.flags
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn sample_mask_count(&self) -> usize {
        match self.flags {
            vk::SampleCountFlags::_64 => 2,
            _ => 1,
        }
    }
}
