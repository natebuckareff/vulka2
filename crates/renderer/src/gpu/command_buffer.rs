use std::rc::Rc;

use anyhow::Result;

use crate::gpu::{
    CommandBufferHandle, CommandPoolId, FrameToken, LaneKey, LaneVec, QueueGroupId, RetireToken,
};

pub struct CommandBuffer {
    frame: FrameToken,
    retire: RetireToken<CommandPoolId>,
    lanes: LaneVec<BufferLane>,
    alive: Rc<()>,
}

impl CommandBuffer {
    pub(crate) fn new(
        frame: FrameToken,
        retire: RetireToken<CommandPoolId>,
        lanes: LaneVec<BufferLane>,
        alive: Rc<()>,
    ) -> Self {
        Self {
            frame,
            retire,
            lanes,
            alive,
        }
    }

    pub(crate) fn frame(&self) -> &FrameToken {
        &self.frame
    }

    pub(crate) fn lanes(&self) -> &LaneVec<BufferLane> {
        &self.lanes
    }

    pub(crate) fn take_lanes(self) -> LaneVec<BufferLane> {
        self.lanes
    }

    fn touch(&mut self, key: LaneKey) {
        let lane = self.lanes.get_mut(key);
        lane.is_dirty = true;
        self.retire.touch(key, &self.frame);
    }

    pub fn graphics(&mut self) -> Result<GraphicsScope<'_>> {
        // TODO: check for graphics support
        Ok(GraphicsScope::new(self))
    }

    pub fn compute(&mut self) -> Result<ComputeScope<'_>> {
        // TODO: check for support
        Ok(ComputeScope::new(self))
    }

    pub fn transfer(&mut self) -> Result<TransferScope<'_>> {
        // TODO: check for support
        Ok(TransferScope::new(self))
    }
}

pub(crate) struct BufferLane {
    cmdbuf: CommandBufferHandle,
    is_dirty: bool,
}

impl BufferLane {
    pub(crate) fn new(cmdbuf: CommandBufferHandle) -> Self {
        Self {
            cmdbuf,
            is_dirty: false,
        }
    }

    pub(crate) fn handle(&self) -> &CommandBufferHandle {
        &self.cmdbuf
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub(crate) fn take_handle(self) -> CommandBufferHandle {
        self.cmdbuf
    }
}

// TODO
struct GraphicsScope<'cb> {
    buffer: &'cb mut CommandBuffer,
}

// TODO
impl<'cb> GraphicsScope<'cb> {
    fn new(buffer: &'cb mut CommandBuffer) -> Self {
        Self { buffer }
    }

    pub fn render(&mut self) -> RenderingScope<'_> {
        RenderingScope::new(self.buffer)
    }
}

// TODO
struct RenderingScope<'cb> {
    buffer: &'cb mut CommandBuffer,
}

// TODO
impl<'cb> RenderingScope<'cb> {
    fn new(buffer: &'cb mut CommandBuffer) -> Self {
        Self { buffer }
    }
}

// TODO
struct ComputeScope<'cb> {
    buffer: &'cb mut CommandBuffer,
}

// TODO
impl<'cb> ComputeScope<'cb> {
    fn new(buffer: &'cb mut CommandBuffer) -> Self {
        Self { buffer }
    }
}

// TODO
struct TransferScope<'cb> {
    buffer: &'cb mut CommandBuffer,
}

// TODO
impl<'cb> TransferScope<'cb> {
    fn new(buffer: &'cb mut CommandBuffer) -> Self {
        Self { buffer }
    }
}

// fn test(cmdbuf: &mut CommandBuffer) -> Result<()> {
//     let mut gfx = cmdbuf.graphics()?;
//     let pass1 = gfx.render();
//     let pass2 = gfx.render();
//     Ok(())
// }
