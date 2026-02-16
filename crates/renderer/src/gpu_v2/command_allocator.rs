use anyhow::{Context, Result, anyhow};
use std::collections::VecDeque;
use std::sync::Arc;

use crate::gpu_v2::{
    Device, GpuFuture, GpuFutureSet, LivenessGuard, LivenessToken, QueueGroupId, QueueGroupInfo,
    QueueId,
};

pub struct CommandAllocator {
    id: usize,
    device: Arc<Device>,
    queue_info: QueueGroupInfo,
    capacity: usize,
    pending: Vec<CommandPool>,
    ready: VecDeque<CommandPool>,
    acquired: usize,
    next_pool_id: usize,
    liveness: LivenessToken,
}

impl CommandAllocator {
    pub(crate) fn new(
        id: usize,
        device: Arc<Device>,
        queue_group_id: QueueGroupId,
        capacity: usize,
    ) -> Result<Self> {
        let queue_info = device
            .queue_group_table()
            .get_info(queue_group_id)
            .context("queue group not found")?
            .clone();
        let allocator = Self {
            id,
            device,
            queue_info,
            capacity,
            pending: Vec::new(),
            ready: VecDeque::new(),
            acquired: 0,
            next_pool_id: 0,
            liveness: LivenessToken::new(),
        };
        Ok(allocator)
    }

    fn len(&self) -> usize {
        self.pending.len() + self.ready.len() + self.acquired
    }

    pub fn acquire(&mut self) -> Result<CommandPool> {
        // check if self.ready has something
        if let Some(pool) = self.ready.pop_front() {
            return Ok(pool);
        }

        let pool = if self.len() < self.capacity {
            // check if we're under capacity and can just create a new pool
            self.create_pool()?
        } else {
            // otherwise call self.reclaim(), blocking if necessary
            self.reclaim()?
        };

        self.acquired += 1;
        Ok(pool)
    }

    fn create_pool(&mut self) -> Result<CommandPool> {
        let pool = CommandPool::new(
            self.id,
            self.next_pool_id,
            self.queue_info.clone(),
            self.liveness.guard(),
        );
        self.next_pool_id = self.next_pool_id.checked_add(1).expect("pool id overflow");
        Ok(pool)
    }

    fn reclaim(&mut self) -> Result<CommandPool> {
        todo!()
    }

    pub fn release(&mut self, pool: CommandPool) -> Result<()> {
        self.acquired -= 1;
        self.ready.push_back(pool);
        Ok(())
    }
}

struct CommandPool {
    allocator_id: usize,
    pool_id: usize,
    queue_info: QueueGroupInfo,
    future_set: GpuFutureSet,
    liveness: LivenessToken,
    guard: LivenessGuard,
}

impl CommandPool {
    // TODO: probably need to pass in a QueueGroupInfo or something that comes
    // from Device
    fn new(
        allocator_id: usize,
        pool_id: usize,
        queue_info: QueueGroupInfo,
        guard: LivenessGuard,
    ) -> Self {
        let mut future_set = GpuFutureSet::with_capacity(queue_info.bindings.len());
        for binding in &queue_info.bindings {
            future_set.add_if_not_present(GpuFuture::new(binding.id));
        }
        Self {
            allocator_id,
            pool_id,
            queue_info,
            future_set,
            liveness: LivenessToken::new(),
            guard,
        }
    }

    fn allocate(&mut self) -> CommandBuffer {
        let guard = self.liveness.guard();
        CommandBuffer::new(
            self.pool_id,
            self.queue_info.clone(),
            self.future_set.clone(),
            guard,
        )
    }

    fn batch(&mut self) -> CommandBatch {
        CommandBatch::new(self.pool_id, self.queue_info.id)
    }
}
struct CommandBuffer {
    pool_id: usize,
    queue_info: QueueGroupInfo,
    future_set: GpuFutureSet,
    dirty: Vec<QueueId>, // OPTIMIZE: can also be a SmallVec
    guard: LivenessGuard,
}

impl CommandBuffer {
    fn new(
        pool_id: usize,
        queue_info: QueueGroupInfo,
        future_set: GpuFutureSet,
        guard: LivenessGuard,
    ) -> Self {
        Self {
            pool_id,
            queue_info,
            future_set,
            dirty: Vec::new(),
            guard,
        }
    }

    // TODO: this is private and will only be called from the command recording
    // methods after choosing the appropriate queue based on role using
    // self.queues
    fn touch(&mut self, id: QueueId) {
        debug_assert!(self.queue_info.bindings.iter().any(|q| q.id == id));
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

        CommandRecording::new(self.pool_id, self.queue_info.id, dirty_futures)
    }
}

struct RecordingPacket {
    // TODO: will only put in here what the QueueGroup needs to submit
}

struct CommandRecording {
    pool_id: usize,
    queue_group_id: QueueGroupId,
    futures: Vec<GpuFuture>,
    packet: RecordingPacket,
}

impl CommandRecording {
    fn new(pool_id: usize, queue_group_id: QueueGroupId, futures: Vec<GpuFuture>) -> Self {
        Self {
            pool_id,
            queue_group_id,
            futures,
            packet: RecordingPacket {},
        }
    }
}

struct CommandBatch {
    pool_id: usize,
    queue_group_id: QueueGroupId,
    future_set: GpuFutureSet,
    packets: Vec<RecordingPacket>,
}

impl CommandBatch {
    fn new(pool_id: usize, queue_group_id: QueueGroupId) -> Self {
        Self {
            pool_id,
            queue_group_id,
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
        if self.queue_group_id != recording.queue_group_id {
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
        let submission = Submission::new(self.queue_group_id, packets, futures);
        Ok(submission)
    }
}

struct Submission {
    queue_group_id: QueueGroupId,
    packets: Vec<RecordingPacket>,
    futures: Vec<GpuFuture>,
}

impl Submission {
    fn new(
        queue_group_id: QueueGroupId,
        packets: Vec<RecordingPacket>,
        futures: Vec<GpuFuture>,
    ) -> Self {
        Self {
            queue_group_id,
            packets,
            futures,
        }
    }
}
