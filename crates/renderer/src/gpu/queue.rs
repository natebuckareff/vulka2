use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{
    CommandBufferHandle, FrameToken, LaneKey, QueueId, QueueRoleFlags, SettledLanes, VulkanHandle,
};

pub struct Queue {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    id: QueueId,
    key: LaneKey,
    roles: QueueRoleFlags,
    queue: vk::Queue,
    settled: Arc<SettledLanes>,
    current_frame: u64,
    waiting: BTreeMap<(u64, usize), (FrameToken, Vec<CommandBufferHandle>)>,
    waiting_next: usize,
    semaphore: VulkanHandle<vk::Semaphore>, // XXX: why not owned?
    submissions: u64,
    scratch: RefCell<Vec<vk::CommandBufferSubmitInfo>>,
}

impl Queue {
    pub(crate) fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        id: QueueId,
        key: LaneKey,
        roles: QueueRoleFlags,
        queue: vk::Queue,
        settled: Arc<SettledLanes>,
        semaphore: VulkanHandle<vk::Semaphore>,
    ) -> Result<Self> {
        Ok(Self {
            device,
            id,
            key,
            roles,
            queue,
            settled,
            current_frame: 0,
            waiting: BTreeMap::new(),
            waiting_next: 0,
            semaphore,
            submissions: 0,
            scratch: RefCell::new(Vec::new()),
        })
    }

    pub fn id(&self) -> QueueId {
        self.id
    }

    pub(crate) fn key(&self) -> LaneKey {
        self.key
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

    pub fn wait_idle(&self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;
        unsafe { self.device.raw().queue_wait_idle(self.queue) }?;
        Ok(())
    }

    pub fn submit(&mut self, frame: FrameToken, packet: Vec<CommandBufferHandle>) -> Result<()> {
        debug_assert!(!packet.is_empty());

        if frame.number() < self.current_frame {
            return Err(anyhow!("out-of-order frame submission"));
        }

        if frame.number() == self.current_frame {
            self.submit_packet(frame, &packet)?;
        } else {
            let key = (frame.number(), self.waiting_next);
            self.waiting.insert(key, (frame, packet));
            self.waiting_next += 1;
        }

        self.flush()?;

        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        loop {
            if self.settled.is_host_complete(self.key, self.current_frame) {
                self.current_frame += 1;
                while let Some(entry) = self.waiting.first_entry() {
                    let (frame_number, _) = entry.key();
                    if *frame_number != self.current_frame {
                        break;
                    }
                    let (frame, packet) = entry.remove();
                    self.submit_packet(frame, &packet)?;
                }
                continue;
            }
            break;
        }
        Ok(())
    }

    fn submit_packet(
        &mut self,
        frame: FrameToken,
        packet: &Vec<CommandBufferHandle>,
    ) -> Result<()> {
        use vulkanalia::prelude::v1_3::*;

        debug_assert!(!packet.is_empty());

        frame.consume(self.key);

        self.submissions += 1;

        let mut scratch = self.scratch.borrow_mut();
        scratch.clear();

        for handle in packet {
            scratch.push(
                vk::CommandBufferSubmitInfo::builder()
                    .command_buffer(unsafe { handle.raw() })
                    .build(),
            );
        }

        let cmdbuf_infos = scratch.as_slice();

        let signal_infos = [vk::SemaphoreSubmitInfo::builder()
            .semaphore(unsafe { *self.semaphore.raw() })
            .value(self.submissions)
            .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .build()];

        let submit_infos = [vk::SubmitInfo2::builder()
            .command_buffer_infos(&cmdbuf_infos)
            .signal_semaphore_infos(&signal_infos)
            .build()];

        unsafe {
            let device = self.device.raw();
            device.queue_submit2(self.queue, &submit_infos, vk::Fence::null())
        }?;

        Ok(())
    }
}
