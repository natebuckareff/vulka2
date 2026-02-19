use anyhow::{Context, Result, anyhow};
use smallvec::SmallVec;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::gpu_v2::{
    Device, LivenessGuard, LivenessToken, QueueBinding, QueueGroupId, QueueGroupInfo,
    QueueGroupTable, QueueId, QueueRoleFlags, SubmissionId,
};

pub struct CommandAllocator {
    id: usize,
    device: Arc<Device>,
    queue_group_table: QueueGroupTable, // TODO: inner vs outer Arc design?
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
        if capacity == 0 {
            return Err(anyhow!("capacity must be greater than 0"));
        }
        // TODO: really don't like the inner-Arc design. It's hard to reason about
        let queue_group_table = device.queue_group_table().clone();
        let queue_info = queue_group_table
            .get_info(queue_group_id)
            .context("queue group not found")?
            .clone();
        let allocator = Self {
            id,
            device,
            queue_group_table,
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

    pub fn acquire(&mut self) -> Result<Option<CommandPool>> {
        // check if self.ready has something
        if let Some(pool) = self.ready.pop_front() {
            return Ok(Some(pool));
        }

        let pool = if self.len() < self.capacity {
            // check if we're under capacity and can just create a new pool
            Some(self.create_pool()?)
        } else {
            // otherwise call self.reclaim(), blocking if necessary
            self.reclaim()?
        };

        if pool.is_some() {
            self.acquired += 1;
        }

        Ok(pool)
    }

    fn create_pool(&mut self) -> Result<CommandPool> {
        let pool = CommandPool::new(
            self.id,
            self.next_pool_id,
            self.queue_info.clone(),
            self.liveness.guard(),
        )?;
        self.next_pool_id = self.next_pool_id.checked_add(1).expect("pool id overflow");
        Ok(pool)
    }

    fn reclaim(&mut self) -> Result<Option<CommandPool>> {
        use vulkanalia::prelude::v1_3::*;

        if self.pending.is_empty() {
            // nothing to reclaim
            return Ok(None);
        }

        loop {
            // will poll the current timeline value for each lane's semaphore,
            // stack allocating for up to 4 lanes
            const MAX_LANES: usize = 4;
            let device = self.device.vk_device();
            let mut current_values: SmallVec<[u64; MAX_LANES]> = SmallVec::new();

            for binding in self.queue_info.bindings.iter() {
                let value = unsafe { device.get_semaphore_counter_value(binding.semaphore) }?;
                current_values.push(value);
            }

            // loop through each pool and check if all lanes for that pool are
            // either unsubmitted or are no longer in use by the gpu
            'pools: for i in 0..self.pending.len() {
                let pool = &self.pending[i];

                for lane in 0..pool.pool_state.lanes.len() {
                    let last_submitted = pool.pool_state.last_submitted[lane];

                    // check that the pool actually submitted on this lane
                    if last_submitted.is_set() {
                        // the last polled value is still less than the expected
                        // value, therefore the pool is not ready to be
                        // reclaimed
                        if current_values[lane] < last_submitted.into() {
                            continue 'pools;
                        }
                    }
                }

                // if we get here after looping over each lane, then all lanes
                // for the pool were either never submitted or >= their
                // last_submitted values, and therefore the pool is reclaimable
                let mut pool = self.pending.swap_remove(i);
                pool.reset();
                return Ok(Some(pool));
            }

            // all pools in self.pending are still pending for at least one lane

            // will build a list of semaphores, and will wait until at least one
            // signals a timeline value >= the corresponding value
            type WaitItem = (vk::Semaphore, u64);
            let mut wait_list = SmallVec::<[WaitItem; MAX_LANES]>::new();

            for binding in self.queue_info.bindings.iter() {
                // use u64::MAX as a placeholder for now
                wait_list.push((binding.semaphore, u64::MAX));
            }

            for pool in self.pending.iter() {
                for lane in 0..pool.pool_state.lanes.len() {
                    let last_submitted = pool.pool_state.last_submitted[lane];

                    // only include this pool's lane value in the wait list if
                    // it was actually set *and* the last-polled value is less
                    // that the value it is waiting for still. this is necessary
                    // otherwise device.wait_semaphores will immediate return
                    // since this value is already signalled

                    if last_submitted.is_set() && current_values[lane] < last_submitted.into() {
                        let wait_value = wait_list[lane].1;
                        if wait_value == u64::MAX {
                            wait_list[lane].1 = last_submitted.into();
                        } else {
                            // take the min so device.wait_semaphores wakes as soon
                            // as possible
                            wait_list[lane].1 = wait_value.min(last_submitted.into());
                        }
                    }
                }
            }

            // filter out any lanes that were not pending for all pending pools
            wait_list.retain(|(_, value)| *value < u64::MAX);

            // in order for wait_list to be empty, all lanes for all pools must
            // be unset, but that would've caused an early return in the pool
            // poll loop, therefore it's not possible at this point
            assert!(!wait_list.is_empty());

            let (wait_semaphores, wait_values): (
                SmallVec<[vk::Semaphore; MAX_LANES]>,
                SmallVec<[u64; MAX_LANES]>,
            ) = wait_list.into_iter().unzip();

            let info = vk::SemaphoreWaitInfo::builder()
                .flags(vk::SemaphoreWaitFlags::ANY)
                .semaphores(&wait_semaphores)
                .values(&wait_values);

            // TODO: timeout

            // wait for any semaphore to signal
            unsafe { device.wait_semaphores(&info, u64::MAX) }?;

            // loop and repeat, but with fresh semaphore timeline values for
            // each lane, and therefore the lanes that caused a wait this
            // iteration will not cause a wait next iteration
        }
    }

    pub fn release(&mut self, pool: CommandPool) -> Result<()> {
        if self.id != pool.allocator_id {
            return Err(anyhow!("allocator id mismatch"));
        }
        self.acquired = self.acquired.checked_sub(1).context("acquired underflow")?;
        self.pending.push(pool);
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct QueueLane {
    id: QueueId,
    roles: QueueRoleFlags,
}

struct PoolState {
    lanes: Vec<QueueLane>,             // OPTIMIZE: SmallVec
    last_submitted: Vec<SubmissionId>, // OPTIMIZE: SmallVec
}

impl PoolState {
    fn new(bindings: &[QueueBinding]) -> Result<Self> {
        let mut lanes = Vec::with_capacity(bindings.len());
        let mut last_submitted = Vec::with_capacity(bindings.len());
        for binding in bindings {
            lanes.push(QueueLane {
                id: binding.id,
                roles: binding.roles,
            });
            last_submitted.push(SubmissionId::new(0)?);
        }
        Ok(Self {
            lanes,
            last_submitted,
        })
    }
}

pub struct CommandPool {
    allocator_id: usize,
    pool_id: usize, // TODO: needs to be (allocator_id, pool_id) for correctness
    queue_info: QueueGroupInfo,
    pool_state: PoolState,
    liveness: LivenessToken,
    guard: LivenessGuard,
}

impl CommandPool {
    // TODO: probably need to pass in a QueueGroupInfo or something that comes
    // from Device
    fn new(
        allocator_id: usize,
        pool_id: usize,
        queue_info: QueueGroupInfo, // TODO: do we need full QueueGroupInfo?
        guard: LivenessGuard,
    ) -> Result<Self> {
        let pool_state = PoolState::new(&queue_info.bindings)?;
        Ok(Self {
            allocator_id,
            pool_id,
            queue_info,
            pool_state,
            liveness: LivenessToken::new(),
            guard,
        })
    }

    fn allocate(&mut self) -> Result<CommandBuffer> {
        Ok(CommandBuffer::new(
            self.pool_id,
            self.queue_info.id,
            &self.pool_state,
            self.liveness.guard(),
        )?)
    }

    fn batch(&mut self) -> CommandBatch {
        CommandBatch::new(self.pool_id, self.queue_info.id)
    }

    fn reset(&mut self) {
        todo!()
        // self.future_set.reset()
    }

    fn flush(&mut self, batch: CommandBatch) -> Result<Submission> {
        if self.pool_id != batch.pool_id {
            return Err(anyhow!("mismatched pool id"));
        }
        if self.queue_info.id != batch.queue_group_id {
            return Err(anyhow!("mismatched queue groups"));
        }

        let (batch_state, packets) = batch.consume();

        let Some(batch_state) = batch_state else {
            // TODO: same as empty?
            return Err(anyhow!("no queues in batch"));
        };

        if packets.is_empty() {
            // TODO: maybe not an error??
            return Err(anyhow!("empty batch"));
        }

        let mut i = 0;
        while i < batch_state.lanes.len() {
            if batch_state.dirty[i] {
                let binding = &self.queue_info.bindings[i];
                let value = binding.counter.reserve()?;
                let last_submitted = self.pool_state.last_submitted[i];
                self.pool_state.last_submitted[i] = last_submitted.max(value);
            }
            i += 1;
        }

        let queue_group_id = self.queue_info.id;
        let submission = Submission::new(queue_group_id, packets);
        Ok(submission)
    }
}
struct CommandBuffer {
    pool_id: usize,
    queue_group_id: QueueGroupId,
    batch_state: BatchState,
    guard: LivenessGuard,
}

impl CommandBuffer {
    fn new(
        pool_id: usize,
        queue_group_id: QueueGroupId,
        pool_state: &PoolState,
        guard: LivenessGuard,
    ) -> Result<Self> {
        let batch_state = BatchState::new(pool_state)?;
        Ok(Self {
            pool_id,
            queue_group_id,
            batch_state,
            guard,
        })
    }

    // TODO: this is private and will only be called from the command recording
    // methods after choosing the appropriate queue based on role using
    // self.queues
    // fn touch(&mut self, id: QueueId) {
    //     debug_assert!(self.queue_info.bindings.iter().any(|q| q.id == id));
    //     if !self.dirty.contains(&id) {
    //         self.dirty.push(id);
    //     }
    // }

    fn finish(mut self) -> CommandRecording {
        CommandRecording::new(self.pool_id, self.queue_group_id, self.batch_state)
    }
}

struct RecordingPacket {
    // TODO: will only put in here what the QueueGroup needs to submit
}

struct CommandRecording {
    pool_id: usize,
    queue_group_id: QueueGroupId,
    batch_state: BatchState,
    packet: RecordingPacket,
}

impl CommandRecording {
    fn new(pool_id: usize, queue_group_id: QueueGroupId, batch_state: BatchState) -> Self {
        Self {
            pool_id,
            queue_group_id,
            batch_state,
            packet: RecordingPacket {},
        }
    }
}

struct BatchState {
    lanes: Vec<QueueLane>, // OPTIMIZE: SmallVec
    dirty: Vec<bool>,      // OPTIMIZE: SmallVec
}

impl BatchState {
    fn new(pool_state: &PoolState) -> Result<Self> {
        let lanes = pool_state.lanes.clone();
        let mut dirty = Vec::new();
        dirty.resize_with(pool_state.lanes.len(), || false);
        Ok(Self { lanes, dirty })
    }
}

struct CommandBatch {
    pool_id: usize,
    queue_group_id: QueueGroupId,
    batch_state: Option<BatchState>,
    packets: Vec<RecordingPacket>,
    is_dirty: bool,
    is_flushed: bool,
}

impl CommandBatch {
    fn new(pool_id: usize, queue_group_id: QueueGroupId) -> Self {
        Self {
            pool_id,
            queue_group_id,
            batch_state: None,
            packets: Vec::new(),
            is_dirty: false,
            is_flushed: false,
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
        self.is_dirty = true;
        match &mut self.batch_state {
            Some(batch_state) => {
                let mut i = 0;
                while i < recording.batch_state.lanes.len() {
                    batch_state.dirty[i] |= recording.batch_state.dirty[i];
                    i += 1;
                }
            }
            None => {
                self.batch_state = Some(recording.batch_state);
            }
        }
        self.packets.push(recording.packet);
        Ok(())
    }

    // should only be called from flush()
    fn consume(mut self) -> (Option<BatchState>, Vec<RecordingPacket>) {
        self.is_flushed = true;
        (self.batch_state.take(), std::mem::take(&mut self.packets))
    }
}

impl Drop for CommandBatch {
    fn drop(&mut self) {
        if self.is_dirty {
            debug_assert!(self.is_flushed, "command batch dirty but not flushed");
        }
    }
}

struct Submission {
    queue_group_id: QueueGroupId,
    packets: Vec<RecordingPacket>,
}

impl Submission {
    fn new(queue_group_id: QueueGroupId, packets: Vec<RecordingPacket>) -> Self {
        Self {
            queue_group_id,
            packets,
        }
    }
}
