use std::sync::Arc;

use vulkanalia::vk;

use crate::gal::{
    image::Image,
    image_view::ImageView,
    queue::QueueFamily,
    semaphore_resource::SemaphoreResource,
};

pub struct AcquiredImage {
    generation: u64,
    index: u32,
    family: QueueFamily,
    image: Arc<Image>,
    view: Arc<ImageView>,
    extent: vk::Extent2D,
    format: vk::Format,
    image_available: Arc<SemaphoreResource>,
    render_finished: Arc<SemaphoreResource>,
}

impl AcquiredImage {
    pub(crate) fn new(
        generation: u64,
        index: u32,
        family: QueueFamily,
        image: Arc<Image>,
        view: Arc<ImageView>,
        extent: vk::Extent2D,
        format: vk::Format,
        image_available: Arc<SemaphoreResource>,
        render_finished: Arc<SemaphoreResource>,
    ) -> Self {
        Self {
            generation,
            index,
            family,
            image,
            view,
            extent,
            format,
            image_available,
            render_finished,
        }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn image(&self) -> &Arc<Image> {
        &self.image
    }

    pub fn view(&self) -> &Arc<ImageView> {
        &self.view
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.extent
    }

    pub fn format(&self) -> vk::Format {
        self.format
    }

    pub fn image_available(&self) -> &Arc<SemaphoreResource> {
        &self.image_available
    }

    pub fn render_finished(&self) -> &Arc<SemaphoreResource> {
        &self.render_finished
    }

    pub fn into_present_token(self) -> PresentToken {
        PresentToken {
            generation: self.generation,
            index: self.index,
            family: self.family,
            render_finished: self.render_finished,
        }
    }
}

pub struct PresentToken {
    generation: u64,
    index: u32,
    family: QueueFamily,
    render_finished: Arc<SemaphoreResource>,
}

impl PresentToken {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn family(&self) -> QueueFamily {
        self.family
    }

    pub fn render_finished(&self) -> &Arc<SemaphoreResource> {
        &self.render_finished
    }
}
