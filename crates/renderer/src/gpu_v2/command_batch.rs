use anyhow::{Result, anyhow};
use smallvec::SmallVec;

use crate::gpu_v2::{
    CommandBuffer, CommandPool, GpuFutureWriter, MAX_LANES, QueueGroupId, SubmitSignal,
};

pub(crate) struct QueuePacket {
    pub(crate) lane_index: usize,
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
        type Futures = SmallVec<[Option<GpuFutureWriter>; MAX_LANES]>;
        let mut futures: Futures = SmallVec::with_capacity(MAX_LANES);
        futures.resize_with(self.pool.lanes().len(), || None);

        let mut packets = vec![];
        for buffers in self.buffers.into_iter() {
            for (lane_index, lane) in buffers.lanes().iter().enumerate() {
                if lane.dirty {
                    if futures[lane_index].is_none() {
                        futures[lane_index] = Some(self.pool.lanes()[lane_index].future.send()?);
                    }
                    packets.push(QueuePacket { lane_index });
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
    pub(crate) futures: SmallVec<[Option<GpuFutureWriter>; MAX_LANES]>,
    pub(crate) packets: Vec<QueuePacket>,
}

impl Submission {
    fn new(
        queue_group_id: QueueGroupId,
        signal: SubmitSignal,
        futures: SmallVec<[Option<GpuFutureWriter>; MAX_LANES]>,
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
