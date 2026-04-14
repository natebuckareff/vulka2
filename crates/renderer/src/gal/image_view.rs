use std::sync::Arc;

use anyhow::{Result, bail};
use vulkanalia::vk;

use crate::gal::SampleCount;
use crate::gal::image_span::ImageSubresource;
use crate::gal::image_view_resource::ImageViewResource;

pub struct ImageView {
    subresource: Arc<ImageSubresource>,
    resource: ImageViewResource,
    view_type: vk::ImageViewType,
    format: vk::Format,
    components: vk::ComponentMapping,
    samples: SampleCount,
}

impl ImageView {
    pub(crate) fn new(
        subresource: Arc<ImageSubresource>,
        view_type: vk::ImageViewType,
        format: vk::Format,
        components: vk::ComponentMapping,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let image = subresource.image();
        let range = subresource.range();
        let info = vk::ImageViewCreateInfo::builder()
            .image(unsafe { image.storage().handle() })
            .view_type(view_type)
            .format(format)
            .components(components)
            .subresource_range(range);

        let device = image.device().resource().clone();
        let resource = ImageViewResource::new(device, image.clone(), &info)?;
        let samples = image.samples();

        Ok(Self {
            subresource,
            resource,
            view_type,
            format,
            components,
            samples,
        })
    }

    pub(crate) fn resource(&self) -> &ImageViewResource {
        &self.resource
    }

    pub fn subresource(&self) -> &Arc<ImageSubresource> {
        &self.subresource
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

    pub fn samples(&self) -> SampleCount {
        self.samples
    }
}
