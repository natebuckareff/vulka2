use std::{cell::RefCell, rc::Rc, sync::Arc};

use anyhow::{Result, anyhow};
use vulkanalia::vk::{self, DeviceV1_0, DeviceV1_3};

use crate::gpu::{
    CommandBufferHandle, CommandPoolId, Device, FrameToken, GraphicsPipeline, LaneKey, LaneVec,
    ParameterToken, PipelineLayout, PushConstantData, QueueRoleFlags, RenderTargets, RetireToken,
    VulkanResource,
};

pub struct CommandBuffer {
    device: Arc<Device>,
    frame: FrameToken,
    retire: RetireToken<CommandPoolId>,
    lanes: LaneVec<CommandLane>,
    // TODO: track bound pipeline
    layout: RefCell<Option<Arc<PipelineLayout>>>,
    alive: Rc<()>, // TODO: what is this for?
}

impl CommandBuffer {
    pub(crate) fn new(
        device: Arc<Device>,
        frame: FrameToken,
        retire: RetireToken<CommandPoolId>,
        lanes: LaneVec<CommandLane>,
        alive: Rc<()>,
    ) -> Self {
        Self {
            device,
            frame,
            retire,
            lanes,
            layout: RefCell::new(None),
            alive,
        }
    }

    pub(crate) fn frame(&self) -> &FrameToken {
        &self.frame
    }

    pub(crate) fn lanes(&self) -> &LaneVec<CommandLane> {
        &self.lanes
    }

    pub(crate) fn take_lanes(self) -> LaneVec<CommandLane> {
        self.lanes
    }

    pub(crate) fn touch(&mut self, key: LaneKey) {
        let lane = self.lanes.get_mut(key);
        lane.is_dirty = true;
        self.retire.touch(key, &self.frame);
    }

    // TODO: can we do better than this? O(1)? is it worth for small vecs?
    unsafe fn lane(&mut self, roles: QueueRoleFlags) -> Result<(LaneKey, vk::CommandBuffer)> {
        for (key, lane) in self.lanes.iter_entries_mut() {
            if lane.roles.contains(roles) {
                return Ok((key, unsafe { lane.cmdbuf.raw() }));
            }
        }
        Err(anyhow!("lane does not exist for given role"))
    }

    // TODO: need to really rethink Arc for handles since this is going to cause
    // contention when trying to record command buffers across different threads
    // with shared PipelineLayouts. Need a way to lower handles to a more
    // lightweight per-thread version
    pub fn bind_layout(&mut self, layout: Arc<PipelineLayout>) {
        *self.layout.borrow_mut() = Some(layout);
    }

    pub fn graphics(&mut self) -> GraphicsEncoder<'_> {
        GraphicsEncoder::new(self)
    }

    pub fn compute(&mut self) -> ComputeEncoder<'_> {
        ComputeEncoder::new(self)
    }

    // ~

    fn validate_pipeline_layout(&self, layout: &PipelineLayout) -> Result<()> {
        match &*self.layout.borrow() {
            Some(bound_layout) => {
                if bound_layout.as_ref() != layout {
                    return Err(anyhow!("incompatible pipeline layout"));
                }
            }
            None => return Err(anyhow!("no pipeline layout bound")),
        };
        Ok(())
    }

    fn begin_dynamic_rendering(&mut self, info: &vk::RenderingInfo) -> Result<()> {
        let (key, cmdbuf) = unsafe { self.lane(QueueRoleFlags::GRAPHICS)? };
        self.touch(key);
        unsafe {
            let device = self.device.handle().raw();
            device.cmd_begin_rendering(cmdbuf, info);
        }
        Ok(())
    }

    fn end_dynamic_rendering(&mut self) -> Result<()> {
        let (key, cmdbuf) = unsafe { self.lane(QueueRoleFlags::GRAPHICS)? };
        self.touch(key);
        unsafe {
            let device = self.device.handle().raw();
            device.cmd_end_rendering(cmdbuf);
        }
        Ok(())
    }

    fn bind_graphics_pipeline(&mut self, pipeline: &GraphicsPipeline) -> Result<()> {
        self.validate_pipeline_layout(pipeline.layout())?;
        let (key, cmdbuf) = unsafe { self.lane(QueueRoleFlags::GRAPHICS)? };
        self.touch(key);
        unsafe {
            let device = self.device.handle().raw();
            let bind_point = vk::PipelineBindPoint::GRAPHICS;
            device.cmd_bind_pipeline(cmdbuf, bind_point, *pipeline.owned().raw());
        };
        Ok(())
    }

    fn bind_descriptor_set(
        &mut self,
        role: QueueRoleFlags,
        token: &mut ParameterToken,
    ) -> Result<()> {
        let (key, cmdbuf) = unsafe { self.lane(role)? };
        self.touch(key);

        // TODO: possible to touch the errorl; fixable if we pre-validate
        token.touch(key, &self.frame);

        let set = token.set_token().set();

        // TODO: pointer-chasing and validation in recording hot-path; can we
        // pre-validate somehow?
        let Some(set_number) = set.set_layout().layout().parameter_block_layout()?.set else {
            return Err(anyhow!("parameter block does not contain a descriptor set"));
        };

        // TODO: validate negative
        let set_number = set_number as u32;

        // TODO: pre-validate somehow?
        if let Some(bound_layout) = &*self.layout.borrow() {
            let expected = bound_layout.set(set_number as usize)?;
            if expected != set.set_layout().as_ref() {
                return Err(anyhow!("incompatible descriptor set layout"));
            }
        }

        let layout = self
            .layout
            .borrow()
            .as_ref()
            .map(|layout| unsafe { *layout.owned().raw() });

        let Some(layout) = layout else {
            return Err(anyhow!("no pipeline layout bound"));
        };

        let pipeline_bind_point = match role {
            QueueRoleFlags::GRAPHICS => vk::PipelineBindPoint::GRAPHICS,
            QueueRoleFlags::COMPUTE => vk::PipelineBindPoint::COMPUTE,
            _ => todo!(),
        };

        unsafe {
            let device = self.device.handle().raw();
            device.cmd_bind_descriptor_sets(
                cmdbuf,
                pipeline_bind_point,
                layout,
                set_number,
                &[set.raw()],
                &[], // not using dynamic offsets for now
            );
        };
        Ok(())
    }
}

pub(crate) struct CommandLane {
    // queue: QueueId,
    roles: QueueRoleFlags,
    cmdbuf: CommandBufferHandle, // TODO: this feels clunky for some reason, why?
    is_dirty: bool,
}

impl CommandLane {
    pub(crate) fn new(roles: QueueRoleFlags, cmdbuf: CommandBufferHandle) -> Self {
        Self {
            // queue,
            roles,
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

pub struct GraphicsEncoder<'c> {
    cmdbuf: &'c mut CommandBuffer,
}

impl<'c> GraphicsEncoder<'c> {
    fn new(cmdbuf: &'c mut CommandBuffer) -> Self {
        Self { cmdbuf }
    }

    pub fn bind_layout(&mut self, layout: Arc<PipelineLayout>) {
        self.cmdbuf.bind_layout(layout)
    }

    pub fn bind_pipeline(&mut self, pipeline: &GraphicsPipeline) -> Result<()> {
        self.cmdbuf.bind_graphics_pipeline(pipeline)
    }

    pub fn bind_parameters(&mut self, token: &mut ParameterToken) -> Result<()> {
        let role = QueueRoleFlags::GRAPHICS;
        self.cmdbuf.bind_descriptor_set(role, token)
    }

    pub fn push_constants(&mut self, data: &PushConstantData) -> Result<()> {
        todo!()
    }

    pub fn render<'t>(&mut self, targets: &'t RenderTargets) -> Result<Rendering<'c, '_, 't>> {
        Rendering::new(self, targets)
    }
}

pub struct Rendering<'c, 'g, 't> {
    graphics: &'g mut GraphicsEncoder<'c>,
    targets: &'t RenderTargets,
}

impl<'c, 'g, 't> Rendering<'c, 'g, 't> {
    fn new(graphics: &'g mut GraphicsEncoder<'c>, targets: &'t RenderTargets) -> Result<Self> {
        // TODO: if there is already a bound pipeline, check that it's rendering
        // layout is compatible
        // if let Some(bound_pipeline) = &*graphics.cmdbuf.pipeline.borrow() {
        //     return Err(anyhow!("incompatible rendering layouts"));
        // }
        let info = targets.rendering_info();
        graphics.cmdbuf.begin_dynamic_rendering(info)?;
        Ok(Self { graphics, targets })
    }

    pub fn bind_layout(&mut self, layout: Arc<PipelineLayout>) {
        self.graphics.bind_layout(layout)
    }

    pub fn bind_pipeline(&mut self, pipeline: &GraphicsPipeline) -> Result<()> {
        // TODO: feels expensive to be doing while recording
        if pipeline.rendering() != self.targets.layout() {
            return Err(anyhow!("incompatible rendering layouts"));
        }
        self.graphics.bind_pipeline(pipeline)
    }

    pub fn bind_parameters(&mut self, token: &mut ParameterToken) -> Result<()> {
        self.graphics.bind_parameters(token)
    }

    pub fn push_constants(&mut self, data: &PushConstantData) -> Result<()> {
        self.graphics.push_constants(data)
    }

    pub fn draw(&mut self) -> Result<()> {
        todo!()
    }

    pub fn end(self) -> Result<()> {
        self.graphics.cmdbuf.end_dynamic_rendering()
    }
}

pub struct ComputeEncoder<'a> {
    cmdbuf: &'a mut CommandBuffer,
}

impl<'a> ComputeEncoder<'a> {
    fn new(cmdbuf: &'a mut CommandBuffer) -> Self {
        Self { cmdbuf }
    }

    pub fn dispatch(&mut self) -> Result<()> {
        todo!()
    }
}

trait TransferEncoder {
    fn copy(&mut self) -> Result<()>;
}

impl TransferEncoder for CommandBuffer {
    fn copy(&mut self) -> Result<()> {
        todo!()
    }
}

impl<'a> TransferEncoder for GraphicsEncoder<'a> {
    fn copy(&mut self) -> Result<()> {
        todo!()
    }
}

impl<'a> TransferEncoder for ComputeEncoder<'a> {
    fn copy(&mut self) -> Result<()> {
        todo!()
    }
}

fn test(
    cmdbuf: &mut CommandBuffer,
    token: &mut ParameterToken,
    pipeline: &GraphicsPipeline,
    data: &PushConstantData,
    targets: &RenderTargets,
) -> Result<()> {
    cmdbuf.bind_layout(pipeline.layout().clone());

    let mut graphics = cmdbuf.graphics();
    graphics.bind_parameters(token)?;
    graphics.push_constants(data)?;
    graphics.copy()?;

    let mut render = graphics.render(targets)?;
    render.bind_pipeline(pipeline)?;
    render.draw()?;
    render.end()?;

    cmdbuf.copy()?;

    let mut compute = cmdbuf.compute();
    compute.copy()?;
    compute.dispatch()?;

    Ok(())
}
