use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use smallvec::SmallVec;

use crate::gpu_v2::{
    CommandBatch, CommandPool, Device, GpuFutureState, LivenessToken, MAX_STATIC_LANES,
    QueueGroupId, QueueGroupInfo, QueueGroupTable, SubmitSignal,
};

pub struct CommandAllocator {
    id: usize,
    device: Arc<Device>,
    queue_group_table: QueueGroupTable, // TODO: inner vs outer Arc design?
    queue_info: QueueGroupInfo,
    capacity: usize,
    signal: SubmitSignal,
    waiting: Vec<CommandPool>,
    pending: Vec<CommandPool>,
    ready: VecDeque<CommandPool>,
    acquired: usize,
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
            signal: SubmitSignal::new(),
            waiting: Vec::new(),
            pending: Vec::new(),
            ready: VecDeque::new(),
            acquired: 0,
            liveness: LivenessToken::new(),
        };
        Ok(allocator)
    }

    pub fn len(&self) -> usize {
        self.waiting.len() + self.pending.len() + self.ready.len() + self.acquired
    }

    pub fn acquire(&mut self) -> Result<Option<CommandBatch>> {
        let Some(pool) = self.acquire_pool()? else {
            return Ok(None);
        };
        let batch = CommandBatch::new(self.signal.clone(), pool);
        Ok(Some(batch))
    }

    fn acquire_pool(&mut self) -> Result<Option<CommandPool>> {
        loop {
            // attempt to acquire on the fast path:
            // 1. first from ready
            // 2. then creating a new pool if under capacity
            // 3. otherwise moving from waiting to pending and attempting to reclaim
            if let Some(pool) = self.acquire_ready_or_pending()? {
                return Ok(Some(pool));
            }

            // it nothing *and* no pools are waiting, there is really nothing
            if self.waiting.is_empty() {
                return Ok(None);
            }

            // read the current condition value
            let seen = {
                let guard = self.signal.lock();
                *guard
            };

            // attempt to acquire again in case some waiting pools can now move
            // to pending
            if let Some(pool) = self.acquire_ready_or_pending()? {
                return Ok(Some(pool));
            }

            // nothing to acquire
            if self.waiting.is_empty() {
                return Ok(None);
            }

            // read current condition again
            let mut guard = self.signal.lock();

            // loop and wait until until the condition changes
            while *guard == seen {
                guard = self.signal.wait(guard);
            }
        }
    }

    fn acquire_ready_or_pending(&mut self) -> Result<Option<CommandPool>> {
        // check if self.ready has something
        if let Some(pool) = self.ready.pop_front() {
            self.acquired += 1;
            return Ok(Some(pool));
        }

        let pool = if self.len() < self.capacity {
            // under capacity and can just create a new pool
            Some(self.create_pool()?)
        } else {
            // attempt to reclaim a pending pool
            self.reclaim()?
        };

        if pool.is_some() {
            self.acquired += 1;
        }

        debug_assert!(self.len() <= self.capacity);

        Ok(pool)
    }

    fn create_pool(&mut self) -> Result<CommandPool> {
        let pool = CommandPool::new(self.id, self.queue_info.clone(), self.liveness.guard())?;
        Ok(pool)
    }

    fn reclaim(&mut self) -> Result<Option<CommandPool>> {
        use vulkanalia::prelude::v1_3::*;

        // first, attempt to move waiting pools into pending if their waiting
        // futures are all set
        let mut i = 0;
        'pools: while i < self.waiting.len() {
            let pool = &self.waiting[i];

            // TODO: move to PoolLane::is_ready() or something
            for lane in pool.lanes().iter() {
                match lane.future.get()? {
                    GpuFutureState::Unset => {
                        // skip lanes that were never submitted
                        continue;
                    }
                    GpuFutureState::Waiting => {
                        // if the lane is still waiting on a submission, then
                        // this pool must remain in waiting
                        i += 1;
                        continue 'pools;
                    }
                    GpuFutureState::Set(_) => {
                        // future for this lane was set, fallthrough to
                        // potentially move this pool from waiting to pending
                    }
                }
            }
            let pool = self.waiting.swap_remove(i);
            self.pending.push(pool);
        }

        if self.pending.is_empty() {
            // nothing to reclaim
            return Ok(None);
        }

        loop {
            // will poll the current timeline value for each lane's semaphore
            let device = self.device.vk_device();
            let mut current_values: SmallVec<[u64; MAX_STATIC_LANES]> = SmallVec::new();

            for binding in self.queue_info.bindings.iter() {
                let value = unsafe { device.get_semaphore_counter_value(binding.semaphore) }?;
                current_values.push(value);
            }

            // loop through each pool and check if all lanes for that pool are
            // either unsubmitted or are no longer in use by the gpu
            'pools: for i in 0..self.pending.len() {
                let pool = &self.pending[i];

                debug_assert_eq!(pool.lanes().len(), current_values.len());

                for lane in 0..pool.lanes().len() {
                    match pool.lanes()[lane].future.get()? {
                        GpuFutureState::Unset => {
                            // skip lanes that were never submitted
                            continue;
                        }
                        GpuFutureState::Waiting => {
                            unreachable!("waiting future should never be in pending");
                        }
                        GpuFutureState::Set(value) => {
                            // the last polled value is still less than the
                            // expected value, therefore the pool is not ready
                            // to be reclaimed
                            if current_values[lane] < value.into() {
                                continue 'pools;
                            }
                        }
                    }
                }

                // if we get here after looping over each lane, then all lanes
                // for the pool were either never submitted or >= their timeline
                // values, and therefore the pool is reclaimable
                let mut pool = self.pending.swap_remove(i);
                pool.reset()?;
                return Ok(Some(pool));
            }

            // all pools in self.pending are still pending for at least one lane

            // will build a list of semaphores, and will wait until at least one
            // signals a timeline value >= the corresponding value
            type WaitItem = (vk::Semaphore, u64);
            let mut wait_list = SmallVec::<[WaitItem; MAX_STATIC_LANES]>::new();

            for binding in self.queue_info.bindings.iter() {
                // use u64::MAX as a placeholder for now
                wait_list.push((binding.semaphore, u64::MAX));
            }

            for pool in self.pending.iter() {
                debug_assert_eq!(pool.lanes().len(), wait_list.len());

                for lane in 0..pool.lanes().len() {
                    let value = match pool.lanes()[lane].future.get()? {
                        GpuFutureState::Unset => {
                            // skip lanes that were never submitted
                            continue;
                        }
                        GpuFutureState::Waiting => {
                            unreachable!("waiting future should never be in pending");
                        }
                        GpuFutureState::Set(value) => value.into(),
                    };

                    // only include this pool's lane value in the wait list if
                    // the last-polled value is less that the value it is
                    // waiting for still. this is necessary otherwise
                    // device.wait_semaphores will immediate return since this
                    // value is already signalled

                    if current_values[lane] < value {
                        let wait_value = wait_list[lane].1;
                        if wait_value == u64::MAX {
                            wait_list[lane].1 = value;
                        } else {
                            // take the min so device.wait_semaphores wakes as soon
                            // as possible
                            wait_list[lane].1 = wait_value.min(value);
                        }
                    }
                }
            }

            // filter out any lanes that were not pending for all pending pools
            wait_list.retain(|(_, value)| *value < u64::MAX);

            // INVARIANT: in order for wait_list to be empty, all lanes for all
            // pools must be unset, but that would've caused an early return in
            // the pool poll loop, therefore it's not possible at this point
            assert!(
                !wait_list.is_empty(),
                "wait list is empty but pending pools exist"
            );

            let (wait_semaphores, wait_values): (
                SmallVec<[vk::Semaphore; MAX_STATIC_LANES]>,
                SmallVec<[u64; MAX_STATIC_LANES]>,
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
        if self.id != pool.allocator_id() {
            return Err(anyhow!("allocator id mismatch"));
        }
        self.acquired = self.acquired.checked_sub(1).context("acquired underflow")?;
        self.waiting.push(pool);

        debug_assert!(self.len() <= self.capacity);

        Ok(())
    }
}
