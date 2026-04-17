use std::sync::Arc;

use vulkanalia::vk;

use crate::gal::image_span::ImageSubresource;
use crate::gal::queue::Lane;
use crate::gal::usage_token::UsageToken;

pub struct ImageToken {
    subresource: Arc<ImageSubresource>,
    state: ImageState,
    usage: UsageToken,
}

impl ImageToken {
    pub(crate) fn new(subresource: Arc<ImageSubresource>, state: ImageState) -> Self {
        Self {
            subresource,
            state,
            usage: UsageToken::new(),
        }
    }

    pub fn subresource(&self) -> &Arc<ImageSubresource> {
        &self.subresource
    }

    pub fn state(&self) -> ImageState {
        self.state
    }

    pub fn set_state(&mut self, state: ImageState) {
        self.state = state;
    }

    pub fn usage(&self) -> &UsageToken {
        &self.usage
    }

    pub fn usage_mut(&mut self) -> &mut UsageToken {
        &mut self.usage
    }

    pub fn touch(&mut self, frame: u32, lane: Lane) {
        self.usage.touch(frame, lane);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ImageState {
    owner: Option<Lane>,
    layout: vk::ImageLayout,
    access: ImageAccess,
}

impl ImageState {
    pub fn new(owner: Option<Lane>, layout: vk::ImageLayout, access: ImageAccess) -> Self {
        Self {
            owner,
            layout,
            access,
        }
    }

    pub fn owner(&self) -> Option<Lane> {
        self.owner
    }

    pub fn layout(&self) -> vk::ImageLayout {
        self.layout
    }

    pub fn access(&self) -> ImageAccess {
        self.access
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ImageAccess {
    Uninitialized,
    Present,
    TransferRead,
    TransferWrite,
    SampledRead,
    StorageRead,
    StorageWrite,
    ColorAttachmentWrite,
    DepthStencilAttachmentRead,
    DepthStencilAttachmentWrite,
}
