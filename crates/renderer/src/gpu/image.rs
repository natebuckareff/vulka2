use std::sync::Arc;

use anyhow::{Result, anyhow};
use vulkanalia::vk;
use vulkanalia_vma as vma;

use crate::gpu::{Device, QueueFamilyId, RetireToken};

pub struct Image {
    device: Arc<Device>,
    image: vk::Image,
    allocation: vma::Allocation,
    image_type: vk::ImageType,
    format: vk::Format,
    extent: vk::Extent3D,
    mip_levels: u32,
    array_layers: u32,
    samples: SampleCount,
    tiling: vk::ImageTiling,
    usage: vk::ImageUsageFlags,
    flags: vma::AllocationCreateFlags,
}

impl Image {
    pub fn new(
        device: Arc<Device>,
        image_type: vk::ImageType,
        format: vk::Format,
        extent: vk::Extent3D,
        mip_levels: u32,
        array_layers: u32,
        samples: SampleCount,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
        flags: vma::AllocationCreateFlags,
        initial_layout: vk::ImageLayout,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        use vulkanalia_vma::Alloc;

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

        let options = vma::AllocationOptions {
            flags,
            ..Default::default()
        };

        let gpu_allocator = device.gpu_allocator();
        let (image, allocation) = unsafe { gpu_allocator.raw().create_image(info, &options)? };

        Ok(Self {
            device,
            image,
            allocation,
            image_type,
            format,
            extent,
            mip_levels,
            array_layers,
            samples,
            tiling,
            usage,
            flags,
        })
    }

    // TODO: why not OwnedImage?
    pub(crate) unsafe fn raw(&self) -> vk::Image {
        self.image
    }

    pub fn device(&self) -> &Arc<Device> {
        &self.device
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

    pub fn flags(&self) -> vma::AllocationCreateFlags {
        self.flags
    }
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.image == other.image
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        let gpu_allocator = self.device.gpu_allocator();
        unsafe {
            gpu_allocator
                .raw()
                .destroy_image(self.image, self.allocation);
        }
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

// XXX
pub struct ImageToken {
    owner: QueueFamilyId,
    retire: RetireToken<()>, // XXX
    image: Arc<Image>,
    subresource: vk::ImageSubresourceRange,
    layout: vk::ImageLayout,
    access: ImageAccess,
}

impl ImageToken {
    pub fn owner(&self) -> QueueFamilyId {
        self.owner
    }

    pub fn retire(&self) -> &RetireToken<()> {
        &self.retire
    }

    pub fn image(&self) -> &Arc<Image> {
        &self.image
    }

    pub fn subresource(&self) -> &vk::ImageSubresourceRange {
        &self.subresource
    }

    pub fn layout(&self) -> vk::ImageLayout {
        self.layout
    }

    pub fn access(&self) -> ImageAccess {
        self.access
    }
}

// XXX
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ImageAccess {
    TransferRead,
    TransferWrite,
    SampledRead,
    StorageRead,
    StorageWrite,
    ColorAttachmentWrite,
    DepthStencilAttachmentRead,
    DepthStencilAttachmentWrite,
}
