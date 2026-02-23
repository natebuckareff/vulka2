use std::{collections::VecDeque, sync::Arc};

use anyhow::Result;
use vulkanalia::vk;

use crate::gpu_v2::{
    BufferLane, Device, GpuFuture, LaneVec, LaneVecBuilder, LivenessGuard, LivenessToken,
    QueueFamilyId, QueueGroupId, QueueGroupInfo, QueueId, QueueRoleFlags,
};

#[derive(Clone, Copy)]
pub(crate) struct QueueLane {
    pub(crate) id: QueueId,
    pub(crate) roles: QueueRoleFlags,
}

pub(crate) struct PoolLane {
    device: Arc<Device>,
    queue: QueueLane,
    future: GpuFuture,
    pool: vk::CommandPool,
    waiting: VecDeque<vk::CommandBuffer>,
    active: Vec<vk::CommandBuffer>,
}

impl PoolLane {
    pub(crate) fn queue(&self) -> QueueLane {
        self.queue
    }

    pub(crate) fn future(&self) -> &GpuFuture {
        &self.future
    }
}

impl Drop for PoolLane {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.device
                .vk_device()
                .destroy_command_pool(self.pool, None);
        }
    }
}

pub struct CommandPool {
    device: Arc<Device>,
    allocator_id: usize,
    lanes: LaneVec<PoolLane>,
    liveness: LivenessToken,
    guard: LivenessGuard,
}

impl CommandPool {
    pub(crate) fn new(
        device: Arc<Device>,
        allocator_id: usize,
        queue_info: QueueGroupInfo,
        guard: LivenessGuard,
    ) -> Result<Self> {
        let mut lanes = LaneVecBuilder::with_lanes(&queue_info.bindings);

        for binding in queue_info.bindings.iter() {
            let queue = QueueLane {
                id: binding.id,
                roles: binding.roles,
            };
            let lane = PoolLane {
                device: device.clone(),
                queue,
                future: GpuFuture::new(),
                pool: create_command_pool(&device, binding.id.family)?,
                waiting: VecDeque::new(),
                active: Vec::new(),
            };
            lanes.push(lane);
        }

        Ok(Self {
            device,
            allocator_id,
            lanes: lanes.build(),
            liveness: LivenessToken::new(),
            guard,
        })
    }

    pub(crate) fn allocator_id(&self) -> usize {
        self.allocator_id
    }

    pub fn queue_group_id(&self) -> QueueGroupId {
        self.lanes.queue_group_id()
    }

    pub(crate) fn lanes(&self) -> &LaneVec<PoolLane> {
        &self.lanes
    }

    pub(crate) fn lanes_mut(&mut self) -> &mut LaneVec<PoolLane> {
        &mut self.lanes
    }

    pub(crate) fn liveness(&self) -> &LivenessToken {
        &self.liveness
    }

    pub(crate) fn allocate(&mut self) -> Result<LaneVec<BufferLane>> {
        let mut buffer_lanes = LaneVecBuilder::with_lanes(&self.lanes);
        for lane in self.lanes.iter_mut() {
            let cmdbuf = match lane.waiting.pop_front() {
                Some(buffer) => buffer,
                None => allocate_command_buffer(&self.device, lane.pool)?,
            };
            lane.active.push(cmdbuf);
            let buffer_lane = BufferLane::new(lane.queue, cmdbuf);
            buffer_lanes.push(buffer_lane);
        }
        Ok(buffer_lanes.build())
    }

    pub(crate) fn reset(&mut self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;
        for lane in self.lanes.iter_mut() {
            unsafe {
                self.device
                    .vk_device()
                    .reset_command_pool(lane.pool, vk::CommandPoolResetFlags::empty())
            }?;
            lane.future.reset()?;
            lane.waiting.extend(lane.active.drain(..));
        }
        Ok(())
    }
}

fn create_command_pool(device: &Device, family: QueueFamilyId) -> Result<vk::CommandPool> {
    use vulkanalia::prelude::v1_0::*;

    let create_info = vk::CommandPoolCreateInfo::builder()
        .queue_family_index(family.into())
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

    Ok(unsafe { device.vk_device().create_command_pool(&create_info, None) }?)
}

fn allocate_command_buffer(device: &Device, pool: vk::CommandPool) -> Result<vk::CommandBuffer> {
    use vulkanalia::prelude::v1_0::*;

    let alloc_info = vk::CommandBufferAllocateInfo::builder()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);

    let cmdbufs = unsafe { device.vk_device().allocate_command_buffers(&alloc_info) }?;

    Ok(cmdbufs[0])
}
