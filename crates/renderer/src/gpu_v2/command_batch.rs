use anyhow::{Result, anyhow};

use crate::gpu_v2::{
    CommandBuffer, CommandPool, GpuFutureWriter, LaneIndex, LaneVec, QueueGroupId, SubmitSignal,
    UsageToken,
};

pub(crate) struct QueuePacket {
    pub(crate) index: LaneIndex,
    // TODO: will contain only what the queue will need to submit
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

        type Futures = LaneVec<Option<GpuFutureWriter>>;
        let pool_lanes = self.pool.lanes();
        let mut futures: Futures = LaneVec::filled(pool_lanes, || None);
        let mut packets = vec![];

        for buffer in self.buffers.into_iter() {
            for (index, lane) in buffer.lanes().iter_entries() {
                if lane.dirty {
                    if futures.get(index).is_none() {
                        // TODO: there is a lifecycle hole here if there is an
                        // error on send, owned pool is dropped
                        let value = pool_lanes.get(index).future().send()?;
                        futures.set(index, Some(value));
                    }
                    packets.push(QueuePacket { index });
                }
            }
        }

        let pool = self.pool;
        let submission = Submission::new(pool.queue_group_id(), self.signal, futures, packets);
        Ok((submission, pool))
    }
}

pub struct Submission {
    pub(crate) queue_group_id: QueueGroupId,
    pub(crate) signal: SubmitSignal,
    pub(crate) futures: LaneVec<Option<GpuFutureWriter>>,
    pub(crate) packets: Vec<QueuePacket>,
    pub(crate) usage: UsageToken,
}

impl Submission {
    fn new(
        queue_group_id: QueueGroupId,
        signal: SubmitSignal,
        futures: LaneVec<Option<GpuFutureWriter>>,
        packets: Vec<QueuePacket>,
    ) -> Self {
        Self {
            queue_group_id,
            signal,
            futures,
            packets,
            usage: UsageToken::new(),
        }
    }
}
