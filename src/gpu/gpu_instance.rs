use std::sync::Arc;

use anyhow::Result;
use vulkano::VulkanLibrary;
use vulkano::device::QueueFlags;
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::swapchain::Surface;
use winit::raw_window_handle::HasDisplayHandle;

use crate::gpu::{GpuPhysicalDevice, GpuPhysicalDeviceCaps};

pub enum GpuPhysicalDeviceProfile {
    DiscreteGpu,
    HasGraphicsQueue,
    CanPresentTo(Arc<Surface>),
}

enum MatchType {
    NoMatch,
    MatchNoCap,
    MatchWithCaps(GpuPhysicalDeviceCaps),
}

pub struct GpuInstance {
    instance: Arc<Instance>,
}

impl GpuInstance {
    pub fn new(
        event_loop: &impl HasDisplayHandle,
        library: Arc<VulkanLibrary>,
        application_name: String,
    ) -> Result<Self> {
        let required_extensions = Surface::required_extensions(event_loop)?;
        let instance = Instance::new(
            library.clone(),
            InstanceCreateInfo {
                application_name: Some(application_name),
                max_api_version: Some(vulkano::Version::V1_3),
                enabled_extensions: required_extensions,
                ..Default::default()
            },
        )?;
        Ok(Self { instance })
    }

    pub fn get_vk_instance(&self) -> &Arc<Instance> {
        &self.instance
    }

    pub fn for_each_physical_device<F>(&self, f: F) -> Result<()>
    where
        F: FnMut((usize, Arc<PhysicalDevice>)),
    {
        self.instance
            .enumerate_physical_devices()?
            .enumerate()
            .for_each(f);
        Ok(())
    }

    fn match_profile(
        profile: &GpuPhysicalDeviceProfile,
        item: &(usize, Arc<PhysicalDevice>),
    ) -> MatchType {
        let (_, physical_device) = item;
        match profile {
            GpuPhysicalDeviceProfile::DiscreteGpu => {
                if physical_device.properties().device_type == PhysicalDeviceType::DiscreteGpu {
                    MatchType::MatchNoCap
                } else {
                    MatchType::NoMatch
                }
            }
            GpuPhysicalDeviceProfile::HasGraphicsQueue => {
                let queue_family_index = physical_device
                    .queue_family_properties()
                    .iter()
                    .enumerate()
                    .find_map(|(index, queue_family)| {
                        let index = index as u32;
                        if queue_family.queue_flags.contains(QueueFlags::GRAPHICS) {
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
                let queue_family_index = physical_device
                    .queue_family_properties()
                    .iter()
                    .enumerate()
                    .find_map(|(index, _)| {
                        let index = index as u32;
                        let supports_surface = physical_device
                            .surface_support(index, &surface)
                            .unwrap_or(false);
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
        &self,
        profiles: &[GpuPhysicalDeviceProfile],
    ) -> Result<Option<Arc<GpuPhysicalDevice>>> {
        let found_and_created = self
            .instance
            .enumerate_physical_devices()?
            .enumerate()
            .find_map(|item| {
                let mut caps = vec![];
                for profile in profiles {
                    match Self::match_profile(profile, &item) {
                        MatchType::NoMatch => return None,
                        MatchType::MatchNoCap => {}
                        MatchType::MatchWithCaps(cap) => caps.push(cap),
                    }
                }
                Some((item.1, caps))
            })
            .map(|(physical_device, caps)| Arc::new(GpuPhysicalDevice::new(physical_device, caps)));
        Ok(found_and_created)
    }
}
