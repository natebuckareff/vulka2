use std::sync::{Arc, OnceLock};

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{ImageView, SampleCount, VulkanResource};

pub struct RenderTargets {
    layout: Arc<RenderingLayout>,
    area: vk::Rect2D,
    colors: Box<[ColorTarget]>,
    depth: Option<DepthTarget>,
    stencil: Option<StencilTarget>,
    info: OnceLock<RenderTargetsInfo>,
}

impl RenderTargets {
    pub fn new(
        layout: Arc<RenderingLayout>,
        area: vk::Rect2D,
        colors: Box<[ColorTarget]>,
        depth: Option<DepthTarget>,
        stencil: Option<StencilTarget>,
    ) -> Result<Self> {
        // NOTE: if ever adding multi-view rendering, validate layer counts

        layout.validate_color_targets(&colors)?;
        layout.validate_depth_target(depth.as_ref())?;
        layout.validate_stencil_target(stencil.as_ref())?;

        Ok(Self {
            layout,
            area,
            colors,
            depth,
            stencil,
            info: OnceLock::new(),
        })
    }

    pub fn layout(&self) -> &Arc<RenderingLayout> {
        &self.layout
    }

    pub fn area(&self) -> vk::Rect2D {
        self.area
    }

    pub fn layer_count(&self) -> u32 {
        // multi-view rendering not supported
        1
    }

    pub fn colors(&self) -> &[ColorTarget] {
        &self.colors
    }

    pub fn depth(&self) -> Option<&DepthTarget> {
        self.depth.as_ref()
    }

    pub fn stencil(&self) -> Option<&StencilTarget> {
        self.stencil.as_ref()
    }

    pub fn rendering_info(&self) -> &vk::RenderingInfo {
        let info = self.info.get_or_init(|| RenderTargetsInfo::new(self));
        info.rendering_info(self)
    }
}

struct RenderTargetsInfo {
    color_info: Vec<vk::RenderingAttachmentInfo>,
    depth_info: Option<vk::RenderingAttachmentInfo>,
    stencil_info: Option<vk::RenderingAttachmentInfo>,
    rendering_info: OnceLock<vk::RenderingInfo>,
}

impl RenderTargetsInfo {
    fn new(targets: &RenderTargets) -> Self {
        let mut color_info = Vec::with_capacity(targets.colors.len());
        for color in &targets.colors {
            let info = color.target.get_rendering_attachment_info();
            color_info.push(info);
        }

        let mut depth_info = None;
        if let Some(depth) = &targets.depth {
            let info = depth.target.get_rendering_attachment_info();
            depth_info = Some(info);
        }

        let mut stencil_info = None;
        if let Some(stencil) = &targets.stencil {
            let info = stencil.target.get_rendering_attachment_info();
            stencil_info = Some(info);
        }

        Self {
            color_info,
            depth_info,
            stencil_info,
            rendering_info: OnceLock::new(),
        }
    }

    fn rendering_info(&self, targets: &RenderTargets) -> &vk::RenderingInfo {
        use vulkanalia::prelude::v1_0::*;

        self.rendering_info.get_or_init(|| {
            let mut rendering_info = vk::RenderingInfo::builder()
                .flags(vk::RenderingFlags::empty())
                .render_area(targets.area)
                .layer_count(targets.layer_count())
                .view_mask(targets.layout().view_mask())
                .color_attachments(&self.color_info);

            if let Some(info) = &self.depth_info {
                rendering_info = rendering_info.depth_attachment(info);
            }

            if let Some(info) = &self.stencil_info {
                rendering_info = rendering_info.stencil_attachment(info);
            }

            rendering_info.build()
        })
    }
}

pub struct ColorTarget {
    target: AttachmentTarget,
}

impl ColorTarget {
    pub fn new(
        view: Arc<ImageView>,
        layout: vk::ImageLayout,
        load_op: vk::AttachmentLoadOp,
        store_op: vk::AttachmentStoreOp,
        clear_color: vk::ClearColorValue,
    ) -> Self {
        let clear = vk::ClearValue { color: clear_color };
        let target = AttachmentTarget {
            view,
            layout,
            load_op,
            store_op,
            clear,
        };
        Self { target }
    }

    pub fn view(&self) -> &Arc<ImageView> {
        &self.target.view
    }

    pub fn layout(&self) -> vk::ImageLayout {
        self.target.layout
    }

    pub fn load_op(&self) -> vk::AttachmentLoadOp {
        self.target.load_op
    }

    pub fn store_op(&self) -> vk::AttachmentStoreOp {
        self.target.store_op
    }

    pub fn clear_color(&self) -> vk::ClearColorValue {
        unsafe { self.target.clear.color }
    }
}

pub struct DepthTarget {
    target: AttachmentTarget,
}

impl DepthTarget {
    pub fn new(
        view: Arc<ImageView>,
        layout: vk::ImageLayout,
        load_op: vk::AttachmentLoadOp,
        store_op: vk::AttachmentStoreOp,
        clear_value: f32,
    ) -> Self {
        let depth_stencil = vk::ClearDepthStencilValue {
            depth: clear_value,
            ..Default::default()
        };
        let clear = vk::ClearValue { depth_stencil };
        let target = AttachmentTarget {
            view,
            layout,
            load_op,
            store_op,
            clear,
        };
        Self { target }
    }

    pub fn view(&self) -> &Arc<ImageView> {
        &self.target.view
    }

    pub fn layout(&self) -> vk::ImageLayout {
        self.target.layout
    }

    pub fn load_op(&self) -> vk::AttachmentLoadOp {
        self.target.load_op
    }

    pub fn store_op(&self) -> vk::AttachmentStoreOp {
        self.target.store_op
    }

    pub fn clear_depth(&self) -> f32 {
        unsafe { self.target.clear.depth_stencil.depth }
    }
}

pub struct StencilTarget {
    target: AttachmentTarget,
}

impl StencilTarget {
    pub fn new(
        view: Arc<ImageView>,
        layout: vk::ImageLayout,
        load_op: vk::AttachmentLoadOp,
        store_op: vk::AttachmentStoreOp,
        clear_value: u32,
    ) -> Self {
        let depth_stencil = vk::ClearDepthStencilValue {
            stencil: clear_value,
            ..Default::default()
        };
        let clear = vk::ClearValue { depth_stencil };
        let target = AttachmentTarget {
            view,
            layout,
            load_op,
            store_op,
            clear,
        };
        Self { target }
    }

    pub fn view(&self) -> &Arc<ImageView> {
        &self.target.view
    }

    pub fn layout(&self) -> vk::ImageLayout {
        self.target.layout
    }

    pub fn load_op(&self) -> vk::AttachmentLoadOp {
        self.target.load_op
    }

    pub fn store_op(&self) -> vk::AttachmentStoreOp {
        self.target.store_op
    }

    pub fn clear_stencil(&self) -> u32 {
        unsafe { self.target.clear.depth_stencil.stencil }
    }
}

struct AttachmentTarget {
    view: Arc<ImageView>,
    layout: vk::ImageLayout,
    load_op: vk::AttachmentLoadOp,
    store_op: vk::AttachmentStoreOp,
    clear: vk::ClearValue,
}

impl AttachmentTarget {
    fn get_rendering_attachment_info(&self) -> vk::RenderingAttachmentInfo {
        use vulkanalia::prelude::v1_0::*;
        vk::RenderingAttachmentInfo::builder()
            .image_view(unsafe { *self.view.owned().raw() })
            .image_layout(self.layout)
            .load_op(self.load_op)
            .store_op(self.store_op)
            .clear_value(self.clear)
            .build()
    }
}

pub struct RenderingLayout {
    pub color_formats: Box<[vk::Format]>,
    pub depth_format: Option<vk::Format>,
    pub stencil_format: Option<vk::Format>,
    pub samples: SampleCount,
}

impl RenderingLayout {
    pub fn view_mask(&self) -> u32 {
        // multi-view rendering not supported
        0
    }

    fn validate_color_targets(&self, colors: &[ColorTarget]) -> Result<()> {
        if self.color_formats.len() != colors.len() {
            return Err(anyhow!("invalid number of color targets"));
        }
        for (i, color) in colors.iter().enumerate() {
            let msg = format!("color({})", i);
            self.validate_image_view(
                msg.as_str(),
                color.view(),
                self.color_formats[i],
                vk::ImageAspectFlags::COLOR,
            )?;
        }
        Ok(())
    }

    fn validate_depth_target(&self, depth: Option<&DepthTarget>) -> Result<()> {
        match (self.depth_format.as_ref(), depth) {
            (Some(layout), Some(depth)) => self.validate_image_view(
                "depth",
                depth.view(),
                *layout,
                vk::ImageAspectFlags::DEPTH,
            )?,
            (None, Some(_)) => {
                return Err(anyhow!("unexpected depth target"));
            }
            (Some(_), None) => {
                return Err(anyhow!("missing depth target"));
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_stencil_target(&self, stencil: Option<&StencilTarget>) -> Result<()> {
        match (self.stencil_format.as_ref(), stencil) {
            (Some(layout), Some(stencil)) => self.validate_image_view(
                "stencil",
                stencil.view(),
                *layout,
                vk::ImageAspectFlags::STENCIL,
            )?,
            (None, Some(_)) => {
                return Err(anyhow!("unexpected stencil target"));
            }
            (Some(_), None) => {
                return Err(anyhow!("missing stencil target"));
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_image_view(
        &self,
        kind: &str,
        view: &Arc<ImageView>,
        format: vk::Format,
        aspect: vk::ImageAspectFlags,
    ) -> Result<()> {
        if view.dimensions()? > 2 {
            return Err(anyhow!("invalid {} target shape", kind));
        }
        if view.subresource_range().layer_count != 1 {
            // multi-view rendering not supported
            return Err(anyhow!("invalid {} target layer count", kind));
        }
        if view.subresource_range().level_count != 1 {
            return Err(anyhow!("invalid {} target mip count", kind));
        }
        if !view.subresource_range().aspect_mask.contains(aspect) {
            return Err(anyhow!("{} target does not have {} aspect", kind, kind));
        }
        if view.format() != format {
            return Err(anyhow!("incompatible {} target format", kind));
        }
        if view.samples() != self.samples {
            return Err(anyhow!("invalid render target sample count"));
        }
        Ok(())
    }
}
