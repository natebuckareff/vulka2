use std::sync::Arc;

use anyhow::{Result, bail};

use crate::gal::{
    Engine, Surface,
    device::{Device, DeviceResource},
    device_solver::{Allocation, get_ranked_devices},
    device_vk::{create_device, get_device_infos, load_queues},
    queue::{Lane, LaneIndex, Queue, QueueCapFlags, QueueFamily},
    semaphore_resource::SemaphoreResource,
};

pub struct DeviceBuilder {
    engine: Arc<Engine>,
    request: DeviceRequest,
}

impl DeviceBuilder {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self {
            engine,
            request: DeviceRequest::default(),
        }
    }

    pub fn name(mut self, name: String) -> Self {
        self.request.name = Some(name);
        self
    }

    pub fn kind(mut self, kind: DeviceKind) -> Self {
        self.request.kind = Some(kind);
        self
    }

    pub fn allocate_queue(mut self, request: &mut QueueRequest) -> Result<Self> {
        if request.index.is_some() {
            bail!("queue request already added to device builder")
        }
        request.index = Some(self.request.queues.len());
        self.request.queues.push(request.kind);
        Ok(self)
    }

    pub fn present(mut self, surface: Arc<Surface>) -> Self {
        self.request.surface = Some(surface);
        self
    }

    pub fn build(self) -> Result<(Vec<Queue>, Device)> {
        if self.request.queues.is_empty() {
            bail!("at least one queue must be requested before building a device");
        }
        let surface = self.request.surface.as_deref();
        let infos = get_device_infos(&self.engine, surface)?;
        let ranked = get_ranked_devices(&self.request, &infos);
        let Some(best) = ranked.into_iter().next() else {
            bail!("no device found matching request")
        };
        let (info, solution) = best.into_parts();
        let plan = BuildPlan::new(solution.allocations());
        let resource = Arc::new(create_device(
            &self.engine,
            info.physical_device,
            surface,
            &plan.family_counts,
        )?);
        let queue_handles = plan
            .queues
            .iter()
            .map(|queue| (queue.family, queue.queue))
            .collect::<Vec<_>>();
        let queue_resources = load_queues(resource.as_ref(), &queue_handles)?;
        let queues = build_queues(resource.clone(), queue_resources, &plan)?;
        let device = Device::new(self.engine, resource, &queues);
        Ok((queues, device))
    }
}

struct BuildPlan {
    family_counts: Vec<(QueueFamily, u32)>,
    queues: Vec<Allocation>,
}

impl BuildPlan {
    fn new(allocations: &[Allocation]) -> Self {
        let mut family_counts = Vec::new();

        for allocation in allocations {
            push_family_count(&mut family_counts, allocation.family);
        }

        Self {
            family_counts,
            queues: allocations.to_vec(),
        }
    }
}

fn push_family_count(families: &mut Vec<(QueueFamily, u32)>, family: QueueFamily) {
    if let Some((_, count)) = families.iter_mut().find(|(current, _)| *current == family) {
        *count += 1;
        return;
    }
    families.push((family, 1));
}

fn build_queues(
    device: Arc<DeviceResource>,
    resources: Vec<crate::gal::queue_resource::QueueResource>,
    plan: &BuildPlan,
) -> Result<Vec<Queue>> {
    let mut queues = Vec::with_capacity(resources.len());

    for (index, (resource, plan)) in resources.into_iter().zip(plan.queues.iter()).enumerate() {
        let semaphore = Arc::new(SemaphoreResource::new(device.clone())?);
        let lane = Lane::new(LaneIndex::new(index as u32), resource.family());
        let queue = Queue::new(
            device.clone(),
            semaphore,
            resource,
            plan.request,
            plan.present,
            lane,
        );
        queues.push(queue);
    }

    Ok(queues)
}

#[derive(Default)]
pub struct DeviceRequest {
    pub name: Option<String>,
    pub kind: Option<DeviceKind>,
    pub queues: Vec<QueueKind>,
    pub surface: Option<Arc<Surface>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Discrete,
    Integrated,
}

pub struct QueueRequest {
    kind: QueueKind,
    index: Option<usize>,
}

impl QueueRequest {
    pub fn new(kind: QueueKind) -> Self {
        Self { kind, index: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    Graphics,        // graphics
    GraphicsCompute, // combined
    Compute,         // async compute
    Transfer,        // async transfer
}

impl From<QueueKind> for QueueCapFlags {
    fn from(value: QueueKind) -> Self {
        let cap = match value {
            QueueKind::Graphics => QueueCapFlags::GRAPHICS,
            QueueKind::GraphicsCompute => QueueCapFlags::GRAPHICS | QueueCapFlags::COMPUTE,
            QueueKind::Compute => QueueCapFlags::COMPUTE,
            QueueKind::Transfer => QueueCapFlags::TRANSFER,
        };
        // graphics and compute queues implicitly support transfer
        cap | QueueCapFlags::TRANSFER
    }
}
