use std::sync::Arc;

use anyhow::{Result, anyhow};
use bitflags::bitflags;
use vulkanalia::vk;

use crate::gpu_v2::{
    DeviceBuilder, LaneVec, LaneVecBuilder, Queue, QueuePacket, Submission, SubmissionLane,
    VulkanHandle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueFamilyId(u32);

impl From<u32> for QueueFamilyId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<usize> for QueueFamilyId {
    fn from(value: usize) -> Self {
        Self(value as u32)
    }
}

impl Into<u32> for QueueFamilyId {
    fn into(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueueId {
    pub family: QueueFamilyId,
    pub index: u32,
}

// TODO: justify u8 probably with lane_index hardening
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueGroupId(u8);

impl From<u8> for QueueGroupId {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl TryFrom<u32> for QueueGroupId {
    type Error = anyhow::Error;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(Self(u8::try_from(value)?))
    }
}

impl Into<u32> for QueueGroupId {
    fn into(self) -> u32 {
        self.0 as u32
    }
}

impl Into<u8> for QueueGroupId {
    fn into(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFamily {
    pub id: QueueFamilyId,
    pub roles: QueueRoleFlags,
    pub count: u32,
}

bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct QueueRoleFlags: u8 {
        const GRAPHICS = 0b0001;
        const COMPUTE  = 0b0010;
        const TRANSFER = 0b0100;
        const PRESENT  = 0b1000;
    }
}

impl From<vk::QueueFlags> for QueueRoleFlags {
    fn from(flags: vk::QueueFlags) -> Self {
        let mut roles = Self::empty();
        if flags.contains(vk::QueueFlags::GRAPHICS) {
            roles |= Self::GRAPHICS;
        }
        if flags.contains(vk::QueueFlags::COMPUTE) {
            roles |= Self::COMPUTE;
        }
        if flags.contains(vk::QueueFlags::TRANSFER) {
            roles |= Self::TRANSFER;
        }
        roles
    }
}

pub struct QueueGroupBuilder<'a> {
    builder: &'a mut DeviceBuilder,
    id: QueueGroupId,
    roles: QueueRoleFlags,
}

impl<'a> QueueGroupBuilder<'a> {
    pub(crate) fn new(builder: &'a mut DeviceBuilder, id: QueueGroupId) -> Self {
        Self {
            builder,
            id,
            roles: QueueRoleFlags::empty(),
        }
    }

    pub fn graphics(mut self) -> Self {
        self.roles |= QueueRoleFlags::GRAPHICS;
        self
    }

    pub fn present(mut self) -> Self {
        self.roles |= QueueRoleFlags::PRESENT;
        self
    }

    pub fn compute(mut self) -> Self {
        self.roles |= QueueRoleFlags::COMPUTE;
        self
    }

    pub fn transfer(mut self) -> Self {
        self.roles |= QueueRoleFlags::TRANSFER;
        self
    }

    pub fn build(self) -> Result<QueueGroupId> {
        let Some(group_id) = self.builder.allocate_group(self.id, self.roles)? else {
            return Err(anyhow!("no queue group found"));
        };
        Ok(group_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueAllocation {
    pub queue_id: QueueId,
    pub roles: QueueRoleFlags,
}

pub struct QueueGroup {
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    id: QueueGroupId,
    roles: QueueRoleFlags,
    queues: LaneVec<Queue>,
    scratch: LaneVec<Vec<QueuePacket>>,
}

impl QueueGroup {
    pub(crate) fn new(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        id: QueueGroupId,
        queues: LaneVec<Queue>,
    ) -> Self {
        let roles = queues.iter().map(|q| q.roles()).collect();
        let mut scratch = LaneVecBuilder::with_lanes(&queues);
        for _ in 0..queues.len() {
            scratch.push(Vec::new());
        }
        Self {
            device,
            id,
            roles,
            queues,
            scratch: scratch.build(),
        }
    }

    pub fn id(&self) -> QueueGroupId {
        self.id
    }

    pub fn roles(&self) -> QueueRoleFlags {
        self.roles
    }

    pub fn queues(&self) -> &LaneVec<Queue> {
        &self.queues
    }

    pub fn submit(&mut self, submission: Submission) -> Result<()> {
        let Submission {
            lanes,
            signal,
            packets,
            mut usage,
        } = submission;

        usage.disarm();

        let result = self.submit_packets(lanes, packets);

        // notify the command allocator that all GpuFutures were set, making
        // sure to *always* signal, even if submit_packets() failed
        if let Err(e) = signal.notify() {
            if let Err(e2) = result {
                return Err(e2.context(e));
            } else {
                return Err(e);
            }
        }

        result
    }

    fn submit_packets(
        &mut self,
        lanes: LaneVec<SubmissionLane>,
        packets: Vec<QueuePacket>,
    ) -> Result<()> {
        if self.id != lanes.queue_group_id() {
            return Err(anyhow!("mismatched queue groups"));
        }

        if cfg!(debug_assertions) {
            debug_assert!(
                self.scratch.iter().all(|buf| buf.is_empty()),
                "scratch buffers always reset after use"
            );
        }

        for buf in self.scratch.iter_mut() {
            buf.clear();
        }

        // split packets by queue lane
        for packet in packets.into_iter() {
            let buf = &mut self.scratch.get_mut(packet.index);
            buf.push(packet);
        }

        // submit the packets
        for (index, lane) in lanes.into_entries() {
            let Some(future) = lane.future else {
                continue;
            };

            let buf = &mut self.scratch.get_mut(index);

            // TODO: this guarantee feels a bit sketchy, can we harden it with
            // better types?
            debug_assert!(!buf.is_empty());

            let queue = &mut self.queues.get_mut(index);
            queue.submit(future, buf)?;
        }

        Ok(())
    }
}
