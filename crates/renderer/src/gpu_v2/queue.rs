use std::sync::Arc;

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu_v2::{
    GpuFutureWriter, LaneIndex, QueueId, QueuePacket, QueueRoleFlags, VulkanHandle,
};

pub struct Queue {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    id: QueueId,
    lane: LaneIndex,
    roles: QueueRoleFlags,
    queue: vk::Queue,
    semaphore: VulkanHandle<vk::Semaphore>,
    submission_counter: u64,
}

impl Queue {
    pub(crate) fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        id: QueueId,
        lane: LaneIndex,
        roles: QueueRoleFlags,
        queue: vk::Queue,
        semaphore: VulkanHandle<vk::Semaphore>,
    ) -> Result<Self> {
        Ok(Self {
            device,
            id,
            lane,
            roles,
            queue,
            semaphore,
            submission_counter: 0,
        })
    }

    pub fn id(&self) -> QueueId {
        self.id
    }

    pub(crate) fn lane(&self) -> LaneIndex {
        self.lane
    }

    pub fn roles(&self) -> QueueRoleFlags {
        self.roles
    }

    pub fn handle(&self) -> vk::Queue {
        self.queue
    }

    pub fn semaphore(&self) -> &VulkanHandle<vk::Semaphore> {
        &self.semaphore
    }

    pub fn submit(&mut self, future: GpuFutureWriter, packets: &[QueuePacket]) -> Result<()> {
        let submission_id = self.submit_packets(packets)?;
        future.set(submission_id)?;
        Ok(())
    }

    fn submit_packets(&mut self, packets: &[QueuePacket]) -> Result<u64> {
        use vulkanalia::prelude::v1_3::*;

        if packets.is_empty() {
            return Err(anyhow!("no packets to submit"));
        }

        let submission_id = self.submission_counter + 1;
        self.submission_counter += 1;

        let cmdbuf_infos = packets
            .iter()
            .map(|packet| {
                vk::CommandBufferSubmitInfo::builder()
                    .command_buffer(packet.cmdbuf)
                    .build()
            })
            .collect::<Vec<_>>();

        let signal_infos = [vk::SemaphoreSubmitInfo::builder()
            .semaphore(unsafe { *self.semaphore.raw() })
            .value(submission_id)
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .build()];

        let submit_infos = [vk::SubmitInfo2::builder()
            .command_buffer_infos(&cmdbuf_infos)
            .signal_semaphore_infos(&signal_infos)
            .build()];

        unsafe {
            self.device
                .raw()
                .queue_submit2(self.queue, &submit_infos, vk::Fence::null())
        }?;

        Ok(submission_id)
    }
}
