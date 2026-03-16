use std::{rc::Rc, sync::Arc};

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{
    BufferLane, CommandBuffer, CommandPoolId, Device, FrameToken, LaneVec, LaneVecBuilder,
    OwnedCommandPool, QueueBinding, QueueFamilyId, QueueGroupInfo, RetireToken, VulkanResource,
};

pub struct CommandPool {
    retire: RetireToken<CommandPoolId>,
    state: CommandPoolState,
}

impl CommandPool {
    pub(crate) fn new(retire: RetireToken<CommandPoolId>, state: CommandPoolState) -> Self {
        Self { retire, state }
    }

    pub(crate) fn retire(&self) -> &RetireToken<CommandPoolId> {
        &self.retire
    }

    pub(crate) fn split(self) -> Result<(RetireToken<CommandPoolId>, CommandPoolState)> {
        if Rc::strong_count(&self.state.children) > 1 {
            // if a command pool is retired while command buffers allocated from
            // it have not yet been recored to, then the pool's RetireToken may
            // not account for all dependencies, and could be prematurely
            // re-acquired. fix is to make sure all child command buffers are
            // dropped before retirement
            return Err(anyhow!("command pool still has live command buffers"));
        }
        let retire = self.retire;
        let state = self.state;
        Ok((retire, state))
    }

    pub fn allocate(&mut self, frame: FrameToken) -> Result<CommandBuffer> {
        let mut cmdbuf_lanes = LaneVecBuilder::with_lanes(&self.state.lanes);
        for lane in self.state.lanes.iter_mut() {
            let cmdbuf = lane.allocate(&self.state.device)?;
            let cmdbuf_lane = BufferLane::new(cmdbuf);
            cmdbuf_lanes.push(cmdbuf_lane);
        }
        let retire = self.retire.clone();
        let alive = self.state.children.clone();
        let cmdbuf = CommandBuffer::new(frame, retire, cmdbuf_lanes.build(), alive);
        Ok(cmdbuf)
    }
}

pub(crate) struct CommandPoolState {
    device: Arc<Device>,
    lanes: LaneVec<PoolLane>,
    children: Rc<()>,
}

impl CommandPoolState {
    pub(crate) fn new(device: Arc<Device>, queue_info: &QueueGroupInfo) -> Result<Self> {
        let mut lanes = LaneVecBuilder::with_lanes(&queue_info.bindings);
        for binding in queue_info.bindings.iter() {
            let lane = PoolLane::new(&device, binding)?;
            lanes.push(lane);
        }
        Ok(CommandPoolState {
            device,
            lanes: lanes.build(),
            children: Rc::new(()),
        })
    }

    // SAFETY: unsafe to call if the pool is still in use
    pub(crate) unsafe fn reset(&mut self) -> Result<()> {
        for lane in self.lanes.iter_mut() {
            unsafe {
                lane.reset(&self.device)?;
            }
        }
        Ok(())
    }
}

struct PoolLane {
    pool: Arc<OwnedCommandPool>,
    cmdbufs: Vec<vk::CommandBuffer>,
    next_cmdbuf: usize,
    is_reset: bool,
}

impl PoolLane {
    fn new(device: &Device, binding: &QueueBinding) -> Result<Self> {
        let pool = create_command_pool(&device, binding.id.family)?;
        Ok(Self {
            pool: Arc::new(pool),
            cmdbufs: vec![],
            next_cmdbuf: 0,
            is_reset: true,
        })
    }

    // SAFETY: unsafe to call if the pool is still in use
    unsafe fn reset(&mut self, device: &Device) -> Result<()> {
        if self.is_reset {
            return Ok(());
        }
        reset_command_pool(device, &self.pool)?;
        self.next_cmdbuf = 0;
        self.is_reset = true;
        Ok(())
    }

    fn allocate(&mut self, device: &Device) -> Result<CommandBufferHandle> {
        let cmdbuf = if self.next_cmdbuf < self.cmdbufs.len() {
            let cmdbuf = self.cmdbufs[self.next_cmdbuf];
            cmdbuf
        } else {
            let cmdbuf = allocate_command_buffer(device, &self.pool)?;
            self.cmdbufs.push(cmdbuf);
            cmdbuf
        };
        self.next_cmdbuf += 1;
        self.is_reset = false;
        Ok(CommandBufferHandle {
            pool: self.pool.clone(),
            cmdbuf,
        })
    }
}

#[derive(Clone)]
pub(crate) struct CommandBufferHandle {
    pool: Arc<OwnedCommandPool>,
    cmdbuf: vk::CommandBuffer,
}

impl CommandBufferHandle {
    pub(crate) unsafe fn raw(&self) -> vk::CommandBuffer {
        self.cmdbuf
    }
}

fn allocate_command_buffer(device: &Device, pool: &OwnedCommandPool) -> Result<vk::CommandBuffer> {
    use vulkanalia::prelude::v1_0::*;

    let info = vk::CommandBufferAllocateInfo::builder()
        .command_pool(unsafe { *pool.raw() })
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);

    let cmdbufs = unsafe {
        let device = device.handle().raw();
        device.allocate_command_buffers(&info)
    }?;

    Ok(cmdbufs[0])
}

fn create_command_pool(device: &Device, family: QueueFamilyId) -> Result<OwnedCommandPool> {
    use vulkanalia::prelude::v1_0::*;

    let create_info = vk::CommandPoolCreateInfo::builder()
        .queue_family_index(family.into())
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

    let device = device.handle().clone();
    let pool = OwnedCommandPool::new(device, &create_info)?;
    Ok(pool)
}

fn reset_command_pool(device: &Device, pool: &OwnedCommandPool) -> Result<()> {
    use vulkanalia::prelude::v1_0::*;
    unsafe {
        let device = device.handle().raw();
        device.reset_command_pool(*pool.raw(), vk::CommandPoolResetFlags::empty())?;
    }
    Ok(())
}
