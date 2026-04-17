use std::sync::Arc;

use anyhow::bail;
use anyhow::{Result, anyhow};

use crate::gal::image_token::ImageToken;
use crate::gal::image_view::ImageView;
use crate::gal::render_targets::{ColorTarget, DepthTarget, RenderTargets, StencilTarget};
use crate::gal::swapchain_v2::SwapchainToken;

pub struct BoundRenderTargets<'a> {
    targets: &'a RenderTargets,
    colors: Box<[BoundColorTarget<'a>]>,
    depth: Option<BoundDepthTarget<'a>>,
    stencil: Option<BoundStencilTarget<'a>>,
}

impl<'a> BoundRenderTargets<'a> {
    pub(crate) fn new(
        targets: &'a RenderTargets,
        colors: Box<[BoundColorTarget<'a>]>,
        depth: Option<BoundDepthTarget<'a>>,
        stencil: Option<BoundStencilTarget<'a>>,
    ) -> Self {
        Self {
            targets,
            colors,
            depth,
            stencil,
        }
    }

    pub fn targets(&self) -> &RenderTargets {
        self.targets
    }

    pub fn colors(&self) -> &[BoundColorTarget<'a>] {
        &self.colors
    }

    pub fn colors_mut(&mut self) -> &mut [BoundColorTarget<'a>] {
        &mut self.colors
    }

    pub fn depth(&self) -> Option<&BoundDepthTarget<'a>> {
        self.depth.as_ref()
    }

    pub fn depth_mut(&mut self) -> Option<&mut BoundDepthTarget<'a>> {
        self.depth.as_mut()
    }

    pub fn stencil(&self) -> Option<&BoundStencilTarget<'a>> {
        self.stencil.as_ref()
    }

    pub fn stencil_mut(&mut self) -> Option<&mut BoundStencilTarget<'a>> {
        self.stencil.as_mut()
    }
}

pub struct BoundRenderTargetsBuilder<'a> {
    targets: &'a RenderTargets,
    colors: Vec<Option<&'a mut ImageToken>>,
    depth: Option<&'a mut ImageToken>,
    stencil: Option<&'a mut ImageToken>,
}

impl<'a> BoundRenderTargetsBuilder<'a> {
    pub(crate) fn new(targets: &'a RenderTargets) -> Self {
        let mut colors = Vec::with_capacity(targets.colors().len());
        colors.resize_with(targets.colors().len(), || None);
        Self {
            targets,
            colors,
            depth: None,
            stencil: None,
        }
    }

    pub fn color(mut self, index: usize, token: &'a mut ImageToken) -> Result<Self> {
        let Some(target) = self.targets.colors().get(index) else {
            return Err(anyhow!("invalid color target index"));
        };
        validate_binding(target, token)?;
        self.colors[index] = Some(token);
        Ok(self)
    }

    pub fn swapchain_color(mut self, index: usize, token: &'a mut SwapchainToken) -> Result<Self> {
        let Some(target) = self.targets.colors().get(index) else {
            return Err(anyhow!("invalid color target index"));
        };
        let (_, image_token) = token.bind()?;
        validate_binding(target, image_token)?;
        self.colors[index] = Some(image_token);
        Ok(self)
    }

    pub fn depth(mut self, token: &'a mut ImageToken) -> Result<Self> {
        let Some(target) = self.targets.depth() else {
            return Err(anyhow!("render targets do not have a depth attachment"));
        };
        validate_binding(target, token)?;
        self.depth = Some(token);
        Ok(self)
    }

    pub fn stencil(mut self, token: &'a mut ImageToken) -> Result<Self> {
        let Some(target) = self.targets.stencil() else {
            return Err(anyhow!("render targets do not have a stencil attachment"));
        };
        validate_binding(target, token)?;
        self.stencil = Some(token);
        Ok(self)
    }

    pub fn build(self) -> Result<BoundRenderTargets<'a>> {
        let Self {
            targets,
            colors: pending_colors,
            depth,
            stencil,
        } = self;

        let mut colors = Vec::with_capacity(targets.colors().len());
        for (target, token) in targets.colors().iter().zip(pending_colors.into_iter()) {
            let Some(token) = token else {
                return Err(anyhow!("missing color target binding"));
            };
            colors.push(BoundColorTarget::new(target, token));
        }

        let depth = match (targets.depth(), depth) {
            (Some(target), Some(token)) => Some(BoundDepthTarget::new(target, token)),
            (Some(_), None) => return Err(anyhow!("missing depth target binding")),
            (None, Some(_)) => return Err(anyhow!("unexpected depth target binding")),
            (None, None) => None,
        };

        let stencil = match (targets.stencil(), stencil) {
            (Some(target), Some(token)) => Some(BoundStencilTarget::new(target, token)),
            (Some(_), None) => return Err(anyhow!("missing stencil target binding")),
            (None, Some(_)) => return Err(anyhow!("unexpected stencil target binding")),
            (None, None) => None,
        };

        Ok(BoundRenderTargets::new(
            targets,
            colors.into_boxed_slice(),
            depth,
            stencil,
        ))
    }
}

pub struct BoundColorTarget<'a> {
    target: &'a ColorTarget,
    token: &'a mut ImageToken,
}

impl<'a> BoundColorTarget<'a> {
    fn new(target: &'a ColorTarget, token: &'a mut ImageToken) -> Self {
        Self { target, token }
    }

    pub fn target(&self) -> &ColorTarget {
        self.target
    }

    pub fn token(&self) -> &ImageToken {
        self.token
    }

    pub fn token_mut(&mut self) -> &mut ImageToken {
        self.token
    }
}

pub struct BoundDepthTarget<'a> {
    target: &'a DepthTarget,
    token: &'a mut ImageToken,
}

impl<'a> BoundDepthTarget<'a> {
    fn new(target: &'a DepthTarget, token: &'a mut ImageToken) -> Self {
        Self { target, token }
    }

    pub fn target(&self) -> &DepthTarget {
        self.target
    }

    pub fn token(&self) -> &ImageToken {
        self.token
    }

    pub fn token_mut(&mut self) -> &mut ImageToken {
        self.token
    }
}

pub struct BoundStencilTarget<'a> {
    target: &'a StencilTarget,
    token: &'a mut ImageToken,
}

impl<'a> BoundStencilTarget<'a> {
    fn new(target: &'a StencilTarget, token: &'a mut ImageToken) -> Self {
        Self { target, token }
    }

    pub fn target(&self) -> &StencilTarget {
        self.target
    }

    pub fn token(&self) -> &ImageToken {
        self.token
    }

    pub fn token_mut(&mut self) -> &mut ImageToken {
        self.token
    }
}

fn validate_binding<T>(target: &T, token: &ImageToken) -> Result<()>
where
    T: AttachmentView,
{
    // NOTE: `ImageSubresource` is meant to be a strong identity based handle
    // for a corresponding `ImageSpan`; checking ptr equality here is a bit of a
    // hack, but works for now
    let view = target.view();
    let target_subresource = view.subresource();
    let token_subresource = token.subresource();
    if !std::sync::Arc::ptr_eq(target_subresource, token_subresource) {
        bail!("image token does not match render target image span");
    }
    Ok(())
}

trait AttachmentView {
    fn view(&self) -> &Arc<ImageView>;
}

impl AttachmentView for ColorTarget {
    fn view(&self) -> &Arc<ImageView> {
        self.view()
    }
}

impl AttachmentView for DepthTarget {
    fn view(&self) -> &Arc<ImageView> {
        self.view()
    }
}

impl AttachmentView for StencilTarget {
    fn view(&self) -> &Arc<ImageView> {
        self.view()
    }
}
