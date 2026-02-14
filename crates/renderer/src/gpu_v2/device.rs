use std::sync::Arc;

use anyhow::{Context, Result, anyhow};

use crate::gpu_v2::{
    DeviceInfo, Engine, QueueFamily, QueueFamilyId, QueueGroupBuilder, QueueId, QueueRoleFlags,
};

pub struct DeviceBuilder {
    engine: Arc<Engine>,
    info: DeviceInfo,
    families: Vec<QueueFamilyState>,
}

#[derive(Debug, Clone, Copy)]
struct QueueFamilyState {
    id: QueueFamilyId,
    roles: QueueRoleFlags,
    available_count: u32,
    next_queue_index: u32,
}

impl QueueFamilyState {
    fn new(family: QueueFamily) -> Self {
        Self {
            id: family.id,
            roles: family.roles,
            available_count: family.count,
            next_queue_index: 0,
        }
    }

    fn to_queue_family(self) -> QueueFamily {
        QueueFamily {
            id: self.id,
            roles: self.roles,
            count: self.available_count,
        }
    }
}

impl DeviceBuilder {
    pub(crate) fn new(engine: Arc<Engine>, info: DeviceInfo) -> Self {
        let families = info
            .families
            .iter()
            .copied()
            .map(QueueFamilyState::new)
            .collect();

        Self {
            engine,
            info,
            families,
        }
    }

    pub(crate) fn available_queue_families(&self) -> Vec<QueueFamily> {
        self.families
            .iter()
            .copied()
            .map(QueueFamilyState::to_queue_family)
            .collect()
    }

    pub(crate) fn reserve_queue(&mut self, id: QueueFamilyId) -> Result<QueueId> {
        let family = self
            .families
            .iter_mut()
            .find(|family| family.id == id)
            .context("family not found")?;

        if family.available_count == 0 {
            let id: u32 = id.into();
            return Err(anyhow!("not enough queues available in family {}", id));
        }

        let queue_id = QueueId {
            family: family.id,
            index: family.next_queue_index,
        };

        family.available_count -= 1;
        family.next_queue_index += 1;

        self.families.retain(|family| family.available_count > 0);
        Ok(queue_id)
    }

    pub(crate) fn reserve_queues(&mut self, family_ids: &[QueueFamilyId]) -> Result<Vec<QueueId>> {
        let mut required_counts: Vec<(QueueFamilyId, u32)> = Vec::new();

        for family_id in family_ids {
            match required_counts
                .iter_mut()
                .find(|(required_id, _)| *required_id == *family_id)
            {
                Some((_, count)) => *count += 1,
                None => required_counts.push((*family_id, 1)),
            }
        }

        for (family_id, required_count) in required_counts {
            let family = self
                .families
                .iter()
                .find(|family| family.id == family_id)
                .context("family not found")?;

            if family.available_count < required_count {
                let id: u32 = family_id.into();
                return Err(anyhow!("not enough queues available in family {}", id));
            }
        }

        family_ids
            .iter()
            .copied()
            .map(|family_id| self.reserve_queue(family_id))
            .collect()
    }

    pub fn queue_group(&'_ mut self) -> QueueGroupBuilder<'_> {
        QueueGroupBuilder::new(self)
    }

    pub fn build(self) -> Result<Arc<Device>> {
        Ok(Device::new()?)
    }
}

pub struct Device {
    //
}

impl Device {
    pub(crate) fn new() -> Result<Arc<Self>> {
        let device = Self {};
        Ok(Arc::new(device))
    }
}
