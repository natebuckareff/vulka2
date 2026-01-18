use std::ffi::CStr;
use std::sync::Arc;

use crate::gpu::{
    GpuDevice, GpuDeviceProfile, GpuDeviceRequest, GpuDeviceRequestBuilder,
    GpuFindDeviceProfileResult, GpuInstance, GpuQueueProfile, GpuQueueRequest,
};
use anyhow::{Context, Result, anyhow};
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;
use vulkanalia::vk::{KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands};
use vulkanalia::window::{create_surface, get_required_instance_extensions};
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct Renderer {
    gpu_instance: Arc<GpuInstance>,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    gpu_device: Arc<GpuDevice>,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    graphics_queue_family: u32,
    present_queue_family: u32,
    swapchain: Option<vk::SwapchainKHR>,
    swapchain_format: Option<vk::Format>,
    swapchain_color_space: Option<vk::ColorSpaceKHR>,
    swapchain_extent: vk::Extent2D,
    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    render_pass: Option<vk::RenderPass>,
    framebuffers: Vec<vk::Framebuffer>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fence: vk::Fence,
    recreate_swapchain: bool,
    window_size: [u32; 2],
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        let loader =
            unsafe { LibloadingLoader::new(LIBRARY) }.context("failed to load Vulkan loader")?;

        let entry = unsafe { Entry::new(loader) }
            .map_err(|err| anyhow!("failed to create Vulkan entry: {}", err))?;

        const VK_LAYER_KHRONOS_VALIDATION: vk::ExtensionName =
            vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");

        let gpu_instance = GpuInstance::build(&entry)
            .application_name("voxels2".to_string())?
            .require_layer(VK_LAYER_KHRONOS_VALIDATION)?
            .require_extensions(&get_required_instance_extensions(&window))?
            .build()?;

        Self::log_physical_devices(gpu_instance.get_vk_instance())?;

        let surface = unsafe {
            create_surface(
                gpu_instance.get_vk_instance(),
                window.as_ref(),
                window.as_ref(),
            )
        }
        .context("failed to create window surface")?;

        let requests = GpuDeviceRequestBuilder::new()
            .is_discrete()
            .minimum_api_version(vk::make_version(1, 3, 0))
            .required_extension(vk::KHR_SWAPCHAIN_EXTENSION.name)
            .has_queue(
                "main",
                GpuQueueProfile {
                    priority: 1.0,
                    requests: vec![
                        GpuQueueRequest::HasGraphics,
                        GpuQueueRequest::CanPresentTo(surface),
                    ],
                },
            );

        let profile = gpu_instance.find_device_profile(&requests)?.ok()?;
        let physical_device = profile.physical_device();
        let gpu_device = GpuDevice::new(gpu_instance.clone(), profile)?;

        let main_queue = gpu_device
            .get_queue(requests.queue_request_index("main")?)
            .context("missing graphics/present queue")?;

        let graphics_queue_family = main_queue.family_index();
        let present_queue_family = main_queue.family_index();
        let graphics_queue = main_queue.get_vk_queue();
        let present_queue = main_queue.get_vk_queue();

        let command_pool = unsafe {
            gpu_device
                .get_vk_device()
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::builder()
                        .queue_family_index(graphics_queue_family)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )
                .context("failed to create command pool")?
        };

        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let image_available_semaphore = unsafe {
            gpu_device
                .get_vk_device()
                .create_semaphore(&semaphore_info, None)
                .context("failed to create acquire semaphore")?
        };
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight_fence = unsafe {
            gpu_device
                .get_vk_device()
                .create_fence(&fence_info, None)
                .context("failed to create fence")?
        };

        let window_size = window.inner_size();
        let mut renderer = Self {
            gpu_instance,
            surface,
            physical_device,
            gpu_device,
            graphics_queue,
            present_queue,
            graphics_queue_family,
            present_queue_family,
            swapchain: None,
            swapchain_format: None,
            swapchain_color_space: None,
            swapchain_extent: vk::Extent2D {
                width: 0,
                height: 0,
            },
            swapchain_images: Vec::new(),
            swapchain_image_views: Vec::new(),
            render_pass: None,
            framebuffers: Vec::new(),
            command_pool,
            command_buffers: Vec::new(),
            image_available_semaphore,
            render_finished_semaphores: Vec::new(),
            in_flight_fence,
            recreate_swapchain: true,
            window_size: [window_size.width, window_size.height],
        };

        renderer.recreate_swapchain_if_needed()?;
        Ok(renderer)
    }

    pub fn resized_window(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        self.window_size = [size.width, size.height];
        self.recreate_swapchain = true;
        Ok(())
    }

    pub fn render_frame(&mut self) -> Result<()> {
        if self.recreate_swapchain {
            self.recreate_swapchain_if_needed()?;
        }

        let swapchain = match self.swapchain {
            Some(swapchain) => swapchain,
            None => return Ok(()),
        };

        unsafe {
            self.gpu_device
                .get_vk_device()
                .wait_for_fences(&[self.in_flight_fence], true, u64::MAX)
                .context("failed to wait for in-flight fence")?;
            self.gpu_device
                .get_vk_device()
                .reset_fences(&[self.in_flight_fence])
                .context("failed to reset in-flight fence")?;
        }

        let (image_index, acquire_status) = unsafe {
            match self.gpu_device.get_vk_device().acquire_next_image_khr(
                swapchain,
                u64::MAX,
                self.image_available_semaphore,
                vk::Fence::null(),
            ) {
                Ok(result) => result,
                Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                    self.recreate_swapchain = true;
                    return Ok(());
                }
                Err(err) => return Err(anyhow!(err)),
            }
        };

        if acquire_status == vk::SuccessCode::SUBOPTIMAL_KHR {
            self.recreate_swapchain = true;
        }

        let command_buffer = *self
            .command_buffers
            .get(image_index as usize)
            .context("missing command buffer for swapchain image")?;

        let render_finished_semaphore = *self
            .render_finished_semaphores
            .get(image_index as usize)
            .context("missing render-finished semaphore for swapchain image")?;
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let wait_semaphores = [self.image_available_semaphore];
        let command_buffers = [command_buffer];
        let signal_semaphores = [render_finished_semaphore];
        let submit_info = [vk::SubmitInfo::builder()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores)];

        unsafe {
            self.gpu_device
                .get_vk_device()
                .queue_submit(self.graphics_queue, &submit_info, self.in_flight_fence)
                .context("failed to submit command buffer")?;
        }

        let present_wait = [render_finished_semaphore];
        let present_swapchains = [swapchain];
        let present_indices = [image_index];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&present_wait)
            .swapchains(&present_swapchains)
            .image_indices(&present_indices);

        let present_result = unsafe {
            self.gpu_device
                .get_vk_device()
                .queue_present_khr(self.present_queue, &present_info)
        };

        match present_result {
            Ok(status) => {
                if status == vk::SuccessCode::SUBOPTIMAL_KHR {
                    self.recreate_swapchain = true;
                }
            }
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                self.recreate_swapchain = true;
            }
            Err(err) => return Err(anyhow!(err)),
        }

        Ok(())
    }

    fn recreate_swapchain_if_needed(&mut self) -> Result<()> {
        if !self.recreate_swapchain {
            return Ok(());
        }

        if self.window_size[0] == 0 || self.window_size[1] == 0 {
            return Ok(());
        }

        unsafe {
            self.gpu_device
                .get_vk_device()
                .device_wait_idle()
                .context("failed waiting for device idle")?;
        }

        let old_swapchain = self.swapchain.take();
        self.cleanup_swapchain_resources()?;

        let (swapchain, images, format, color_space, extent) =
            self.create_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()))?;

        if let Some(old_swapchain) = old_swapchain {
            unsafe {
                self.gpu_device
                    .get_vk_device()
                    .destroy_swapchain_khr(old_swapchain, None);
            }
        }

        let render_pass = self.create_render_pass(format)?;
        let image_views = self.create_image_views(format, &images)?;
        let framebuffers = self.create_framebuffers(render_pass, &image_views, extent)?;
        let command_buffers = self.create_command_buffers(render_pass, &framebuffers, extent)?;
        let render_finished_semaphores = self.create_render_finished_semaphores(images.len())?;

        self.swapchain = Some(swapchain);
        self.swapchain_format = Some(format);
        self.swapchain_color_space = Some(color_space);
        self.swapchain_extent = extent;
        self.swapchain_images = images;
        self.swapchain_image_views = image_views;
        self.render_pass = Some(render_pass);
        self.framebuffers = framebuffers;
        self.command_buffers = command_buffers;
        self.render_finished_semaphores = render_finished_semaphores;
        self.recreate_swapchain = false;

        Ok(())
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
        let capabilities = unsafe {
            self.gpu_instance
                .get_vk_instance()
                .get_physical_device_surface_capabilities_khr(self.physical_device, self.surface)
                .context("failed to query surface capabilities")?
        };
        let formats = unsafe {
            self.gpu_instance
                .get_vk_instance()
                .get_physical_device_surface_formats_khr(self.physical_device, self.surface)
                .context("failed to query surface formats")?
        };
        let present_modes = unsafe {
            self.gpu_instance
                .get_vk_instance()
                .get_physical_device_surface_present_modes_khr(self.physical_device, self.surface)
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
            .surface(self.surface)
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
            self.gpu_device
                .get_vk_device()
                .create_swapchain_khr(&swapchain_info, None)
                .context("failed to create swapchain")?
        };

        let images = unsafe {
            self.gpu_device
                .get_vk_device()
                .get_swapchain_images_khr(swapchain)
                .context("failed to get swapchain images")?
        };

        Ok((swapchain, images, image_format, image_color_space, extent))
    }

    fn create_render_pass(&self, format: vk::Format) -> Result<vk::RenderPass> {
        let color_attachment = vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let color_attachment_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };

        let color_attachments = [color_attachment_ref];
        let subpass = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attachments);

        let dependency = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

        let attachments = [color_attachment];
        let subpasses = [subpass];
        let dependencies = [dependency];
        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);

        let render_pass = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_render_pass(&render_pass_info, None)
                .context("failed to create render pass")?
        };

        Ok(render_pass)
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
                self.gpu_device
                    .get_vk_device()
                    .create_image_view(&info, None)
                    .context("failed to create image view")?
            };
            views.push(view);
        }

        Ok(views)
    }

    fn create_framebuffers(
        &self,
        render_pass: vk::RenderPass,
        image_views: &[vk::ImageView],
        extent: vk::Extent2D,
    ) -> Result<Vec<vk::Framebuffer>> {
        let mut framebuffers = Vec::with_capacity(image_views.len());
        for &view in image_views {
            let attachments = [view];
            let info = vk::FramebufferCreateInfo::builder()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);

            let framebuffer = unsafe {
                self.gpu_device
                    .get_vk_device()
                    .create_framebuffer(&info, None)
                    .context("failed to create framebuffer")?
            };
            framebuffers.push(framebuffer);
        }

        Ok(framebuffers)
    }

    fn create_command_buffers(
        &self,
        render_pass: vk::RenderPass,
        framebuffers: &[vk::Framebuffer],
        extent: vk::Extent2D,
    ) -> Result<Vec<vk::CommandBuffer>> {
        if !self.command_buffers.is_empty() {
            unsafe {
                self.gpu_device
                    .get_vk_device()
                    .free_command_buffers(self.command_pool, &self.command_buffers);
            }
        }

        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(framebuffers.len() as u32);

        let command_buffers = unsafe {
            self.gpu_device
                .get_vk_device()
                .allocate_command_buffers(&alloc_info)
                .context("failed to allocate command buffers")?
        };

        for (command_buffer, &framebuffer) in command_buffers.iter().zip(framebuffers.iter()) {
            let begin_info = vk::CommandBufferBeginInfo::builder();
            unsafe {
                self.gpu_device
                    .get_vk_device()
                    .begin_command_buffer(*command_buffer, &begin_info)
                    .context("failed to begin command buffer")?;
            }

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [1.0, 0.0, 0.0, 1.0],
                },
            }];
            let render_area = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            };
            let render_pass_info = vk::RenderPassBeginInfo::builder()
                .render_pass(render_pass)
                .framebuffer(framebuffer)
                .render_area(render_area)
                .clear_values(&clear_values);

            unsafe {
                self.gpu_device.get_vk_device().cmd_begin_render_pass(
                    *command_buffer,
                    &render_pass_info,
                    vk::SubpassContents::INLINE,
                );
                self.gpu_device
                    .get_vk_device()
                    .cmd_end_render_pass(*command_buffer);
                self.gpu_device
                    .get_vk_device()
                    .end_command_buffer(*command_buffer)
                    .context("failed to end command buffer")?;
            }
        }

        Ok(command_buffers)
    }

    fn cleanup_swapchain_resources(&mut self) -> Result<()> {
        unsafe {
            if !self.command_buffers.is_empty() {
                self.gpu_device
                    .get_vk_device()
                    .free_command_buffers(self.command_pool, &self.command_buffers);
            }

            for framebuffer in self.framebuffers.drain(..) {
                self.gpu_device
                    .get_vk_device()
                    .destroy_framebuffer(framebuffer, None);
            }

            for view in self.swapchain_image_views.drain(..) {
                self.gpu_device
                    .get_vk_device()
                    .destroy_image_view(view, None);
            }

            if let Some(render_pass) = self.render_pass.take() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_render_pass(render_pass, None);
            }

            for semaphore in self.render_finished_semaphores.drain(..) {
                self.gpu_device
                    .get_vk_device()
                    .destroy_semaphore(semaphore, None);
            }
        }

        self.command_buffers.clear();
        self.swapchain_images.clear();
        self.swapchain_format = None;
        self.swapchain_color_space = None;

        Ok(())
    }

    fn create_render_finished_semaphores(&self, count: usize) -> Result<Vec<vk::Semaphore>> {
        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let mut semaphores = Vec::with_capacity(count);
        for _ in 0..count {
            let semaphore = unsafe {
                self.gpu_device
                    .get_vk_device()
                    .create_semaphore(&semaphore_info, None)
                    .context("failed to create render-finished semaphore")?
            };
            semaphores.push(semaphore);
        }
        Ok(semaphores)
    }

    fn print_extension_support(instance: &Arc<GpuInstance>) -> Result<()> {
        let devices = unsafe { instance.get_vk_instance().enumerate_physical_devices() }
            .context("failed to enumerate physical devices")?;

        for device in devices {
            let properties = unsafe {
                instance
                    .get_vk_instance()
                    .get_physical_device_properties(device)
            };
            let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }.to_string_lossy();
            println!("device name: {name}");

            let extensions = unsafe {
                instance
                    .get_vk_instance()
                    .enumerate_device_extension_properties(device, None)
                    .context("failed to enumerate device extensions")?
            };
            let supports_swapchain = extensions
                .iter()
                .any(|extension| extension.extension_name == vk::KHR_SWAPCHAIN_EXTENSION.name);
            println!("  extension VK_KHR_swapchain: {supports_swapchain}");
        }

        Ok(())
    }

    fn log_physical_devices(instance: &Instance) -> Result<()> {
        let devices = unsafe { instance.enumerate_physical_devices() }
            .context("failed to enumerate physical devices")?;

        for device in devices {
            let properties = unsafe { instance.get_physical_device_properties(device) };
            let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }.to_string_lossy();
            let api_version = properties.api_version;
            println!("device name: {:?}", name);
            println!("device api version: {}", api_version);
        }

        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.gpu_device.get_vk_device().device_wait_idle();
            let _ = self.cleanup_swapchain_resources();

            if let Some(swapchain) = self.swapchain.take() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_swapchain_khr(swapchain, None);
            }

            self.gpu_device
                .get_vk_device()
                .destroy_semaphore(self.image_available_semaphore, None);
            self.gpu_device
                .get_vk_device()
                .destroy_fence(self.in_flight_fence, None);
            self.gpu_device
                .get_vk_device()
                .destroy_command_pool(self.command_pool, None);
            self.gpu_instance
                .get_vk_instance()
                .destroy_surface_khr(self.surface, None);
        }
    }
}
