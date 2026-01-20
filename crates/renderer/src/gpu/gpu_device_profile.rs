use std::cell::OnceCell;
use std::collections::HashMap;

use anyhow::{Context, Result};
use vulkanalia::vk;
use vulkanalia::{prelude::v1_3::*, vk::KhrSurfaceExtensionInstanceCommands};

use crate::gpu::{
    DeviceFeatureArray, ExtensionNameArray, ExtensionSupport, FeatureSupport, GpuDeviceRequest,
    GpuQueueRequest,
};

pub enum GpuDeviceProfileResult {
    Fulfilled(GpuDeviceProfile),
    Rejected(GpuDeviceProfileRejection),
}

pub enum GpuDeviceProfileRejection {
    Error(anyhow::Error),
    OldApiVersion(u32),
    Unfulfilled(usize),
}

pub enum GpuQueueFamilyRejection {
    Unsupported,
    CountExceeded,
}

pub struct GpuQueueFamilySelection {
    pub request: usize,
    pub queue_family_index: u32,
    pub priority: f32,
}

pub struct GpuDeviceProfile {
    info: PhysicalDeviceInfo,
    score: i32,
    extensions: ExtensionNameArray,
    features: Option<DeviceFeatureArray>,
    queue_families: HashMap<usize, (u32, f32)>,
    queue_family_rejections: HashMap<(usize, u32, Option<usize>), GpuQueueFamilyRejection>, // TODO: better api
    queue_families_ordered: OnceCell<Vec<GpuQueueFamilySelection>>,
}

impl GpuDeviceProfile {
    pub(crate) fn new(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
        requests: &[GpuDeviceRequest],
    ) -> Result<GpuDeviceProfileResult> {
        use GpuDeviceProfileRejection::*;
        use GpuDeviceProfileResult::*;

        let info = PhysicalDeviceInfo::new(physical_device);
        let mut score = 0;
        let mut extensions = ExtensionNameArray::default();
        let mut features = DeviceFeatureArray::default();
        let mut queue_families = HashMap::new();
        let mut queue_family_counts = HashMap::new();
        let mut queue_family_rejections = HashMap::new();

        for (request_index, request) in requests.iter().enumerate() {
            use GpuDeviceRequest::*;
            match request {
                MinimumApiVersion(api_version) => {
                    let properties = info.properties(instance);
                    if properties.api_version < *api_version {
                        return Ok(Rejected(OldApiVersion(properties.api_version)));
                    }
                }
                IsDiscrete => {
                    let properties = info.properties(instance);
                    if properties.device_type != vk::PhysicalDeviceType::DISCRETE_GPU {
                        return Ok(Rejected(Unfulfilled(request_index)));
                    }
                }
                RequiredExtension(extension) => {
                    let extension_support = info.extensions_support(instance, requests)?;
                    if !extension_support.supported.contains(extension) {
                        return Ok(Rejected(Unfulfilled(request_index)));
                    }
                    extensions.push(*extension);
                }
                OptionalExtension(extension) => {
                    let extension_support = info.extensions_support(instance, requests)?;
                    if extension_support.supported.contains(extension) {
                        score += 1;
                        extensions.push(*extension);
                    }
                }
                RequiredFeature(feature) => {
                    let feature_support = info.features_support(instance, requests);
                    if !feature_support.supported.contains(feature) {
                        return Ok(Rejected(Unfulfilled(request_index)));
                    }
                    features.push(*feature);
                }
                OptionalFeature(feature) => {
                    let feature_support = info.features_support(instance, requests);
                    if feature_support.supported.contains(feature) {
                        score += 1;
                        features.push(*feature);
                    }
                }
                HasQueue(queue_profile) => {
                    let queue_family_properties = info.queue_family_properties(instance);
                    let candidates = queue_family_properties.iter().enumerate();
                    let mut found_match = false;

                    'families: for (queue_family_index, queue_family) in candidates {
                        let queue_family_index = queue_family_index as u32;

                        let queue_requests = queue_profile.requests.iter().enumerate();
                        for (queue_request_index, queue_request) in queue_requests {
                            let supports = Self::supports_queue_request(
                                physical_device,
                                instance,
                                queue_request,
                                queue_family_index,
                                queue_family,
                            )?;

                            if !supports {
                                queue_family_rejections.insert(
                                    (request_index, queue_family_index, Some(queue_request_index)),
                                    GpuQueueFamilyRejection::Unsupported,
                                );
                                continue 'families;
                            }
                        }

                        let allocated = queue_family_counts.entry(queue_family_index).or_insert(0);
                        if *allocated >= queue_family.queue_count {
                            queue_family_rejections.insert(
                                (request_index, queue_family_index, None),
                                GpuQueueFamilyRejection::CountExceeded,
                            );
                            continue 'families;
                        }

                        let pair = (queue_family_index, queue_profile.priority);
                        queue_families.insert(request_index, pair);
                        *allocated += 1;
                        found_match = true;

                        break 'families;
                    }

                    if !found_match {
                        return Ok(Rejected(Unfulfilled(request_index)));
                    }
                }
            }
        }

        let profile = GpuDeviceProfile {
            info,
            score,
            extensions,
            features: Some(features),
            queue_families,
            queue_family_rejections,
            queue_families_ordered: OnceCell::new(),
        };

        Ok(Fulfilled(profile))
    }

    fn supports_queue_request(
        physical_device: vk::PhysicalDevice,
        instance: &Instance,
        queue_request: &GpuQueueRequest,
        queue_family_index: u32,
        queue_family: &vk::QueueFamilyProperties,
    ) -> Result<bool> {
        use GpuQueueRequest::*;
        match queue_request {
            HasGraphics => {
                if !queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    return Ok(false);
                }
            }
            CanPresentTo(surface) => {
                let supports_surface = unsafe {
                    instance
                        .get_physical_device_surface_support_khr(
                            physical_device,
                            queue_family_index,
                            *surface,
                        )
                        .context("failed to check surface support")
                }?;
                if !supports_surface {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn physical_device(&self) -> vk::PhysicalDevice {
        self.info.physical_device
    }

    pub(crate) fn score(&self) -> i32 {
        self.score
    }

    pub(crate) fn extensions(&self) -> &ExtensionNameArray {
        &self.extensions
    }

    pub(crate) fn take_features(&mut self) -> Result<DeviceFeatureArray> {
        self.features.take().context("profile not built")
    }

    pub(crate) fn iter_queue_families(&self) -> impl Iterator<Item = &GpuQueueFamilySelection> {
        self.queue_families_ordered
            .get_or_init(|| {
                let mut queue_families = self
                    .queue_families
                    .iter()
                    .map(
                        |(position, (queue_family_index, priority))| GpuQueueFamilySelection {
                            request: *position,
                            queue_family_index: *queue_family_index,
                            priority: *priority,
                        },
                    )
                    .collect::<Vec<_>>();

                queue_families.sort_by_key(|selection| selection.request);
                queue_families
            })
            .iter()
    }

    pub(crate) fn queue_family_rejections(
        &self,
    ) -> &HashMap<(usize, u32, Option<usize>), GpuQueueFamilyRejection> {
        &self.queue_family_rejections
    }
}

struct PhysicalDeviceInfo {
    physical_device: vk::PhysicalDevice,
    properties: OnceCell<vk::PhysicalDeviceProperties>,
    extension_support: OnceCell<ExtensionSupport>,
    features_support: OnceCell<FeatureSupport>,
    queue_family_properties: OnceCell<Vec<vk::QueueFamilyProperties>>,
}

impl PhysicalDeviceInfo {
    fn new(physical_device: vk::PhysicalDevice) -> Self {
        Self {
            physical_device,
            properties: OnceCell::new(),
            extension_support: OnceCell::new(),
            features_support: OnceCell::new(),
            queue_family_properties: OnceCell::new(),
        }
    }

    fn properties(&self, instance: &Instance) -> &vk::PhysicalDeviceProperties {
        self.properties.get_or_init(|| {
            let mut properties2 = vk::PhysicalDeviceProperties2::default();
            unsafe {
                instance.get_physical_device_properties2(self.physical_device, &mut properties2)
            };
            properties2.properties
        })
    }

    fn extensions_support(
        &self,
        instance: &Instance,
        profiles: &[GpuDeviceRequest],
    ) -> Result<&ExtensionSupport> {
        self.extension_support.get_or_try_init(|| {
            let mut extensions = ExtensionNameArray::default();
            for profile in profiles {
                use GpuDeviceRequest::*;
                match profile {
                    RequiredExtension(extension) => extensions.push(*extension),
                    OptionalExtension(extension) => extensions.push(*extension),
                    _ => {}
                }
            }
            ExtensionSupport::from_device_extensions(instance, self.physical_device, extensions)
        })
    }

    fn features_support(
        &self,
        instance: &Instance,
        profiles: &[GpuDeviceRequest],
    ) -> &FeatureSupport {
        self.features_support.get_or_init(|| {
            let mut features = DeviceFeatureArray::default();
            for profile in profiles {
                use GpuDeviceRequest::*;
                match profile {
                    RequiredFeature(feature) => features.push(*feature),
                    OptionalFeature(feature) => features.push(*feature),
                    _ => {}
                }
            }
            FeatureSupport::from_device_features(instance, self.physical_device, features)
        })
    }

    fn queue_family_properties(&self, instance: &Instance) -> &Vec<vk::QueueFamilyProperties> {
        self.queue_family_properties.get_or_init(|| unsafe {
            instance.get_physical_device_queue_family_properties(self.physical_device)
        })
    }
}
