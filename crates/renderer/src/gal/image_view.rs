use std::sync::Arc;

use anyhow::{Result, bail};
use vulkanalia::vk;

use crate::gal::{
    Device,
    image::{Image, SampleCount},
    image_view_resource::ImageViewResource,
};

pub struct ImageView {
    resource: ImageViewResource,
    view_type: vk::ImageViewType,
    format: vk::Format,
    components: vk::ComponentMapping,
    subresource_range: vk::ImageSubresourceRange,
    samples: SampleCount,
}

impl ImageView {
    pub fn new(
        device: Arc<Device>,
        image: Arc<Image>,
        view_type: vk::ImageViewType,
        format: vk::Format,
        components: vk::ComponentMapping,
        subresource_range: vk::ImageSubresourceRange,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let info = vk::ImageViewCreateInfo::builder()
            .image(unsafe { image.storage().handle() })
            .view_type(view_type)
            .format(format)
            .components(components)
            .subresource_range(subresource_range);

        let device = device.resource().clone();
        let resource = ImageViewResource::new(device, image.clone(), &info)?;
        let samples = image.samples();

        Ok(Self {
            resource,
            view_type,
            format,
            components,
            subresource_range,
            samples,
        })
    }

    pub(crate) fn resource(&self) -> &ImageViewResource {
        &self.resource
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
            _ => bail!("invalid view type"),
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
        self.samples
    }
}
