use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk;
use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;

use crate::gpu_v2::Device;

pub struct Swapchain {
    device: Arc<Device>,
    surface: vk::SurfaceKHR,
    swapchain: vk::SwapchainKHR,
    format: vk::SurfaceFormatKHR,
}

impl Swapchain {
    fn new(device: Arc<Device>, extent: vk::Extent2D) -> Result<Self> {
        let Some(surface) = device.engine().surface() else {
            return Err(anyhow!("surface not found"));
        };
        let (swapchain, format) = create_swapchain(&device, surface, extent, None)?;
        Ok(Self {
            device,
            surface,
            swapchain,
            format,
        })
    }

    pub fn recreate(&mut self, extent: vk::Extent2D) -> Result<()> {
        let old_swapchain = self.swapchain;
        let result = create_swapchain(&self.device, self.surface, extent, Some(old_swapchain))?;
        (self.swapchain, self.format) = result;
        unsafe {
            self.device
                .vk_device()
                .destroy_swapchain_khr(old_swapchain, None);
        }
        Ok(())
    }

    pub fn acquire(&self) -> Result<SwapchainImage> {
        todo!()
    }

    pub fn present(&self) {
        todo!()
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            self.device
                .vk_device()
                .destroy_swapchain_khr(self.swapchain, None);
        }
    }
}

struct SwapchainImage {
    // TODO
}

struct SurfaceSupport {
    capabilities: vk::SurfaceCapabilitiesKHR,
    formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
}

impl SurfaceSupport {
    fn new(device: &Device, surface: vk::SurfaceKHR) -> Result<Self> {
        use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;

        let instance = device.engine().vk_instance();
        let physical_device = device.info().physical_device;

        let capabilities = unsafe {
            instance.get_physical_device_surface_capabilities_khr(physical_device, surface)?
        };

        let formats =
            unsafe { instance.get_physical_device_surface_formats_khr(physical_device, surface)? };

        let present_modes = unsafe {
            instance.get_physical_device_surface_present_modes_khr(physical_device, surface)?
        };

        if formats.is_empty() {
            return Err(anyhow!("surface supports no formats"));
        }

        Ok(Self {
            capabilities,
            formats,
            present_modes,
        })
    }

    fn get_min_image_count(&self) -> u32 {
        let min = self.capabilities.min_image_count;
        let max = self.capabilities.max_image_count;
        let count = min + 1;
        if max == 0 {
            return count;
        } else {
            return count.clamp(min, max);
        }
    }

    fn get_srbg_nonlinear_surface_format(&self) -> Option<vk::SurfaceFormatKHR> {
        let preferences_srgb = &[vk::Format::B8G8R8A8_SRGB, vk::Format::R8G8B8A8_SRGB];
        let preferences_unorm = &[vk::Format::B8G8R8A8_UNORM, vk::Format::R8G8B8A8_UNORM];
        for pref in preferences_srgb {
            let format = self.formats.iter().find(|format| {
                format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR && format.format == *pref
            });
            if let Some(format) = format {
                return Some(*format);
            }
        }
        for pref in preferences_unorm {
            let format = self.formats.iter().find(|format| {
                format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR && format.format == *pref
            });
            if let Some(format) = format {
                return Some(*format);
            }
        }
        None
    }

    fn get_best_surface_format(&self) -> vk::SurfaceFormatKHR {
        match self.get_srbg_nonlinear_surface_format() {
            Some(format) => format,
            None => self.formats[0],
        }
    }

    fn get_clamped_extent(&self, extent: vk::Extent2D) -> vk::Extent2D {
        let current_extent = self.capabilities.current_extent;
        if current_extent.width != u32::MAX || current_extent.height != u32::MAX {
            current_extent
        } else {
            let min_extent = self.capabilities.min_image_extent;
            let max_extent = self.capabilities.max_image_extent;
            let mut extent = extent;
            extent.width = extent.width.clamp(min_extent.width, max_extent.width);
            extent.height = extent.height.clamp(min_extent.height, max_extent.height);
            extent
        }
    }

    fn get_present_mode(&self) -> Result<vk::PresentModeKHR> {
        if self.present_modes.contains(&vk::PresentModeKHR::FIFO) {
            Ok(vk::PresentModeKHR::FIFO)
        } else {
            if self.present_modes.is_empty() {
                return Err(anyhow!("no present modes available"));
            }
            Ok(self.present_modes[0])
        }
    }

    fn composite_alpha(&self) -> Result<vk::CompositeAlphaFlagsKHR> {
        let has_opaque = self
            .capabilities
            .supported_composite_alpha
            .contains(vk::CompositeAlphaFlagsKHR::OPAQUE);
        if has_opaque {
            Ok(vk::CompositeAlphaFlagsKHR::OPAQUE)
        } else {
            let bits = self.capabilities.supported_composite_alpha.bits();
            let flag = (bits != 0)
                .then(|| bits & bits.wrapping_neg())
                .and_then(vk::CompositeAlphaFlagsKHR::from_bits)
                .context("no composite alpha flags supported")?;
            Ok(flag)
        }
    }

    fn pre_transform(&self) -> vk::SurfaceTransformFlagsKHR {
        self.capabilities.current_transform
    }
}

fn create_swapchain(
    device: &Device,
    surface: vk::SurfaceKHR,
    extent: vk::Extent2D,
    old_swapchain: Option<vk::SwapchainKHR>,
) -> Result<(vk::SwapchainKHR, vk::SurfaceFormatKHR)> {
    use vulkanalia::prelude::v1_0::*;

    let support = SurfaceSupport::new(device, surface)?;

    let min_image_count = support.get_min_image_count();
    let format = support.get_best_surface_format();
    let image_format = format.format;
    let image_color_space = format.color_space;
    let extent = support.get_clamped_extent(extent);
    let pre_transform = support.pre_transform();
    let composite_alpha = support.composite_alpha()?;
    let present_mode = support.get_present_mode()?;

    let mut info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface)
        .min_image_count(min_image_count)
        .image_format(image_format)
        .image_color_space(image_color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(pre_transform)
        .composite_alpha(composite_alpha)
        .present_mode(present_mode)
        .clipped(true);

    if let Some(old_swapchain) = old_swapchain {
        info = info.old_swapchain(old_swapchain);
    }

    let swapchain = unsafe {
        device
            .vk_device()
            .create_swapchain_khr(&info, None)
            .context("failed to create swapchain")?
    };

    Ok((swapchain, format))
}
