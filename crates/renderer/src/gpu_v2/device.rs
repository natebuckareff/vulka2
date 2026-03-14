use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk;

use crate::gpu_v2::{
    CommandAllocator, DeviceId, DeviceInfo, Engine, FrameAllocator, LaneKey, LaneVec,
    LaneVecBuilder, OwnedDevice, OwnedSemaphore, Queue, QueueAllocation, QueueBinding, QueueFamily,
    QueueFamilyId, QueueGroup, QueueGroupBuilder, QueueGroupId, QueueGroupInfo, QueueGroupTable,
    QueueId, QueueRoleFlags, ResourceArena, SettledLanes, SubmissionProgress, VulkanHandle,
    get_available_families, select_best_families,
};

pub(crate) struct DevicePlan {
    info: DeviceInfo,
    reservations: BTreeMap<QueueFamilyId, u32>,
    allocations: BTreeMap<QueueGroupId, Vec<QueueAllocation>>,
}

pub struct DeviceBuilder {
    engine: Arc<Engine>,
    info: DeviceInfo,
    reservations: BTreeMap<QueueFamilyId, u32>,
    queue_groups: BTreeMap<QueueGroupId, Vec<QueueAllocation>>,
    next_queue_group_id: u32,
}

impl DeviceBuilder {
    pub(crate) fn new(engine: Arc<Engine>, info: DeviceInfo) -> Self {
        Self {
            engine,
            info,
            reservations: BTreeMap::new(),
            queue_groups: BTreeMap::new(),
            next_queue_group_id: 0,
        }
    }

    fn available_families(&self) -> Result<Vec<QueueFamily>> {
        get_available_families(&self.info.families, &self.reservations)
    }

    fn next_queue_group_id(&mut self) -> Result<QueueGroupId> {
        let id = self.next_queue_group_id;
        self.next_queue_group_id = self
            .next_queue_group_id
            .checked_add(1)
            .expect("queue group id overflow");
        QueueGroupId::try_from(id)
    }

    pub(crate) fn allocate_group(
        &mut self,
        queue_group_id: QueueGroupId,
        roles: QueueRoleFlags,
    ) -> Result<Option<QueueGroupId>> {
        if roles.is_empty() {
            return Ok(None);
        }

        if self.queue_groups.contains_key(&queue_group_id) {
            return Err(anyhow!("duplicate queue group id: {:?}", queue_group_id));
        }

        let available_families = self.available_families()?;
        let selected_families = select_best_families(&available_families, roles);
        if selected_families.is_empty() {
            return Ok(None);
        }

        let mut allocations = selected_families
            .into_iter()
            .map(|family| self.allocate_queue(family))
            .collect::<Result<Vec<_>>>()?;

        // sort allocations by queue family; core assumption here is that queues
        // in a queue group are all from different queue families, and this
        // order carries through to all lane usage
        allocations.sort_by_key(|allocation| allocation.queue_id.family);

        self.queue_groups.insert(queue_group_id, allocations);

        Ok(Some(queue_group_id))
    }

    fn allocate_queue(&mut self, family: QueueFamily) -> Result<QueueAllocation> {
        let id = family.id;
        let family = self.info.families.get(&id).context("family not found")?;
        let reserved = self.reservations.get(&id).copied().unwrap_or(0);

        if reserved >= family.count {
            let id: u32 = id.into();
            return Err(anyhow!("not enough queues available in family {}", id));
        }

        let queue_id = QueueId {
            family: id,
            index: reserved,
        };

        self.reservations.insert(id, reserved + 1);

        Ok(QueueAllocation {
            queue_id,
            roles: family.roles,
        })
    }

    pub fn queue_group(&'_ mut self) -> Result<QueueGroupBuilder<'_>> {
        let id = self.next_queue_group_id()?;
        Ok(QueueGroupBuilder::new(self, id))
    }

    pub fn build(self) -> Result<(Arc<Device>, FrameAllocator)> {
        if self.reservations.is_empty() {
            return Err(anyhow!(
                "at least one queue group must be created before building the device"
            ));
        }

        let plan = DevicePlan {
            info: self.info,
            reservations: self.reservations,
            allocations: self.queue_groups,
        };

        Device::new(self.engine, plan)
            .map(|(device, frame_allocator)| (Arc::new(device), frame_allocator))
    }
}

pub struct Device {
    engine: Arc<Engine>,
    info: DeviceInfo,
    arena: ResourceArena,
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    queues: HashMap<QueueId, vk::Queue>,
    queue_groups: Mutex<HashMap<QueueGroupId, QueueGroup>>,
    queue_group_table: QueueGroupTable,
    next_child_id: AtomicUsize,
    settled: Arc<SettledLanes>,
}

struct BuiltQueueLane {
    allocation: QueueAllocation,
    key: LaneKey,
    queue: vk::Queue,
    semaphore: VulkanHandle<vk::Semaphore>,
}

impl Device {
    pub(crate) fn new(engine: Arc<Engine>, plan: DevicePlan) -> Result<(Self, FrameAllocator)> {
        let physical_device = plan.info.physical_device;
        let required_extensions = required_device_extensions(&engine);

        validate_required_extensions(engine.instance(), physical_device, &required_extensions)?;

        let arena = ResourceArena::new("device");
        let device = create_vk_device(
            engine.instance(),
            &arena,
            physical_device,
            &plan.reservations,
            &required_extensions,
        )?;

        let queues = load_queues(&device, &plan.reservations)?;
        let (built_groups, queue_group_table) =
            build_queue_group_layout(device.clone(), &arena, &queues, &plan.allocations)?;

        let settled = Arc::new(SettledLanes::new(&queue_group_table));
        let queue_groups =
            build_queue_groups(device.clone(), plan.info.id, built_groups, settled.clone())?;

        let progress = {
            let device_id = plan.info.id;
            let device = device.clone();
            let queue_groups = queue_group_table.clone();
            SubmissionProgress::new(device_id, device, queue_groups)?
        };
        let frame_allocator = FrameAllocator::new(settled.clone(), progress);

        let device = Self {
            engine,
            info: plan.info,
            arena,
            device,
            queues,
            queue_groups: Mutex::new(queue_groups),
            queue_group_table,
            next_child_id: AtomicUsize::new(0),
            settled,
        };

        Ok((device, frame_allocator))
    }

    pub(crate) fn handle(&self) -> &VulkanHandle<Arc<vulkanalia::Device>> {
        &self.device
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    pub(crate) fn queue_group_table(&self) -> &QueueGroupTable {
        &self.queue_group_table
    }

    pub(crate) fn is_lane_device_complete(&self, key: LaneKey, frame: u64) -> bool {
        self.settled.is_device_complete(key, frame)
    }

    pub fn take_queue_group(&self, id: QueueGroupId) -> Result<Option<QueueGroup>> {
        let mut queue_groups = self
            .queue_groups
            .lock()
            .map_err(|_| anyhow!("queue group state lock poisoned"))?;

        Ok(queue_groups.remove(&id))
    }

    // TODO XXX: factory API is really all over the place right now
    pub fn command_allocator(
        self: &Arc<Self>,
        queue_group_id: QueueGroupId,
        capacity: usize,
    ) -> Result<CommandAllocator> {
        let id = self.next_child_id.fetch_add(1, Ordering::Relaxed);
        let command_allocator = CommandAllocator::new(id, self.clone(), queue_group_id, capacity)?;
        Ok(command_allocator)
    }
}

fn build_queue_group_layout(
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    arena: &ResourceArena,
    queues: &HashMap<QueueId, vk::Queue>,
    queue_group_allocations: &BTreeMap<QueueGroupId, Vec<QueueAllocation>>,
) -> Result<(
    BTreeMap<QueueGroupId, LaneVec<BuiltQueueLane>>,
    QueueGroupTable,
)> {
    use vulkanalia::prelude::v1_0::*;

    let mut claimed_queue_ids = HashSet::new();
    let mut built_groups = BTreeMap::new();
    let mut infos = Vec::with_capacity(queue_group_allocations.len());
    let mut offset = 0u16;

    for (queue_group_id, allocations) in queue_group_allocations {
        if allocations.is_empty() {
            return Err(anyhow!(
                "queue group {:?} has no queue allocations",
                queue_group_id
            ));
        }

        for allocation in allocations {
            if !queues.contains_key(&allocation.queue_id) {
                return Err(anyhow!(
                    "queue {:?} was not created on the vulkan device",
                    allocation.queue_id
                ));
            }

            if !claimed_queue_ids.insert(allocation.queue_id) {
                return Err(anyhow!(
                    "queue {:?} is already assigned to another queue group",
                    allocation.queue_id
                ));
            }
        }

        let mut lanes = LaneVecBuilder::new(*queue_group_id, allocations.len());
        let mut bindings = LaneVecBuilder::new(*queue_group_id, allocations.len());
        let mut lane_index = 0usize;
        for allocation in allocations {
            let queue = *queues
                .get(&allocation.queue_id)
                .expect("validated in previous loop");
            let mut type_info = vk::SemaphoreTypeCreateInfo::builder()
                .semaphore_type(vk::SemaphoreType::TIMELINE)
                .initial_value(0);

            let create_info = vk::SemaphoreCreateInfo::builder().push_next(&mut type_info);

            let semaphore = OwnedSemaphore::new(device.clone(), &create_info)?;
            let semaphore = arena.add(semaphore)?;
            let key = LaneKey {
                offset,
                group: (*queue_group_id).into(),
                index: lane_index as u8,
            };

            bindings.push(QueueBinding {
                id: allocation.queue_id,
                key,
                roles: allocation.roles,
                semaphore: semaphore.clone(),
            });

            lanes.push(BuiltQueueLane {
                allocation: *allocation,
                key,
                queue,
                semaphore,
            });
            lane_index += 1;
        }

        let lane_count = lane_index as u16;
        let bindings = bindings.build();
        infos.push(QueueGroupInfo {
            id: *queue_group_id,
            offset,
            bindings,
        });
        offset += lane_count;
        built_groups.insert(*queue_group_id, lanes.build());
    }

    let table = QueueGroupTable::new(infos);
    Ok((built_groups, table))
}

fn build_queue_groups(
    device: VulkanHandle<Arc<vulkanalia::Device>>,
    device_id: DeviceId,
    built_groups: BTreeMap<QueueGroupId, LaneVec<BuiltQueueLane>>,
    settled: Arc<SettledLanes>,
) -> Result<HashMap<QueueGroupId, QueueGroup>> {
    let mut queue_groups = HashMap::with_capacity(built_groups.len());

    for (queue_group_id, lanes) in built_groups {
        let mut group_queues = LaneVecBuilder::with_lanes(&lanes);
        for (key, lane) in lanes.into_entries() {
            group_queues.push(Queue::new(
                device.clone(),
                lane.allocation.queue_id,
                key,
                lane.allocation.roles,
                lane.queue,
                settled.clone(),
                lane.semaphore,
            )?);
        }

        queue_groups.insert(
            queue_group_id,
            QueueGroup::new(
                device.clone(),
                device_id,
                queue_group_id,
                group_queues.build(),
            ),
        );
    }

    Ok(queue_groups)
}

fn required_device_extensions(engine: &Engine) -> Vec<vk::ExtensionName> {
    let mut extensions = Vec::new();
    if engine.has_surface() {
        extensions.push(vk::KHR_SWAPCHAIN_EXTENSION.name);
    }
    extensions
}

fn validate_required_extensions(
    instance: &VulkanHandle<Arc<vulkanalia::Instance>>,
    physical_device: vk::PhysicalDevice,
    required_extensions: &[vk::ExtensionName],
) -> Result<()> {
    use vulkanalia::prelude::v1_0::*;

    if required_extensions.is_empty() {
        return Ok(());
    }

    let supported_extensions = unsafe {
        instance
            .raw()
            .enumerate_device_extension_properties(physical_device, None)
            .context("failed to enumerate device extension properties")
    }?
    .into_iter()
    .map(|properties| properties.extension_name)
    .collect::<HashSet<_>>();

    let missing_extensions = required_extensions
        .iter()
        .copied()
        .filter(|required| !supported_extensions.contains(required))
        .collect::<Vec<_>>();

    if missing_extensions.is_empty() {
        return Ok(());
    }

    let missing_list = missing_extensions
        .iter()
        .map(|extension| extension.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    Err(anyhow!(
        "required device extensions are not supported: {}",
        missing_list
    ))
}

fn create_vk_device(
    instance: &VulkanHandle<Arc<vulkanalia::Instance>>,
    arena: &ResourceArena,
    physical_device: vk::PhysicalDevice,
    reservations: &BTreeMap<QueueFamilyId, u32>,
    required_extensions: &[vk::ExtensionName],
) -> Result<VulkanHandle<Arc<vulkanalia::Device>>> {
    use vulkanalia::prelude::v1_1::*;

    let queue_priorities = reservations
        .values()
        .map(|count| vec![1.0; *count as usize])
        .collect::<Vec<_>>();

    let queue_create_infos = reservations
        .iter()
        .zip(queue_priorities.iter())
        .map(|((family_id, _), priorities)| {
            let family_index: u32 = (*family_id).into();
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(family_index)
                .queue_priorities(priorities)
        })
        .collect::<Vec<_>>();

    let extension_ptrs = required_extensions
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();

    let mut supported_v12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut supported_v13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut supported_features = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut supported_v12)
        .push_next(&mut supported_v13)
        .build();

    unsafe {
        instance
            .raw()
            .get_physical_device_features2(physical_device, &mut supported_features)
    };

    let mut missing_features = Vec::new();
    if supported_v12.timeline_semaphore != vk::TRUE {
        missing_features.push("timelineSemaphore");
    }
    if supported_v13.synchronization2 != vk::TRUE {
        missing_features.push("synchronization2");
    }
    if !missing_features.is_empty() {
        return Err(anyhow!(
            "required device features are not supported: {}",
            missing_features.join(", ")
        ));
    }

    let mut enabled_v12 = vk::PhysicalDeviceVulkan12Features::default();
    enabled_v12.timeline_semaphore = vk::TRUE;

    let mut enabled_v13 = vk::PhysicalDeviceVulkan13Features::default();
    enabled_v13.synchronization2 = vk::TRUE;

    let device_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_ptrs)
        .push_next(&mut enabled_v12)
        .push_next(&mut enabled_v13);

    let device = OwnedDevice::new(instance.clone(), physical_device, &device_info)?;
    let device = arena.add(device)?;
    Ok(device)
}

fn load_queues(
    vk_device: &VulkanHandle<Arc<vulkanalia::Device>>,
    queue_family_counts: &BTreeMap<QueueFamilyId, u32>,
) -> Result<HashMap<QueueId, vk::Queue>> {
    use vulkanalia::prelude::v1_0::*;

    let mut queues = HashMap::new();

    for (family_id, count) in queue_family_counts {
        let family_index: u32 = (*family_id).into();
        for queue_index in 0..*count {
            let queue_id = QueueId {
                family: *family_id,
                index: queue_index,
            };
            let queue = unsafe { vk_device.raw().get_device_queue(family_index, queue_index) };
            queues.insert(queue_id, queue);
        }
    }

    Ok(queues)
}
