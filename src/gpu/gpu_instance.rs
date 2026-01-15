use std::ffi::CString;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;
use vulkanalia::window::get_required_instance_extensions;
use winit::raw_window_handle::HasWindowHandle;

use crate::gpu::{
    GpuExtensionHandle, GpuExtensionSupport, GpuExtensions, GpuPhysicalDevice,
    GpuPhysicalDeviceCaps,
};

pub enum GpuPhysicalDeviceProfile {
    DiscreteGpu,
    HasGraphicsQueue,
    CanPresentTo(vk::SurfaceKHR),
    RequiresDeviceExtension(GpuExtensionHandle),
}

pub struct GpuInstanceOptions {
    pub application_name: String,
    pub validation_layers: Vec<CString>,
    pub extra_extensions: Arc<GpuExtensions>,
}

impl GpuInstanceOptions {
    pub fn new(application_name: String) -> Self {
        Self {
            application_name,
            validation_layers: Vec::new(),
            extra_extensions: GpuExtensions::empty(),
        }
    }

    pub fn with_validation(mut self) -> Result<Self> {
        self.validation_layers = vec![CString::new("VK_LAYER_KHRONOS_validation")?];
        Ok(self)
    }

    pub fn validation_layers(mut self, layers: Vec<CString>) -> Self {
        self.validation_layers = layers;
        self
    }

    pub fn extra_extensions(mut self, extensions: Arc<GpuExtensions>) -> Self {
        self.extra_extensions = extensions;
        self
    }
}

enum MatchType {
    NoMatch,
    MatchNoCap,
    MatchWithCaps(Vec<GpuPhysicalDeviceCaps>),
}

pub struct GpuInstance {
    instance: Instance,
}

impl GpuInstance {
    pub fn new(
        window: &impl HasWindowHandle,
        entry: &Entry,
        options: GpuInstanceOptions,
    ) -> Result<Arc<Self>> {
        let app_name = CString::new(options.application_name)
            .map_err(|err| anyhow!("invalid application name: {err}"))?;

        let required_extensions = get_required_instance_extensions(window);
        let mut extension_builder = GpuExtensions::builder();
        for extension in required_extensions {
            extension_builder = extension_builder.add(**extension);
        }
        for extension in options.extra_extensions.iter_names() {
            extension_builder = extension_builder.add(extension);
        }
        let extensions = extension_builder.build();

        let layer_names: Vec<*const i8> = options
            .validation_layers
            .iter()
            .map(|layer| layer.as_ptr())
            .collect();

        let app_info = vk::ApplicationInfo::builder()
            .application_name(app_name.as_bytes_with_nul())
            .application_version(0)
            .engine_name(app_name.as_bytes_with_nul())
            .engine_version(0)
            .api_version(vk::make_version(1, 3, 0));

        let instance = extensions.with_ptrs(|extension_names| {
            let instance_info = vk::InstanceCreateInfo::builder()
                .application_info(&app_info)
                .enabled_extension_names(extension_names)
                .enabled_layer_names(&layer_names);

            unsafe { entry.create_instance(&instance_info, None) }
        })
        .context("failed to create Vulkan instance")?;

        Ok(Arc::new(Self { instance }))
    }

    pub fn get_vk_instance(&self) -> &Instance {
        &self.instance
    }

    pub fn for_each_physical_device<F>(&self, f: F) -> Result<()>
    where
        F: FnMut((usize, vk::PhysicalDevice)),
    {
        let devices = unsafe { self.instance.enumerate_physical_devices() }
            .context("failed to enumerate physical devices")?;
        devices.into_iter().enumerate().for_each(f);
        Ok(())
    }

    fn match_profile(
        instance: &Instance,
        extension_support: &GpuExtensionSupport,
        profile: &GpuPhysicalDeviceProfile,
        item: &(usize, vk::PhysicalDevice),
    ) -> MatchType {
        let (_, physical_device) = item;
        match profile {
            GpuPhysicalDeviceProfile::DiscreteGpu => {
                let properties =
                    unsafe { instance.get_physical_device_properties(*physical_device) };
                if properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
                    MatchType::MatchNoCap
                } else {
                    MatchType::NoMatch
                }
            }
            GpuPhysicalDeviceProfile::HasGraphicsQueue => {
                let queue_families = unsafe {
                    instance.get_physical_device_queue_family_properties(*physical_device)
                };
                let caps = queue_families
                    .iter()
                    .enumerate()
                    .filter_map(|(index, queue_family)| {
                        if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                            Some(GpuPhysicalDeviceCaps::Graphics(index as u32))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                if caps.is_empty() {
                    MatchType::NoMatch
                } else {
                    MatchType::MatchWithCaps(caps)
                }
            }
            GpuPhysicalDeviceProfile::CanPresentTo(surface) => {
                let queue_families = unsafe {
                    instance.get_physical_device_queue_family_properties(*physical_device)
                };
                let caps = queue_families
                    .iter()
                    .enumerate()
                    .filter_map(|(index, _)| {
                        let index = index as u32;
                        let supports_surface = unsafe {
                            instance
                                .get_physical_device_surface_support_khr(
                                    *physical_device,
                                    index,
                                    *surface,
                                )
                                .unwrap_or(false)
                        };
                        if supports_surface {
                            Some(GpuPhysicalDeviceCaps::Present(index))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                if caps.is_empty() {
                    MatchType::NoMatch
                } else {
                    MatchType::MatchWithCaps(caps)
                }
            }
            GpuPhysicalDeviceProfile::RequiresDeviceExtension(extension) => {
                if extension_support.is_supported(extension) {
                    MatchType::MatchNoCap
                } else {
                    MatchType::NoMatch
                }
            }
        }
    }

    pub fn find_physical_device(
        self: &Arc<Self>,
        profiles: &[GpuPhysicalDeviceProfile],
    ) -> Result<Option<Arc<GpuPhysicalDevice>>> {
        let requested_extensions = Self::requested_device_extensions(profiles)?;
        let devices = unsafe { self.instance.enumerate_physical_devices() }
            .context("failed to enumerate physical devices")?;
        let instance = self.clone();

        for item in devices.into_iter().enumerate() {
            let mut caps = vec![];
            let extension_support =
                requested_extensions.support_for(&self.instance, item.1)?;
            let mut matched = true;
            for profile in profiles {
                match Self::match_profile(&self.instance, &extension_support, profile, &item) {
                    MatchType::NoMatch => {
                        matched = false;
                        break;
                    }
                    MatchType::MatchNoCap => {}
                    MatchType::MatchWithCaps(found_caps) => caps.extend(found_caps),
                }
            }
            if matched {
                return Ok(Some(Arc::new(GpuPhysicalDevice::new(
                    instance.clone(),
                    item.1,
                    caps,
                    extension_support,
                ))));
            }
        }

        Ok(None)
    }

    pub fn get_all_physical_devices(
        self: &Arc<Self>,
        profiles: &[GpuPhysicalDeviceProfile],
    ) -> Result<Vec<Arc<GpuPhysicalDevice>>> {
        let requested_extensions = Self::requested_device_extensions(profiles)?;
        let devices = unsafe { self.instance.enumerate_physical_devices() }
            .context("failed to enumerate physical devices")?;
        let instance = self.clone();
        let mut results = Vec::with_capacity(devices.len());

        for (index, physical_device) in devices.into_iter().enumerate() {
            let mut caps = vec![];
            let extension_support =
                requested_extensions.support_for(&self.instance, physical_device)?;
            let item = (index, physical_device);
            for profile in profiles {
                match Self::match_profile(&self.instance, &extension_support, profile, &item) {
                    MatchType::NoMatch => {}
                    MatchType::MatchNoCap => {}
                    MatchType::MatchWithCaps(found_caps) => caps.extend(found_caps),
                }
            }

            results.push(Arc::new(GpuPhysicalDevice::new(
                instance.clone(),
                physical_device,
                caps,
                extension_support,
            )));
        }

        Ok(results)
    }

    fn requested_device_extensions(
        profiles: &[GpuPhysicalDeviceProfile],
    ) -> Result<Arc<GpuExtensions>> {
        let mut extensions: Option<Arc<GpuExtensions>> = None;
        for profile in profiles {
            if let GpuPhysicalDeviceProfile::RequiresDeviceExtension(extension) = profile {
                if let Some(existing) = &extensions {
                    if !Arc::ptr_eq(existing, extension.extensions()) {
                        return Err(anyhow!(
                            "device extension profiles must use the same GpuExtensions set"
                        ));
                    }
                } else {
                    extensions = Some(extension.extensions().clone());
                }
            }
        }
        Ok(extensions.unwrap_or_else(GpuExtensions::empty))
    }
}

impl Drop for GpuInstance {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}
