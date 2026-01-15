use std::ffi::CString;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;
use vulkanalia::window::get_required_instance_extensions;
use winit::raw_window_handle::HasWindowHandle;

use crate::gpu::{GpuPhysicalDevice, GpuPhysicalDeviceCaps};

pub enum GpuPhysicalDeviceProfile {
    DiscreteGpu,
    HasGraphicsQueue,
    CanPresentTo(vk::SurfaceKHR),
}

pub struct GpuInstanceOptions {
    pub application_name: String,
    pub validation_layers: Vec<CString>,
    pub extra_extensions: Vec<vk::ExtensionName>,
}

impl GpuInstanceOptions {
    pub fn new(application_name: String) -> Self {
        Self {
            application_name,
            validation_layers: Vec::new(),
            extra_extensions: Vec::new(),
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

    pub fn extra_extensions(mut self, extensions: Vec<vk::ExtensionName>) -> Self {
        self.extra_extensions = extensions;
        self
    }
}

enum MatchType {
    NoMatch,
    MatchNoCap,
    MatchWithCaps(GpuPhysicalDeviceCaps),
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

        let extensions = get_required_instance_extensions(window);
        let mut extension_names: Vec<*const i8> =
            extensions.iter().map(|ext| ext.as_ptr()).collect();

        extension_names.extend(options.extra_extensions.iter().map(|ext| ext.as_ptr()));

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

        let instance_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .enabled_layer_names(&layer_names);

        let instance = unsafe { entry.create_instance(&instance_info, None) }
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
                let queue_family_index =
                    queue_families
                        .iter()
                        .enumerate()
                        .find_map(|(index, queue_family)| {
                            let index = index as u32;
                            if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                                Some(index)
                            } else {
                                None
                            }
                        });

                match queue_family_index {
                    Some(index) => MatchType::MatchWithCaps(GpuPhysicalDeviceCaps::Graphics(index)),
                    None => MatchType::NoMatch,
                }
            }
            GpuPhysicalDeviceProfile::CanPresentTo(surface) => {
                let queue_families = unsafe {
                    instance.get_physical_device_queue_family_properties(*physical_device)
                };
                let queue_family_index =
                    queue_families.iter().enumerate().find_map(|(index, _)| {
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
                        if supports_surface { Some(index) } else { None }
                    });

                match queue_family_index {
                    Some(index) => MatchType::MatchWithCaps(GpuPhysicalDeviceCaps::Present(index)),
                    None => MatchType::NoMatch,
                }
            }
        }
    }

    pub fn find_physical_device(
        self: &Arc<Self>,
        profiles: &[GpuPhysicalDeviceProfile],
    ) -> Result<Option<Arc<GpuPhysicalDevice>>> {
        let devices = unsafe { self.instance.enumerate_physical_devices() }
            .context("failed to enumerate physical devices")?;
        let instance = self.clone();
        let found_and_created = devices
            .into_iter()
            .enumerate()
            .find_map(|item| {
                let mut caps = vec![];
                for profile in profiles {
                    match Self::match_profile(&self.instance, profile, &item) {
                        MatchType::NoMatch => return None,
                        MatchType::MatchNoCap => {}
                        MatchType::MatchWithCaps(cap) => caps.push(cap),
                    }
                }
                Some((item.1, caps))
            })
            .map(|(physical_device, caps)| {
                Arc::new(GpuPhysicalDevice::new(
                    instance.clone(),
                    physical_device,
                    caps,
                ))
            });
        Ok(found_and_created)
    }
}

impl Drop for GpuInstance {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}
