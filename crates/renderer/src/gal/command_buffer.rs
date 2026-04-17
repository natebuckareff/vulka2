use std::marker::PhantomData;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::bound_render_targets::BoundColorTarget;
use crate::gal::bound_render_targets::BoundDepthTarget;
use crate::gal::bound_render_targets::BoundRenderTargets;
use crate::gal::bound_render_targets::BoundStencilTarget;
use crate::gal::device::DeviceResource;
use crate::gal::image_token::{ImageAccess, ImageState, ImageToken};
use crate::gal::queue::Lane;
use crate::gal::swapchain_v2::SwapchainToken;
use crate::gal::usage_token::UsageToken;

pub struct CommandBuffer<'pool> {
    device: &'pool DeviceResource,
    frame: u32,
    lane: Lane,
    handle: vk::CommandBuffer,
    consumed: bool,
    marker: PhantomData<&'pool ()>,
}

impl<'pool> CommandBuffer<'pool> {
    pub(crate) fn new(
        usage: &mut UsageToken,
        device: &'pool DeviceResource,
        frame: u32,
        lane: Lane,
        handle: vk::CommandBuffer,
    ) -> Result<Self> {
        let mut cmdbuf = Self {
            device,
            frame,
            lane,
            handle,
            consumed: false,
            marker: PhantomData,
        };
        cmdbuf.begin()?;
        usage.touch(frame, lane);
        Ok(cmdbuf)
    }

    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn lane(&self) -> Lane {
        self.lane
    }

    pub fn graphics<'c>(&'c mut self) -> GraphicsEncoder<'pool, 'c> {
        // TODO: validate that the queue family supports graphics (and later
        // compute, etc)
        GraphicsEncoder::new(self)
    }

    pub(crate) fn into_packet(mut self) -> Result<CommandPacket> {
        self.end()?;
        self.consumed = true;
        Ok(CommandPacket::new(self.handle))
    }

    fn begin(&mut self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;
        let info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .handle()
                .begin_command_buffer(self.handle, &info)?;
        }
        Ok(())
    }

    fn end(&mut self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.device.handle().end_command_buffer(self.handle)?;
        }
        Ok(())
    }

    fn begin_dynamic_rendering(&mut self, info: &vk::RenderingInfo) -> Result<()> {
        use vulkanalia::prelude::v1_3::*;
        unsafe {
            self.device.handle().cmd_begin_rendering(self.handle, info);
        }
        Ok(())
    }

    fn end_dynamic_rendering(&mut self) -> Result<()> {
        use vulkanalia::prelude::v1_3::*;
        unsafe {
            self.device.handle().cmd_end_rendering(self.handle);
        }
        Ok(())
    }

    fn transition_image(&mut self, token: &mut ImageToken, next: ImageState) -> Result<()> {
        use vulkanalia::prelude::v1_3::*;

        let old = token.state();
        if old == next {
            return Ok(());
        }

        let range = token.subresource().range();
        let (src_stage, src_access) = image_sync(old.access());
        let (dst_stage, dst_access) = image_sync(next.access());

        let mut src_family = vk::QUEUE_FAMILY_IGNORED;
        let mut dst_family = vk::QUEUE_FAMILY_IGNORED;

        if old.owner().is_some() && next.owner().is_some() {
            src_family = old
                .owner()
                .map_or(vk::QUEUE_FAMILY_IGNORED, |owner| u32::from(owner.family()));

            dst_family = next
                .owner()
                .map_or(vk::QUEUE_FAMILY_IGNORED, |owner| u32::from(owner.family()));
        }

        let barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
            .dst_access_mask(dst_access)
            .old_layout(old.layout())
            .new_layout(next.layout())
            .src_queue_family_index(src_family)
            .dst_queue_family_index(dst_family)
            .image(unsafe { token.subresource().image().storage().handle() })
            .subresource_range(range);

        let dependency = vk::DependencyInfo::builder()
            .dependency_flags(vk::DependencyFlags::empty())
            .image_memory_barriers(std::slice::from_ref(&barrier));

        unsafe {
            self.device
                .handle()
                .cmd_pipeline_barrier2(self.handle, &dependency);
        }

        token.set_state(next);
        Ok(())
    }
}

// TODO: maybe will make this an enum
pub struct CommandPacket {
    // TODO: will need more info for owership transfers
    handle: vk::CommandBuffer,
}

impl CommandPacket {
    pub(crate) fn new(handle: vk::CommandBuffer) -> Self {
        Self { handle }
    }

    pub(crate) fn handle(&self) -> vk::CommandBuffer {
        self.handle
    }
}

impl<'pool> Drop for CommandBuffer<'pool> {
    fn drop(&mut self) {
        assert!(self.consumed, "command buffer not consumed by submission");
    }
}

pub struct GraphicsEncoder<'pool, 'c> {
    cmdbuf: &'c mut CommandBuffer<'pool>,
}

impl<'pool, 'c> GraphicsEncoder<'pool, 'c> {
    fn new(cmdbuf: &'c mut CommandBuffer<'pool>) -> Self {
        Self { cmdbuf }
    }

    // TOOD: graphics commands

    pub fn render(self, targets: &mut BoundRenderTargets<'_>) -> Result<Rendering<'pool, 'c>> {
        Rendering::new(self.cmdbuf, targets)
    }
}

pub struct Rendering<'p, 'c> {
    cmdbuf: &'c mut CommandBuffer<'p>,
    ended: bool,
}

impl<'p, 'c> Rendering<'p, 'c> {
    fn new(
        cmdbuf: &'c mut CommandBuffer<'p>,
        targets: &mut BoundRenderTargets<'_>,
    ) -> Result<Self> {
        // TODO: if there is already a bound pipeline, check that it's rendering
        // layout is compatible
        // if let Some(bound_pipeline) = &*graphics.cmdbuf.pipeline.borrow() {
        //     return Err(anyhow!("incompatible rendering layouts"));
        // }
        transition_render_targets(cmdbuf, targets)?;
        let info = targets.targets().rendering_info();
        cmdbuf.begin_dynamic_rendering(info)?;
        Ok(Self {
            cmdbuf,
            ended: false,
        })
    }

    pub fn end(mut self) -> Result<()> {
        self.cmdbuf.end_dynamic_rendering()?;
        self.ended = true;
        Ok(())
    }

    pub fn present(mut self, token: &mut SwapchainToken) -> Result<()> {
        // TODO: error consistency
        self.cmdbuf.end_dynamic_rendering()?;
        self.ended = true;
        let (_, token) = token.record()?;
        let next: ImageState = ImageState::new(
            Some(self.cmdbuf.lane),
            vk::ImageLayout::PRESENT_SRC_KHR,
            ImageAccess::Present,
        );
        self.cmdbuf.transition_image(token, next)?;
        token.touch(self.cmdbuf.frame, self.cmdbuf.lane);
        Ok(())
    }
}

impl<'p, 'c> Drop for Rendering<'p, 'c> {
    fn drop(&mut self) {
        assert!(self.ended, "render scope not ended");
    }
}

fn transition_render_targets<'pool>(
    cmdbuf: &mut CommandBuffer<'pool>,
    targets: &mut BoundRenderTargets<'_>,
) -> Result<()> {
    for color in targets.colors_mut() {
        transition_color_target(cmdbuf, color)?;
    }

    if let Some(depth) = targets.depth_mut() {
        transition_depth_target(cmdbuf, depth)?;
    }

    if let Some(stencil) = targets.stencil_mut() {
        transition_stencil_target(cmdbuf, stencil)?;
    }

    Ok(())
}

// TODO: move all this to command_buffer_vk.rs

fn transition_color_target<'pool>(
    cmdbuf: &mut CommandBuffer<'pool>,
    target: &mut BoundColorTarget<'_>,
) -> Result<()> {
    let next = ImageState::new(
        Some(cmdbuf.lane()),
        target.target().layout(),
        ImageAccess::ColorAttachmentWrite,
    );
    let token = target.token_mut();
    cmdbuf.transition_image(token, next)?;
    token.touch(cmdbuf.frame(), cmdbuf.lane());
    Ok(())
}

fn transition_depth_target<'pool>(
    cmdbuf: &mut CommandBuffer<'pool>,
    target: &mut BoundDepthTarget<'_>,
) -> Result<()> {
    let next = ImageState::new(
        Some(cmdbuf.lane()),
        target.target().layout(),
        ImageAccess::DepthStencilAttachmentWrite,
    );
    let token = target.token_mut();
    cmdbuf.transition_image(token, next)?;
    token.touch(cmdbuf.frame(), cmdbuf.lane());
    Ok(())
}

fn transition_stencil_target<'pool>(
    cmdbuf: &mut CommandBuffer<'pool>,
    target: &mut BoundStencilTarget<'_>,
) -> Result<()> {
    let next = ImageState::new(
        Some(cmdbuf.lane()),
        target.target().layout(),
        ImageAccess::DepthStencilAttachmentWrite,
    );
    let token = target.token_mut();
    cmdbuf.transition_image(token, next)?;
    token.touch(cmdbuf.frame(), cmdbuf.lane());
    Ok(())
}

// TODO: ai wrote this, don't fully understand
fn image_sync(access: ImageAccess) -> (vk::PipelineStageFlags2, vk::AccessFlags2) {
    match access {
        ImageAccess::Uninitialized => (vk::PipelineStageFlags2::NONE, vk::AccessFlags2::NONE),
        ImageAccess::Present => (vk::PipelineStageFlags2::NONE, vk::AccessFlags2::NONE),
        ImageAccess::TransferRead => (
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_READ,
        ),
        ImageAccess::TransferWrite => (
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
        ),
        ImageAccess::SampledRead => (
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::AccessFlags2::SHADER_READ,
        ),
        ImageAccess::StorageRead => (
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::AccessFlags2::SHADER_STORAGE_READ,
        ),
        ImageAccess::StorageWrite => (
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::AccessFlags2::SHADER_STORAGE_WRITE,
        ),
        ImageAccess::ColorAttachmentWrite => (
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
        ImageAccess::DepthStencilAttachmentRead => (
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
        ),
        ImageAccess::DepthStencilAttachmentWrite => (
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),
    }
}
