use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};

use crate::gpu_v2::{
    DeviceInfo, Engine, QueueAllocation, QueueFamily, QueueFamilyId, QueueGroup, QueueGroupBuilder,
    QueueId, QueueRoleFlags, select_best_families,
};

pub struct DeviceBuilder {
    engine: Arc<Engine>,
    info: DeviceInfo,
    families: BTreeMap<QueueFamilyId, QueueFamily>,
    allocations: HashMap<QueueFamilyId, u32>,
}

impl DeviceBuilder {
    pub(crate) fn new(engine: Arc<Engine>, info: DeviceInfo) -> Self {
        let families = info.families.clone();
        debug_assert!(families.iter().all(|(id, family)| *id == family.id));

        Self {
            engine,
            info,
            families,
            allocations: HashMap::new(),
        }
    }

    fn available_families(&self) -> Vec<QueueFamily> {
        self.families
            .values()
            .copied()
            .map(|mut family| {
                let allocated = self.allocations.get(&family.id).copied().unwrap_or(0);
                debug_assert!(allocated <= family.count);
                family.count = family.count.saturating_sub(allocated);
                family
            })
            .collect()
    }

    pub(crate) fn allocate_group(&mut self, roles: QueueRoleFlags) -> Result<Option<QueueGroup>> {
        if roles.is_empty() {
            return Ok(None);
        }

        let available_families = self.available_families();
        let selected_families = select_best_families(&available_families, roles);
        if selected_families.is_empty() {
            return Ok(None);
        }

        let allocations = selected_families
            .into_iter()
            .map(|family| self.allocate_queue(family))
            .collect::<Result<Vec<_>>>()?;

        Ok(Some(QueueGroup::new(allocations)))
    }

    fn allocate_queue(&mut self, family: QueueFamily) -> Result<QueueAllocation> {
        let id = family.id;
        let family = self.families.get(&id).context("family not found")?;
        let allocated = self.allocations.get(&id).copied().unwrap_or(0);

        if allocated >= family.count {
            let id: u32 = id.into();
            return Err(anyhow!("not enough queues available in family {}", id));
        }

        let queue_id = QueueId {
            family: id,
            index: allocated,
        };

        self.allocations.insert(id, allocated + 1);

        Ok(QueueAllocation {
            queue_id,
            roles: family.roles,
        })
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
