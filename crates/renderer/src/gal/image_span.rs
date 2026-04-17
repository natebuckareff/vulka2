use std::sync::Arc;

use anyhow::{Result, bail};
use vulkanalia::vk;

use crate::gal::image::Image;
use crate::gal::image_token::{ImageAccess, ImageState, ImageToken};
use crate::gal::image_view::ImageView;

pub struct ImageSpan {
    subresource: Arc<ImageSubresource>,
    state: TokenState,
}

impl ImageSpan {
    pub(crate) fn new(image: Arc<Image>, range: vk::ImageSubresourceRange) -> Result<Self> {
        validate_subresource_range(image.as_ref(), range)?;
        let subresource = Arc::new(ImageSubresource::new(image, range));
        Ok(Self {
            subresource,
            state: TokenState::Initial,
        })
    }

    pub fn subresource(&self) -> &Arc<ImageSubresource> {
        &self.subresource
    }

    pub fn view(
        &self,
        view_type: vk::ImageViewType,
        format: vk::Format,
        components: vk::ComponentMapping,
    ) -> Result<ImageView> {
        let subresource = self.subresource.clone();
        ImageView::new(subresource, view_type, format, components)
    }

    pub fn acquire(&mut self) -> Result<Option<ImageToken>> {
        self.acquire_poll_or_wait(true)
    }

    pub fn acquire_wait(&mut self) -> Result<Option<ImageToken>> {
        self.acquire_poll_or_wait(false)
    }

    fn acquire_poll_or_wait(&mut self, poll: bool) -> Result<Option<ImageToken>> {
        let state = std::mem::replace(&mut self.state, TokenState::Acquired);
        match state {
            TokenState::Initial => {
                let layout = vk::ImageLayout::UNDEFINED;
                let access = ImageAccess::Uninitialized;
                let state = ImageState::new(None, layout, access); // TODO: ImageState::uninitialized()?
                let token = ImageToken::new(self.subresource.clone(), state);
                Ok(Some(token))
            }
            TokenState::Acquired => {
                self.state = TokenState::Acquired;
                Ok(None)
            }
            TokenState::Retired(token) => {
                let usage = token.usage();
                let device = self.subresource.image().device();
                if !usage.is_reclaimable_poll_or_wait(device, poll)? {
                    self.state = TokenState::Retired(token);
                    return Ok(None);
                }
                Ok(Some(token))
            }
        }
    }

    pub fn retire(&mut self, token: ImageToken) -> Result<()> {
        use TokenState::*;
        match self.state {
            Initial => bail!("cannot retire image token before acquisition"),
            Retired(_) => bail!("cannot retire image token when one is already retired"),
            Acquired => {}
        }
        if !Arc::ptr_eq(self.subresource(), token.subresource()) {
            bail!("image token does not match image span");
        }
        self.state = TokenState::Retired(token);
        Ok(())
    }
}

pub struct ImageSubresource {
    image: Arc<Image>,
    range: vk::ImageSubresourceRange,
}

impl ImageSubresource {
    fn new(image: Arc<Image>, range: vk::ImageSubresourceRange) -> Self {
        Self { image, range }
    }

    pub fn image(&self) -> &Arc<Image> {
        &self.image
    }

    pub fn range(&self) -> vk::ImageSubresourceRange {
        self.range
    }

    pub fn subresource_range(&self) -> vk::ImageSubresourceRange {
        self.range()
    }
}

enum TokenState {
    Initial,
    Acquired,
    Retired(ImageToken),
}

fn validate_subresource_range(image: &Image, range: vk::ImageSubresourceRange) -> Result<()> {
    if range.level_count == 0 {
        bail!("subresource range level_count must be non-zero");
    }

    if range.layer_count == 0 {
        bail!("subresource range layer_count must be non-zero");
    }

    if range.aspect_mask.is_empty() {
        bail!("subresource range aspect_mask must be non-empty");
    }

    let mip_end = range
        .base_mip_level
        .checked_add(range.level_count)
        .ok_or_else(|| anyhow::anyhow!("subresource range mip level overflow"))?;

    if mip_end > image.mip_levels() {
        bail!("subresource range mip levels out of bounds");
    }

    let layer_end = range
        .base_array_layer
        .checked_add(range.layer_count)
        .ok_or_else(|| anyhow::anyhow!("subresource range array layer overflow"))?;

    if layer_end > image.array_layers() {
        bail!("subresource range array layers out of bounds");
    }

    Ok(())
}
