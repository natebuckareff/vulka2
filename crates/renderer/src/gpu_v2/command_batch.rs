use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu_v2::{
    CommandBuffer, CommandPool, GpuFutureWriter, LaneIndex, LaneVec, SubmitSignal, UsageToken,
};

pub(crate) struct QueuePacket {
    pub(crate) index: LaneIndex,
    pub(crate) cmdbuf: vk::CommandBuffer, // TODO: VulkanHandle
}

pub struct CommandBatch {
    signal: SubmitSignal,
    pool: CommandPool,
    buffers: Vec<CommandBuffer>,
    usage: UsageToken,
}

impl CommandBatch {
    pub(crate) fn new(signal: SubmitSignal, pool: CommandPool) -> Self {
        Self {
            signal,
            pool,
            buffers: Vec::new(),
            usage: UsageToken::new(),
        }
    }

    pub fn allocate(&mut self) -> Result<CommandBuffer> {
        CommandBuffer::new(self.pool.allocate()?, self.pool.liveness().guard())
    }

    pub fn add(&mut self, mut buffer: CommandBuffer) -> Result<()> {
        if self.pool.queue_group_id() != buffer.queue_group_id() {
            buffer.disarm();
            return Err(anyhow!("mismatched queue groups"));
        }
        self.buffers.push(buffer);
        Ok(())
    }

    pub fn finish(mut self) -> Result<(Submission, CommandPool)> {
        self.usage.disarm();
        for buffer in self.buffers.iter_mut() {
            buffer.disarm();
        }

        let pool_lanes = self.pool.lanes();
        let mut sub_lanes = LaneVec::filled(pool_lanes, || SubmissionLane::default());
        let mut packets = vec![];

        for buffer in self.buffers.into_iter() {
            for (index, buf_lane) in buffer.lanes().iter_entries() {
                if buf_lane.dirty {
                    let sub_lane = sub_lanes.get_mut(index);
                    if sub_lane.future.is_none() {
                        // TODO: there is a lifecycle hole here if there is an
                        // error on send, owned pool is dropped
                        let value = pool_lanes.get(index).future().send()?;
                        sub_lane.future = Some(value);
                    }
                    let cmdbuf = buf_lane.cmdbuf;
                    packets.push(QueuePacket { index, cmdbuf });
                }
            }
        }

        let pool = self.pool;
        let submission = Submission::new(sub_lanes, self.signal, packets);
        Ok((submission, pool))
    }
}

#[derive(Default)]
pub(crate) struct SubmissionLane {
    pub(crate) future: Option<GpuFutureWriter>,
}

pub struct Submission {
    pub(crate) lanes: LaneVec<SubmissionLane>,
    pub(crate) signal: SubmitSignal,
    pub(crate) packets: Vec<QueuePacket>,
    pub(crate) usage: UsageToken,
}

impl Submission {
    fn new(
        lanes: LaneVec<SubmissionLane>,
        signal: SubmitSignal,
        packets: Vec<QueuePacket>,
    ) -> Self {
        Self {
            lanes,
            signal,
            packets,
            usage: UsageToken::new(),
        }
    }
}
