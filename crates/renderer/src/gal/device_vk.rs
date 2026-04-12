use std::collections::HashSet;

use anyhow::Result;
use anyhow::anyhow;
use vulkanalia::Version;
use vulkanalia::vk;

use crate::gal::Engine;
use crate::gal::Surface;
use crate::gal::device::DeviceResource;
use crate::gal::device_builder::DeviceKind;
use crate::gal::device_info::DeviceInfo;
use crate::gal::device_info::QueueFamilyInfo;
use crate::gal::queue::QueueCapFlags;
use crate::gal::queue::QueueFamily;
use crate::gal::queue_resource::QueueResource;

const MIN_API_VERSION: Version = Version::V1_3_0;

pub(crate) fn get_device_infos(
    engine: &Engine,
    surface: Option<&Surface>,
) -> Result<Vec<DeviceInfo>> {
    use vulkanalia::prelude::v1_0::*;

    let instance = unsafe { engine.instance() };
    let physical_devices = unsafe { instance.enumerate_physical_devices()? };
    let required_extensions = required_device_extensions(surface);
    let mut infos = Vec::with_capacity(physical_devices.len());

    for physical_device in physical_devices {
        if !is_physical_device_compatible(instance, physical_device, &required_extensions)? {
            continue;
        }

        let info = query_device_info(instance, physical_device, surface)?;
        infos.push(info);
    }

    Ok(infos)
}

pub(crate) fn ensure_required_device_extensions_supported(
    instance: &vulkanalia::Instance,
    physical_device: vk::PhysicalDevice,
    surface: Option<&Surface>,
) -> Result<()> {
    let extensions = required_device_extensions(surface);
    ensure_device_extensions_supported(instance, physical_device, &extensions)
}

pub(crate) fn ensure_required_device_features_supported(
    instance: &vulkanalia::Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<()> {
    ensure_device_features_supported(instance, physical_device)
}

pub(crate) fn create_device(
    instance: &vulkanalia::Instance,
    physical_device: vk::PhysicalDevice,
    surface: Option<&Surface>,
    queue_families: &[(QueueFamily, u32)],
) -> Result<DeviceResource> {
    use vulkanalia::prelude::v1_1::*;

    ensure_required_device_extensions_supported(instance, physical_device, surface)?;
    ensure_required_device_features_supported(instance, physical_device)?;

    let queue_priorities = queue_families
        .iter()
        .map(|(_, count)| vec![1.0; *count as usize])
        .collect::<Vec<_>>();
    let queue_create_infos = queue_families
        .iter()
        .zip(queue_priorities.iter())
        .map(|((family, _), priorities)| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(u32::from(*family))
                .queue_priorities(priorities)
        })
        .collect::<Vec<_>>();

    let extensions = required_device_extensions(surface);
    let extension_ptrs = extensions
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();

    let mut enabled_v12 = vk::PhysicalDeviceVulkan12Features::default();
    enabled_v12.timeline_semaphore = vk::TRUE;

    let mut enabled_v13 = vk::PhysicalDeviceVulkan13Features::default();
    enabled_v13.synchronization2 = vk::TRUE;

    let create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_ptrs)
        .push_next(&mut enabled_v12)
        .push_next(&mut enabled_v13);

    let handle = unsafe { instance.create_device(physical_device, &create_info, None)? };
    Ok(DeviceResource::new(handle))
}

pub(crate) fn load_queues(
    device: &DeviceResource,
    queues: &[(QueueFamily, u32)],
) -> Result<Vec<QueueResource>> {
    queues
        .iter()
        .map(|(family, id)| QueueResource::new(device, *family, *id))
        .collect()
}

fn query_device_info(
    instance: &vulkanalia::Instance,
    physical_device: vk::PhysicalDevice,
    surface: Option<&Surface>,
) -> Result<DeviceInfo> {
    use vulkanalia::prelude::v1_0::*;
    use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;

    let properties = unsafe { instance.get_physical_device_properties(physical_device) };
    let family_properties =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

    let surface_handle = surface.map(|surface| unsafe { surface.handle() });
    let mut families = Vec::with_capacity(family_properties.len());

    for (family_index, properties) in family_properties.into_iter().enumerate() {
        let family = QueueFamily::from(family_index as u32);
        let flags = QueueCapFlags::from(properties.queue_flags);
        let present = if let Some(surface) = surface_handle {
            unsafe {
                instance.get_physical_device_surface_support_khr(
                    physical_device,
                    family_index as u32,
                    surface,
                )?
            }
        } else {
            false
        };

        families.push(QueueFamilyInfo::new(
            family,
            properties.queue_count,
            flags,
            present,
        ));
    }

    Ok(DeviceInfo {
        physical_device,
        name: properties.device_name.to_string(),
        kind: device_kind(properties.device_type),
        families,
    })
}

fn is_physical_device_compatible(
    instance: &vulkanalia::Instance,
    physical_device: vk::PhysicalDevice,
    required_extensions: &[vk::ExtensionName],
) -> Result<bool> {
    use vulkanalia::prelude::v1_0::*;

    let properties = unsafe { instance.get_physical_device_properties(physical_device) };
    if !is_version_compatible(properties.api_version) {
        return Ok(false);
    }

    if ensure_device_extensions_supported(instance, physical_device, required_extensions).is_err() {
        return Ok(false);
    }

    if ensure_device_features_supported(instance, physical_device).is_err() {
        return Ok(false);
    }

    Ok(true)
}

fn required_device_extensions(surface: Option<&Surface>) -> Vec<vk::ExtensionName> {
    let mut extensions = Vec::new();
    if surface.is_some() {
        extensions.push(vk::KHR_SWAPCHAIN_EXTENSION.name);
    }
    extensions
}

fn ensure_device_extensions_supported(
    instance: &vulkanalia::Instance,
    physical_device: vk::PhysicalDevice,
    required_extensions: &[vk::ExtensionName],
) -> Result<()> {
    use vulkanalia::prelude::v1_0::*;

    if required_extensions.is_empty() {
        return Ok(());
    }

    let supported_extensions =
        unsafe { instance.enumerate_device_extension_properties(physical_device, None)? }
            .into_iter()
            .map(|properties| properties.extension_name)
            .collect::<HashSet<_>>();

    let missing_extensions = required_extensions
        .iter()
        .copied()
        .filter(|extension| !supported_extensions.contains(extension))
        .collect::<Vec<_>>();

    if missing_extensions.is_empty() {
        return Ok(());
    }

    let missing = missing_extensions
        .into_iter()
        .map(|extension| extension.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    Err(anyhow!(
        "required device extensions are not supported: {missing}"
    ))
}

fn ensure_device_features_supported(
    instance: &vulkanalia::Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<()> {
    use vulkanalia::prelude::v1_1::*;

    let mut supported_v12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut supported_v13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut supported_features = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut supported_v12)
        .push_next(&mut supported_v13)
        .build();

    unsafe {
        instance.get_physical_device_features2(physical_device, &mut supported_features);
    }

    let mut missing_features = Vec::new();
    if supported_v12.timeline_semaphore != vk::TRUE {
        missing_features.push("timelineSemaphore");
    }
    if supported_v13.synchronization2 != vk::TRUE {
        missing_features.push("synchronization2");
    }

    if missing_features.is_empty() {
        return Ok(());
    }

    Err(anyhow!(
        "required device features are not supported: {}",
        missing_features.join(", ")
    ))
}

fn device_kind(device_type: vk::PhysicalDeviceType) -> Option<DeviceKind> {
    match device_type {
        vk::PhysicalDeviceType::INTEGRATED_GPU => Some(DeviceKind::Integrated),
        vk::PhysicalDeviceType::DISCRETE_GPU => Some(DeviceKind::Discrete),
        _ => None,
    }
}

fn is_version_compatible(version: u32) -> bool {
    let major = vk::version_major(version);
    let minor = vk::version_minor(version);
    let patch = vk::version_patch(version);

    if MIN_API_VERSION.major != major {
        return MIN_API_VERSION.major <= major;
    }

    if MIN_API_VERSION.minor != minor {
        return MIN_API_VERSION.minor <= minor;
    }

    MIN_API_VERSION.patch <= patch
}
