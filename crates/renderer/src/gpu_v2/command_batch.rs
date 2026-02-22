use anyhow::{Result, anyhow};
use smallvec::SmallVec;

use crate::gpu_v2::{
    CommandBuffer, CommandPool, GpuFutureWriter, LaneIndex, LaneVec, QueueGroupId, SubmitSignal,
};

pub(crate) struct QueuePacket {
    pub(crate) index: LaneIndex,
    // TODO: will contain only what the queue will need to submit
}

pub struct CommandBatch {
    signal: SubmitSignal,
    pool: CommandPool,
    buffers: Vec<CommandBuffer>,
}

impl CommandBatch {
    pub(crate) fn new(signal: SubmitSignal, pool: CommandPool) -> Self {
        Self {
            signal,
            pool,
            buffers: Vec::new(),
        }
    }

    pub fn allocate(&mut self) -> Result<CommandBuffer> {
        CommandBuffer::new(
            self.pool.queue_group_id(),
            self.pool.lanes(),
            self.pool.liveness().guard(),
        )
    }

    pub fn add(&mut self, buffer: CommandBuffer) -> Result<()> {
        if self.pool.queue_group_id() != buffer.queue_group_id() {
            return Err(anyhow!("mismatched queue groups"));
        }
        self.buffers.push(buffer);
        Ok(())
    }

    pub fn finish(self) -> Result<(Submission, CommandPool)> {
        type Futures = LaneVec<Option<GpuFutureWriter>>;
        let pool_lanes = self.pool.lanes();
        let mut futures: Futures = LaneVec::new(self.pool.queue_group_id(), pool_lanes.len());
        let mut packets = vec![];

        for buffers in self.buffers.into_iter() {
            for (index, lane) in buffers.lanes().iter_entries() {
                if lane.dirty {
                    if futures.get(index).is_none() {
                        let value = pool_lanes.get(index).future.send()?;
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

// TODO: panic if batch is never finished
// impl Drop for CommandBatch {
//     fn drop(&mut self) {
//     }
// }

pub struct Submission {
    pub(crate) queue_group_id: QueueGroupId,
    pub(crate) signal: SubmitSignal,
    pub(crate) futures: LaneVec<Option<GpuFutureWriter>>,
    pub(crate) packets: Vec<QueuePacket>,
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
        }
    }
}
