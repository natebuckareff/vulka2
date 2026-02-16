use anyhow::{Result, anyhow};
use std::collections::VecDeque;
use std::sync::Arc;

use crate::gpu_v2::{
    Device, GpuFuture, GpuFutureSet, LivenessGuard, LivenessToken, QueueAllocation, QueueFamilyId,
    QueueGroupId, QueueId, QueueRoleFlags,
};

struct QueueGroupData {
    id: QueueGroupId,
    queue_ids: Vec<QueueId>, // OPTIMIZE: SmallVec
    allocations: Vec<QueueAllocation>,
}

pub struct CommandAllocator {
    device: Arc<Device>,
    queue_group: QueueGroupData,
    capacity: usize,
    pending: Vec<CommandPool>,
    ready: VecDeque<CommandPool>,
    active: Option<CommandPool>,
    next_pool_id: u32,
    liveness: LivenessToken,
}

impl CommandAllocator {
    pub fn new(device: Arc<Device>, id: QueueGroupId, capacity: usize) -> Result<Self> {
        let allocations = device.queue_allocations(id)?.clone();
        let queue_ids = allocations.iter().map(|a| a.queue_id).collect();
        let queue_group = QueueGroupData {
            id,
            queue_ids,
            allocations,
        };
        let allocator = Self {
            device,
            queue_group,
            capacity,
            pending: Vec::new(),
            ready: VecDeque::new(),
            active: None,
            next_pool_id: 0,
            liveness: LivenessToken::new(),
        };
        Ok(allocator)
    }

    pub fn acquire(&mut self) -> Result<CommandPool> {
        todo!()
    }

    pub fn release(&mut self, pool: CommandPool) -> Result<()> {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct CommandQueueSet {
    id: QueueGroupId,
    queues: Vec<(QueueId, QueueRoleFlags)>, // OPTIMIZE: SmallVec
}

struct CommandPool {
    id: u32,
    queue_set: CommandQueueSet,
    guard: LivenessGuard,
    liveness: LivenessToken,
    future_set: GpuFutureSet,
}

impl CommandPool {
    // TODO: probably need to pass in a QueueGroupInfo or something that comes
    // from Device
    fn new(id: u32, queue_set: CommandQueueSet, guard: LivenessGuard) -> Self {
        let mut future_set = GpuFutureSet::with_capacity(queue_set.queues.len());
        for queue in &queue_set.queues {
            future_set.add_if_not_present(GpuFuture::new(queue.0));
        }
        Self {
            id,
            queue_set,
            guard,
            liveness: LivenessToken::new(),
            future_set,
        }
    }

    fn allocate(&mut self) -> CommandBuffer {
        let guard = self.liveness.child();
        CommandBuffer::new(
            self.id,
            self.queue_set.clone(),
            guard,
            self.future_set.clone(),
        )
    }

    fn batch(&mut self) -> CommandBatch {
        CommandBatch::new(self.id, self.queue_set.id)
    }
}
struct CommandBuffer {
    pool_id: u32,
    queue_set: CommandQueueSet,
    guard: LivenessGuard,
    future_set: GpuFutureSet,
    dirty: Vec<QueueId>, // OPTIMIZE: can also be a SmallVec
}

impl CommandBuffer {
    fn new(
        pool_id: u32,
        queue_set: CommandQueueSet,
        guard: LivenessGuard,
        future_set: GpuFutureSet,
    ) -> Self {
        Self {
            pool_id,
            queue_set,
            guard,
            future_set,
            dirty: Vec::new(),
        }
    }

    // TODO: this is private and will only be called from the command recording
    // methods after choosing the appropriate queue based on role using
    // self.queues
    fn touch(&mut self, id: QueueId) {
        debug_assert!(self.queue_set.queues.iter().any(|q| q.0 == id));
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }

    fn finish(mut self) -> CommandRecording {
        // OPTIMIZE: SmallVec
        let dirty_futures = self
            .future_set
            .into_futures()
            .into_iter()
            .filter(|f| self.dirty.contains(&f.queue_id()))
            .collect::<Vec<_>>();

        CommandRecording::new(self.pool_id, self.queue_set.id, dirty_futures)
    }
}

struct RecordingPacket {
    // TODO: will only put in here what the QueueGroup needs to submit
}

struct CommandRecording {
    pool_id: u32,
    queue_group: QueueGroupId,
    futures: Vec<GpuFuture>,
    packet: RecordingPacket,
}

impl CommandRecording {
    fn new(pool_id: u32, queue_group: QueueGroupId, futures: Vec<GpuFuture>) -> Self {
        Self {
            pool_id,
            queue_group,
            futures,
            packet: RecordingPacket {},
        }
    }
}

struct CommandBatch {
    pool_id: u32,
    queue_group: QueueGroupId,
    future_set: GpuFutureSet,
    packets: Vec<RecordingPacket>,
}

impl CommandBatch {
    fn new(pool_id: u32, queue_group: QueueGroupId) -> Self {
        Self {
            pool_id,
            queue_group,
            future_set: GpuFutureSet::new(),
            packets: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.packets.len()
    }

    fn add(&mut self, recording: CommandRecording) -> Result<()> {
        if self.pool_id != recording.pool_id {
            return Err(anyhow!("mismatched pool id"));
        }
        if self.queue_group != recording.queue_group {
            return Err(anyhow!("mismatched queue groups"));
        }
        for future in recording.futures {
            self.future_set.add_if_not_present(future);
        }
        self.packets.push(recording.packet);
        Ok(())
    }

    fn flush(&mut self) -> Result<Submission> {
        if self.packets.is_empty() {
            return Err(anyhow!("empty batch"));
        }
        let packets = std::mem::take(&mut self.packets);
        let futures = self.future_set.take_futures();
        let submission = Submission::new(self.queue_group, packets, futures);
        Ok(submission)
    }
}

struct Submission {
    queue_group: QueueGroupId,
    packets: Vec<RecordingPacket>,
    futures: Vec<GpuFuture>,
}

impl Submission {
    fn new(
        queue_group: QueueGroupId,
        packets: Vec<RecordingPacket>,
        futures: Vec<GpuFuture>,
    ) -> Self {
        Self {
            queue_group,
            packets,
            futures,
        }
    }
}
