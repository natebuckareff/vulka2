use anyhow::{Context, Result, bail};
use vulkanalia::vk;

use crate::gal::{Device, Surface};

pub struct SurfaceInfo {
    capabilities: vk::SurfaceCapabilitiesKHR,
    formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
}

impl SurfaceInfo {
    pub(crate) fn new(device: &Device, surface: &Surface) -> Result<Self> {
        use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;

        let instance = unsafe { device.engine().instance() };
        let physical_device = device.physical_device();
        let surface = unsafe { surface.resource().handle() };

        let capabilities = unsafe {
            instance.get_physical_device_surface_capabilities_khr(physical_device, surface)?
        };

        let formats =
            unsafe { instance.get_physical_device_surface_formats_khr(physical_device, surface)? };

        let present_modes = unsafe {
            instance.get_physical_device_surface_present_modes_khr(physical_device, surface)?
        };

        if formats.is_empty() {
            bail!("surface supports no formats");
        }

        Ok(Self {
            capabilities,
            formats,
            present_modes,
        })
    }

    pub fn get_min_image_count(&self) -> u32 {
        let min = self.capabilities.min_image_count;
        let max = self.capabilities.max_image_count;
        let count = min + 1;
        if max == 0 {
            count
        } else {
            count.clamp(min, max)
        }
    }

    pub fn get_srgb_nonlinear_surface_format(&self) -> Option<vk::SurfaceFormatKHR> {
        let srgb = [vk::Format::B8G8R8A8_SRGB, vk::Format::R8G8B8A8_SRGB];
        let unorm = [vk::Format::B8G8R8A8_UNORM, vk::Format::R8G8B8A8_UNORM];

        for preferred in srgb {
            let format = self.formats.iter().find(|format| {
                format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                    && format.format == preferred
            });
            if let Some(format) = format {
                return Some(*format);
            }
        }

        for preferred in unorm {
            let format = self.formats.iter().find(|format| {
                format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                    && format.format == preferred
            });
            if let Some(format) = format {
                return Some(*format);
            }
        }

        None
    }

    pub fn get_best_surface_format(&self) -> vk::SurfaceFormatKHR {
        self.get_srgb_nonlinear_surface_format()
            .unwrap_or(self.formats[0])
    }

    pub fn get_clamped_extent(&self, extent: vk::Extent2D) -> vk::Extent2D {
        let current_extent = self.capabilities.current_extent;
        if current_extent.width != u32::MAX || current_extent.height != u32::MAX {
            current_extent
        } else {
            let min_extent = self.capabilities.min_image_extent;
            let max_extent = self.capabilities.max_image_extent;
            vk::Extent2D {
                width: extent.width.clamp(min_extent.width, max_extent.width),
                height: extent.height.clamp(min_extent.height, max_extent.height),
            }
        }
    }

    pub fn composite_alpha(&self) -> Result<vk::CompositeAlphaFlagsKHR> {
        if self
            .capabilities
            .supported_composite_alpha
            .contains(vk::CompositeAlphaFlagsKHR::OPAQUE)
        {
            return Ok(vk::CompositeAlphaFlagsKHR::OPAQUE);
        }

        let bits = self.capabilities.supported_composite_alpha.bits();
        let flag = (bits != 0)
            .then(|| bits & bits.wrapping_neg())
            .and_then(vk::CompositeAlphaFlagsKHR::from_bits)
            .context("no composite alpha flags supported")?;

        Ok(flag)
    }

    pub fn pre_transform(&self) -> vk::SurfaceTransformFlagsKHR {
        self.capabilities.current_transform
    }
}
