use std::sync::Arc;

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{Image, OwnedImageView, SampleCount};

pub struct ImageView {
    image: Arc<Image>,
    owned: OwnedImageView,
    view_type: vk::ImageViewType,
    format: vk::Format,
    components: vk::ComponentMapping,
    subresource_range: vk::ImageSubresourceRange,
}

impl ImageView {
    pub fn new(
        image: Arc<Image>,
        view_type: vk::ImageViewType,
        format: vk::Format,
        components: vk::ComponentMapping,
        subresource_range: vk::ImageSubresourceRange,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let info = vk::ImageViewCreateInfo::builder()
            .image(unsafe { image.raw() })
            .view_type(view_type)
            .format(format)
            .components(components)
            .subresource_range(subresource_range);

        let device = image.device().handle().clone();
        let owned = OwnedImageView::new(device, &info)?;

        Ok(Self {
            image,
            owned,
            view_type,
            format,
            components,
            subresource_range,
        })
    }

    pub(crate) fn owned(&self) -> &OwnedImageView {
        &self.owned
    }

    pub fn view_type(&self) -> vk::ImageViewType {
        self.view_type
    }

    pub fn dimensions(&self) -> Result<u32> {
        let value = match self.view_type {
            vk::ImageViewType::_1D => 1,
            vk::ImageViewType::_1D_ARRAY => 2,
            vk::ImageViewType::_2D => 2,
            vk::ImageViewType::_2D_ARRAY => 3,
            vk::ImageViewType::_3D => 3,
            vk::ImageViewType::CUBE => 3,
            vk::ImageViewType::CUBE_ARRAY => 4,
            _ => return Err(anyhow!("invalid view type")),
        };
        Ok(value)
    }

    pub fn format(&self) -> vk::Format {
        self.format
    }

    pub fn components(&self) -> vk::ComponentMapping {
        self.components
    }

    pub fn subresource_range(&self) -> vk::ImageSubresourceRange {
        self.subresource_range
    }

    pub fn samples(&self) -> SampleCount {
        self.image.samples()
    }
}
