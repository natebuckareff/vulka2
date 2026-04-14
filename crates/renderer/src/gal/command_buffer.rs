use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::queue::Lane;
use crate::gal::render_targets::RenderTargets;

pub struct CommandBuffer<'pool> {
    frame: u32,
    lane: Lane,
    handle: vk::CommandBuffer,
    consumed: bool,
    marker: PhantomData<&'pool ()>,
}

impl<'pool> CommandBuffer<'pool> {
    pub(crate) fn new(frame: u32, lane: Lane, handle: vk::CommandBuffer) -> Self {
        Self {
            frame,
            lane,
            handle,
            consumed: false,
            marker: PhantomData,
        }
    }

    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn lane(&self) -> Lane {
        self.lane
    }

    pub fn graphics<'c>(&'c mut self) -> GraphicsEncoder<'pool, 'c> {
        GraphicsEncoder::new(self)
    }

    fn begin_dynamic_rendering(&mut self, info: &vk::RenderingInfo) -> Result<()> {
        todo!()
    }

    fn end_dynamic_rendering(&mut self) -> Result<()> {
        todo!()
    }

    pub(crate) fn into_packet(mut self) -> CommandPacket {
        self.consumed = true;
        CommandPacket::new(self.handle)
    }
}

pub struct CommandPacket {
    // TODO: will need more info for owership transfers
    handle: vk::CommandBuffer,
}

impl CommandPacket {
    pub(crate) fn new(handle: vk::CommandBuffer) -> Self {
        Self { handle }
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

    pub fn render<'t>(self, targets: &'t RenderTargets) -> Result<Rendering<'pool, 'c, 't>> {
        Rendering::new(self.cmdbuf, targets)
    }
}

pub struct Rendering<'p, 'c, 't> {
    cmdbuf: &'c mut CommandBuffer<'p>,
    targets: &'t RenderTargets,
    ended: bool,
}

impl<'p, 'c, 't> Rendering<'p, 'c, 't> {
    fn new(cmdbuf: &'c mut CommandBuffer<'p>, targets: &'t RenderTargets) -> Result<Self> {
        // TODO: if there is already a bound pipeline, check that it's rendering
        // layout is compatible
        // if let Some(bound_pipeline) = &*graphics.cmdbuf.pipeline.borrow() {
        //     return Err(anyhow!("incompatible rendering layouts"));
        // }
        let info = targets.rendering_info();
        cmdbuf.begin_dynamic_rendering(info)?;
        Ok(Self {
            cmdbuf,
            targets,
            ended: false,
        })
    }

    // TODO: rendering commands

    pub fn end(mut self) -> Result<()> {
        self.cmdbuf.end_dynamic_rendering()?;
        self.ended = true;
        Ok(())
    }
}

impl<'p, 'c, 't> Drop for Rendering<'p, 'c, 't> {
    fn drop(&mut self) {
        assert!(self.ended, "render scope not ended");
    }
}
