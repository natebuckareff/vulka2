use std::{
    cell::OnceCell,
    cmp::Ordering,
    collections::HashSet,
    ffi::{CStr, CString, c_void},
    sync::Arc,
};

use anyhow::{Result, anyhow};
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;
use vulkanalia::window::{create_surface, get_required_instance_extensions};
use vulkanalia::{Version, vk};
use winit::window::Window;

use crate::gpu_v2::{Device, DeviceBuilder, QueueFamilyId, QueueRoleFlags, ValidationLayers};

const MIN_API_VERSION: Version = Version::V1_3_0;

/// The Vulkan SDK version that started requiring the portability subset extension for macOS.
const PORTABILITY_MACOS_VERSION: vulkanalia::Version = vulkanalia::Version::new(1, 3, 216);

pub struct EngineParams {
    pub application_name: Option<String>,
    pub application_version: Option<u32>,
    pub enable_validation_layers: Option<ValidationFeatures>,
    pub debug_message_types: Option<vk::DebugUtilsMessageTypeFlagsEXT>,
    pub window: Option<Arc<Window>>,
}

#[derive(Default, Clone, Copy)]
pub struct ValidationFeatures {
    pub best_practices: bool,
    pub debug_printf: bool,
    pub gpu_assisted: bool,
    pub synchronization_validation: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceProfile {
    pub kind: Option<DeviceKind>,
    pub roles: QueueRoleFlags,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceKind {
    Integrated,
    Discrete,
}

#[derive(Debug)]
pub struct DeviceId(usize);

impl Into<usize> for DeviceId {
    fn into(self) -> usize {
        self.0
    }
}

impl From<usize> for DeviceId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

// TODO: rename to DeviceConfig???
#[derive(Debug)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub score: f32,
    pub name: String,
    pub kind: Option<DeviceKind>,
    pub families: Vec<QueueFamilyInfo>,
}

#[derive(Debug)]
pub struct QueueFamilyInfo {
    pub id: QueueFamilyId,
    pub count: u32,
    pub roles: QueueRoleFlags,
}

// pub struct DeviceBuilder {
//     pub id: Option<DeviceId>,
//     pub queues: Vec<(QueueFamilyId, u32)>,
// }

// impl DeviceBuilder {
//     pub fn new() -> Self {
//         Self {
//             id: None,
//             queues: Vec::new(),
//         }
//     }

//     pub fn id(mut self, id: DeviceId) -> Self {
//         self.id = Some(id);
//         self
//     }

//     pub fn queue(mut self, family: QueueFamilyId, count: u32) -> Self {
//         if !self.queues.iter().any(|q| q.0 == family) {
//             self.queues.push((family, count));
//         }
//         self
//     }
// }

pub struct Engine {
    entry: vulkanalia::Entry,
    instance: vulkanalia::Instance,
    surface: Option<vk::SurfaceKHR>,
    physical_devices: OnceCell<Vec<vk::PhysicalDevice>>,
}

impl Engine {
    pub fn new(params: EngineParams) -> Result<Arc<Self>> {
        let entry = Self::load_library()?;
        let (instance, surface) = Self::create_instance(&entry, &params)?;
        let engine = Self {
            entry,
            instance,
            surface,
            physical_devices: OnceCell::new(),
        };
        Ok(Arc::new(engine))
    }

    fn load_library() -> Result<vulkanalia::Entry> {
        use vulkanalia::Entry;
        use vulkanalia::loader::LIBRARY;
        use vulkanalia::loader::LibloadingLoader;
        use vulkanalia::prelude::v1_1::*;

        let loader = unsafe { LibloadingLoader::new(LIBRARY)? };
        let entry = unsafe { Entry::new(loader).map_err(|e| anyhow::anyhow!("{}", e))? };
        let version = unsafe { entry.enumerate_instance_version()? };

        if !is_version_compatible(version) {
            let (major, minor, patch) = (
                vk::version_major(version),
                vk::version_minor(version),
                vk::version_patch(version),
            );
            eprintln!("vulkan version too old: {}.{}.{}", major, minor, patch);
            return Err(anyhow!("vulkan 1.3 or newer is required"));
        }

        Ok(entry)
    }

    fn create_instance(
        entry: &vulkanalia::Entry,
        params: &EngineParams,
    ) -> Result<(vulkanalia::Instance, Option<vk::SurfaceKHR>)> {
        use vulkanalia::prelude::v1_0::*;

        let mut validation_layers = ValidationLayers::new(entry);
        let mut required_extensions = vec![];

        if let Some(window) = &params.window {
            let window_extensions = get_required_instance_extensions(window)
                .iter()
                .map(|ext| **ext);

            required_extensions.push(vk::KHR_SURFACE_EXTENSION.name);
            required_extensions.extend(window_extensions);
        }

        if let Some(validation_features) = &params.enable_validation_layers {
            validation_layers.enable_features(*validation_features)?;
            validation_layers.enable_extensions(&[vk::EXT_DEBUG_UTILS_EXTENSION])?;
        }

        // Required by Vulkan SDK on macOS since 1.3.216.
        let flags = if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
            println!("enabling extensions for macos portability");
            required_extensions.push(vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION.name);
            required_extensions.push(vk::KHR_PORTABILITY_ENUMERATION_EXTENSION.name);
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::empty()
        };

        let supported_instance_extensions =
            unsafe { entry.enumerate_instance_extension_properties(None)? }
                .into_iter()
                .map(|ext| ext.extension_name)
                .collect::<HashSet<_>>();

        let missing_extensions: Vec<_> = required_extensions
            .iter()
            .copied()
            .filter(|ext| !supported_instance_extensions.contains(ext))
            .collect();

        if !missing_extensions.is_empty() {
            for ext in missing_extensions {
                eprintln!("not supported: {}", ext);
            }
            return Err(anyhow!(
                "some required instance extensions are not supported",
            ));
        }

        let mut all_extensions = vec![];
        all_extensions.extend(required_extensions.iter().map(|ext| ext.as_ptr()));
        all_extensions.extend(validation_layers.get_layer_extensions()?);

        let application_name = params
            .application_name
            .as_ref()
            .map(|name| CString::new(name.as_str()))
            .transpose()?
            .unwrap_or_default();

        let application_version = params.application_version.unwrap_or(0);

        let application_info = vk::ApplicationInfo::builder()
            .api_version(vk::make_version(1, 3, 0))
            .application_name(application_name.as_bytes_with_nul())
            .application_version(application_version)
            .engine_name(b"vulka\0")
            .engine_version(vk::make_version(0, 0, 1));

        let mut validation_features = validation_layers.get_validation_features();

        let mut instance_info = vk::InstanceCreateInfo::builder()
            .application_info(&application_info)
            .enabled_extension_names(&all_extensions)
            .enabled_layer_names(&validation_layers.layer_names())
            .flags(flags)
            .push_next(&mut validation_features);

        let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
            .message_type(params.debug_message_types.unwrap_or_else(|| {
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
            }))
            .user_callback(Some(debug_callback));

        if params.enable_validation_layers.is_some() {
            instance_info = instance_info.push_next(&mut debug_info);
        }

        let instance = unsafe { entry.create_instance(&instance_info, None)? };

        let surface = if let Some(window) = &params.window {
            let surface = unsafe { create_surface(&instance, window, window)? };
            Some(surface)
        } else {
            None
        };

        Ok((instance, surface))
    }

    fn get_physical_devices_mut(&self) -> Result<&Vec<vk::PhysicalDevice>> {
        use vulkanalia::prelude::v1_0::*;
        self.physical_devices.get_or_try_init(|| {
            let physical_devices = unsafe { self.instance.enumerate_physical_devices()? };
            Ok(physical_devices)
        })
    }

    pub fn get_devices(&self, profile: DeviceProfile) -> Result<Vec<DeviceInfo>> {
        use vulkanalia::prelude::v1_0::*;

        let physical_devices = self.get_physical_devices_mut()?;
        let mut infos = Vec::with_capacity(physical_devices.len());

        for (i, physical_device) in physical_devices.iter().enumerate() {
            let device_properties = unsafe {
                self.instance
                    .get_physical_device_properties(*physical_device)
            };

            println!("name: {}", device_properties.device_name);

            let raw_family_properties = unsafe {
                self.instance
                    .get_physical_device_queue_family_properties(*physical_device)
            };

            let mut family_properties = Vec::with_capacity(raw_family_properties.len());

            for (i, properties) in raw_family_properties.into_iter().enumerate() {
                let supports_surface = if let Some(surface) = self.surface {
                    let supports = unsafe {
                        self.instance.get_physical_device_surface_support_khr(
                            *physical_device,
                            i as u32,
                            surface,
                        )?
                    };
                    Some(supports)
                } else {
                    None
                };
                println!(
                    " [{}]: count={} present={:?} flags={:?}",
                    i, properties.queue_count, supports_surface, properties.queue_flags
                );
                family_properties.push(QueueFamilyPropertyInfo {
                    properties,
                    supports_surface,
                });
            }

            if let Some(info) = score_device(i, &profile, device_properties, family_properties) {
                infos.push(info);
            }

            println!();
        }

        infos.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        Ok(infos)
    }

    pub fn get_best_device(&self, profile: DeviceProfile) -> Result<Option<DeviceInfo>> {
        let mut infos = Vec::from(self.get_devices(profile)?);
        infos.truncate(1);
        Ok(infos.pop())
    }

    pub fn device(self: &Arc<Self>, info: DeviceInfo) -> DeviceBuilder {
        DeviceBuilder::new(self.clone(), info)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            if let Some(surface) = self.surface.take() {
                self.instance.destroy_surface_khr(surface, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

struct QueueFamilyPropertyInfo {
    properties: vk::QueueFamilyProperties,
    supports_surface: Option<bool>,
}

fn score_device(
    index: usize,
    profile: &DeviceProfile,
    device_properties: vk::PhysicalDeviceProperties,
    family_properties: Vec<QueueFamilyPropertyInfo>,
) -> Option<DeviceInfo> {
    if !is_version_compatible(device_properties.api_version) {
        return None;
    }

    let kind = if device_properties.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU {
        Some(DeviceKind::Integrated)
    } else if device_properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
        Some(DeviceKind::Discrete)
    } else {
        None
    };

    if let Some(profile_kind) = &profile.kind {
        if let Some(device_kind) = &kind {
            if *profile_kind != *device_kind {
                // device kind does not match requested device kind
                return None;
            }
        } else {
            // specific device kind was requested, but device is of unknown kind
            return None;
        }
    };

    let mut device_info = DeviceInfo {
        id: DeviceId::from(index),
        score: 0.0,
        name: device_properties.device_name.to_string(),
        kind,
        families: Vec::with_capacity(family_properties.len()),
    };

    let mut supported_roles = QueueRoleFlags::empty();

    for (i, family_info) in family_properties.iter().enumerate() {
        let properties = family_info.properties;
        let mut queue_flags: QueueRoleFlags = properties.queue_flags.into();

        if family_info.supports_surface == Some(true) {
            queue_flags |= QueueRoleFlags::PRESENT;
        }

        supported_roles |= queue_flags;

        let intersection = queue_flags.intersection(profile.roles);

        if intersection.is_empty() {
            // this queue family does not support *any* of the requested queue
            // roles
            continue;
        }

        let score = intersection.bits().count_ones() * family_info.properties.queue_count;
        device_info.score += score as f32;

        device_info.families.push(QueueFamilyInfo {
            id: QueueFamilyId::from(i),
            count: properties.queue_count,
            roles: queue_flags,
        });
    }

    if !profile.roles.difference(supported_roles).is_empty() {
        // one or more requested roles was not supported by the device
        return None;
    }

    Some(device_info)
}

fn is_version_compatible(version: u32) -> bool {
    let (major, minor, patch) = (
        vk::version_major(version),
        vk::version_minor(version),
        vk::version_patch(version),
    );

    if MIN_API_VERSION.major != major {
        return MIN_API_VERSION.major <= major;
    }

    if MIN_API_VERSION.minor != minor {
        return MIN_API_VERSION.minor <= minor;
    }

    MIN_API_VERSION.patch <= patch
}

/// Logs debug messages.
extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    let data = unsafe { *data };
    let message = unsafe { CStr::from_ptr(data.message) }.to_string_lossy();

    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        eprintln!("ERROR ({:?})\n  {}\n", type_, message);
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        eprintln!("WARNING ({:?})\n  {}\n", type_, message);
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        // eprintln!("INFO ({:?}) {}", type_, message);
    } else {
        // eprintln!("TRACE ({:?}) {}", type_, message);
    }

    vk::FALSE
}
