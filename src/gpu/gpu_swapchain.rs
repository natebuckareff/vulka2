use std::sync::Arc;

use anyhow::{Context, Result};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk::{KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands};

use crate::gpu::{GpuDevice, GpuInstance, GpuSurface};

pub struct GpuSwapchain {
    instance: Arc<GpuInstance>,
    device: Arc<GpuDevice>,
    surface: Arc<GpuSurface>,
    physical_device: vk::PhysicalDevice,
    graphics_queue_family: u32,
    present_queue_family: u32,
    swapchain: Option<vk::SwapchainKHR>,
    format: Option<vk::Format>,
    color_space: Option<vk::ColorSpaceKHR>,
    extent: vk::Extent2D,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    recreate: bool,
    window_size: [u32; 2],
}

impl GpuSwapchain {
    pub fn new(
        instance: Arc<GpuInstance>,
        device: Arc<GpuDevice>,
        surface: Arc<GpuSurface>,
        physical_device: vk::PhysicalDevice,
        graphics_queue_family: u32,
        present_queue_family: u32,
        window_size: [u32; 2],
    ) -> Result<Self> {
        let mut swapchain = Self {
            instance,
            device,
            surface,
            physical_device,
            graphics_queue_family,
            present_queue_family,
            swapchain: None,
            format: None,
            color_space: None,
            extent: vk::Extent2D {
                width: 0,
                height: 0,
            },
            images: Vec::new(),
            image_views: Vec::new(),
            recreate: true,
            window_size,
        };

        let _ = swapchain.recreate_if_needed()?;
        Ok(swapchain)
    }

    pub fn resized(&mut self, window_size: [u32; 2]) {
        self.window_size = window_size;
        self.recreate = true;
    }

    pub fn mark_recreate(&mut self) {
        self.recreate = true;
    }

    pub fn swapchain(&self) -> Option<vk::SwapchainKHR> {
        self.swapchain
    }

    pub fn format(&self) -> Option<vk::Format> {
        self.format
    }

    pub fn color_space(&self) -> Option<vk::ColorSpaceKHR> {
        self.color_space
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.extent
    }

    pub fn image_views(&self) -> &[vk::ImageView] {
        &self.image_views
    }

    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    pub fn recreate_if_needed(&mut self) -> Result<bool> {
        if !self.recreate {
            return Ok(false);
        }

        if self.window_size[0] == 0 || self.window_size[1] == 0 {
            return Ok(false);
        }

        unsafe {
            self.device
                .get_vk_device()
                .device_wait_idle()
                .context("failed waiting for device idle")?;
        }

        let old_swapchain = self.swapchain.take();
        self.cleanup_swapchain_resources();

        let (swapchain, images, format, color_space, extent) =
            self.create_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()))?;

        if let Some(old_swapchain) = old_swapchain {
            unsafe {
                self.device
                    .get_vk_device()
                    .destroy_swapchain_khr(old_swapchain, None);
            }
        }

        let image_views = self.create_image_views(format, &images)?;

        self.swapchain = Some(swapchain);
        self.format = Some(format);
        self.color_space = Some(color_space);
        self.extent = extent;
        self.images = images;
        self.image_views = image_views;
        self.recreate = false;

        Ok(true)
    }

    fn create_swapchain(
        &self,
        old_swapchain: vk::SwapchainKHR,
    ) -> Result<(
        vk::SwapchainKHR,
        Vec<vk::Image>,
        vk::Format,
        vk::ColorSpaceKHR,
        vk::Extent2D,
    )> {
        let surface = self.surface.surface();
        let capabilities = unsafe {
            self.instance
                .get_vk_instance()
                .get_physical_device_surface_capabilities_khr(self.physical_device, surface)
                .context("failed to query surface capabilities")?
        };
        let formats = unsafe {
            self.instance
                .get_vk_instance()
                .get_physical_device_surface_formats_khr(self.physical_device, surface)
                .context("failed to query surface formats")?
        };
        let present_modes = unsafe {
            self.instance
                .get_vk_instance()
                .get_physical_device_surface_present_modes_khr(self.physical_device, surface)
                .context("failed to query present modes")?
        };

        let surface_format = formats.first().context("no surface formats available")?;
        let image_format = surface_format.format;
        let image_color_space = surface_format.color_space;

        let mut extent = if capabilities.current_extent.width == u32::MAX {
            vk::Extent2D {
                width: self.window_size[0],
                height: self.window_size[1],
            }
        } else {
            capabilities.current_extent
        };

        extent.width = extent.width.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        );
        extent.height = extent.height.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        );

        let mut min_image_count = capabilities.min_image_count + 1;
        if capabilities.max_image_count > 0 {
            min_image_count = min_image_count.min(capabilities.max_image_count);
        }

        let composite_alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
        ]
        .into_iter()
        .find(|alpha| capabilities.supported_composite_alpha.contains(*alpha))
        .context("no supported composite alpha")?;

        let present_mode = if present_modes.contains(&vk::PresentModeKHR::FIFO) {
            vk::PresentModeKHR::FIFO
        } else {
            *present_modes
                .first()
                .context("no present modes available")?
        };

        let sharing_mode = if self.graphics_queue_family != self.present_queue_family {
            vk::SharingMode::CONCURRENT
        } else {
            vk::SharingMode::EXCLUSIVE
        };
        let queue_family_indices = [self.graphics_queue_family, self.present_queue_family];

        let mut swapchain_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(surface)
            .min_image_count(min_image_count)
            .image_format(image_format)
            .image_color_space(image_color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(sharing_mode)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(composite_alpha)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old_swapchain);
        if sharing_mode == vk::SharingMode::CONCURRENT {
            swapchain_info = swapchain_info.queue_family_indices(&queue_family_indices);
        }

        let swapchain = unsafe {
            self.device
                .get_vk_device()
                .create_swapchain_khr(&swapchain_info, None)
                .context("failed to create swapchain")?
        };

        let images = unsafe {
            self.device
                .get_vk_device()
                .get_swapchain_images_khr(swapchain)
                .context("failed to get swapchain images")?
        };

        Ok((swapchain, images, image_format, image_color_space, extent))
    }

    fn create_image_views(
        &self,
        format: vk::Format,
        images: &[vk::Image],
    ) -> Result<Vec<vk::ImageView>> {
        let mut views = Vec::with_capacity(images.len());
        for &image in images {
            let info = vk::ImageViewCreateInfo::builder()
                .image(image)
                .view_type(vk::ImageViewType::_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let view = unsafe {
                self.device
                    .get_vk_device()
                    .create_image_view(&info, None)
                    .context("failed to create image view")?
            };
            views.push(view);
        }

        Ok(views)
    }

    fn cleanup_swapchain_resources(&mut self) {
        unsafe {
            for view in self.image_views.drain(..) {
                self.device.get_vk_device().destroy_image_view(view, None);
            }
        }

        self.images.clear();
        self.format = None;
        self.color_space = None;
    }
}

impl Drop for GpuSwapchain {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.get_vk_device().device_wait_idle();
            self.cleanup_swapchain_resources();

            if let Some(swapchain) = self.swapchain.take() {
                self.device
                    .get_vk_device()
                    .destroy_swapchain_khr(swapchain, None);
            }
        }
    }
}
