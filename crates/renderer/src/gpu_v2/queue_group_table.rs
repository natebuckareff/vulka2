use std::{collections::HashMap, sync::Arc};

use vulkanalia::vk;

use crate::gpu_v2::{
    LaneVec, QueueGroup, QueueGroupId, QueueId, QueueRoleFlags, SubmissionCounter,
};

#[derive(Clone)]
pub struct QueueGroupInfo {
    pub id: QueueGroupId,
    pub bindings: LaneVec<QueueBinding>,
}

#[derive(Debug, Clone)]
pub struct QueueBinding {
    pub id: QueueId,
    pub roles: QueueRoleFlags,
    pub counter: Arc<SubmissionCounter>,
    pub semaphore: vk::Semaphore,
}

struct Inner {
    infos: Vec<QueueGroupInfo>,
}

impl Inner {
    fn new(queue_groups: &HashMap<QueueGroupId, QueueGroup>) -> Self {
        let mut infos = Vec::with_capacity(queue_groups.len());
        for qg in queue_groups.values() {
            let mut info = QueueGroupInfo {
                id: qg.id(),
                bindings: LaneVec::with_lanes(qg.queues()),
            };
            for queue in qg.queues().iter() {
                let binding = QueueBinding {
                    id: queue.id(),
                    roles: queue.roles(),
                    counter: queue.submission_counter().clone(),
                    semaphore: queue.semaphore(),
                };
                info.bindings.push(binding);
            }
            infos.push(info);
        }
        infos.sort_by_key(|info| info.id);
        Self { infos }
    }
}

#[derive(Clone)]
pub struct QueueGroupTable {
    device: vulkanalia::Device,
    inner: Arc<Inner>,
}

impl QueueGroupTable {
    pub(crate) fn new(
        device: vulkanalia::Device,
        queue_groups: &HashMap<QueueGroupId, QueueGroup>,
    ) -> Self {
        let inner = Inner::new(queue_groups);
        Self {
            device,
            inner: Arc::new(inner),
        }
    }

    // fn get_semaphore(&self, id: QueueId) -> Result<vk::Semaphore> {
    //     let semaphore = self
    //         .inner
    //         .semaphores
    //         .get(&(id.family.into(), id.index))
    //         .context("semaphore not found")?;
    //     Ok(*semaphore)
    // }

    pub fn get_info(&self, id: QueueGroupId) -> Option<&QueueGroupInfo> {
        self.inner.infos.iter().find(|info| info.id == id)
    }

    // pub fn last_submission_id(&self, id: QueueId) -> Result<SubmissionId> {
    //     use vulkanalia::prelude::v1_3::*;
    //     let semaphore = self.get_semaphore(id)?;
    //     let value = unsafe { self.device.get_semaphore_counter_value(semaphore) }?;
    //     Ok(SubmissionId::new(value)?)
    // }

    // pub fn is_future_ready(&self, id: QueueId, future: &GpuFuture) -> Result<bool> {
    //     let value = self.last_submission_id(id)?;
    //     match future.get() {
    //         Ok(Some(until)) => Ok(value >= until),
    //         Ok(None) => Ok(false),
    //         Err(e) => Err(e),
    //     }
    // }

    // pub fn all_futures_ready(&self, futures: &[GpuFuture]) -> Result<bool> {
    //     for future in futures {
    //         if !self.is_future_ready(future.queue_id(), future)? {
    //             return Ok(false);
    //         }
    //     }
    //     Ok(true)
    // }

    // pub fn wait_for_all_futures(&self, futures: &[GpuFuture]) -> Result<()> {
    //     // TODO: need to enable this feature on the device
    //     use vulkanalia::prelude::v1_3::*;

    //     // OPTIMIZE: Allocations in hot-path. Should split this out into a
    //     // GpuFutureWaiter or something like that so we can reuse the scratch
    //     // space
    //     let mut semaphores = Vec::with_capacity(futures.len());
    //     let mut values = Vec::with_capacity(futures.len());

    //     for future in futures {
    //         let Ok(Some(target)) = future.get() else {
    //             return Err(anyhow!("cannot wait for uninitialized or failed futures"));
    //         };
    //         let semaphore = self.get_semaphore(future.queue_id())?;
    //         semaphores.push(semaphore);
    //         values.push(target.into());
    //     }

    //     if semaphores.is_empty() {
    //         return Ok(());
    //     }

    //     let info = vk::SemaphoreWaitInfo::builder()
    //         .semaphores(&semaphores)
    //         .values(&values);

    //     unsafe { self.device.wait_semaphores(&info, u64::MAX) }?;
    //     Ok(())
    // }
}
