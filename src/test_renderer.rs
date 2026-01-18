use std::ffi::CStr;
use std::sync::Arc;

use crate::gpu::{
    GpuDevice, GpuDeviceFeatureV12, GpuDeviceFeatureV13, GpuDeviceRequestBuilder, GpuInstance,
    GpuQueueProfile, GpuQueueRequest, GpuSurface, GpuSwapchain,
};
use anyhow::{Context, Result, anyhow};
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;
use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct Renderer {
    gpu_swapchain: GpuSwapchain,
    gpu_device: Arc<GpuDevice>,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_layouts: Vec<vk::ImageLayout>,
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fence: vk::Fence,
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
            .require_extensions(&GpuSurface::required_instance_extensions(&window))?
            .build()?;

        Self::log_physical_devices(gpu_instance.get_vk_instance())?;

        let gpu_surface = GpuSurface::new(gpu_instance.clone(), window.as_ref())?;

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
                        GpuQueueRequest::CanPresentTo(gpu_surface.surface()),
                    ],
                },
            )
            .required_feature_vk12(GpuDeviceFeatureV12::BufferDeviceAddress)
            .required_feature_vk12(GpuDeviceFeatureV12::DescriptorBindingVariableDescriptorCount)
            .required_feature_vk12(GpuDeviceFeatureV12::DescriptorIndexing)
            .required_feature_vk12(GpuDeviceFeatureV12::RuntimeDescriptorArray)
            .required_feature_vk13(GpuDeviceFeatureV13::DynamicRendering)
            .required_feature_vk13(GpuDeviceFeatureV13::Synchronization2);

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
        let gpu_swapchain = GpuSwapchain::new(
            gpu_instance.clone(),
            gpu_device.clone(),
            gpu_surface.clone(),
            physical_device,
            graphics_queue_family,
            present_queue_family,
            [window_size.width, window_size.height],
        )?;

        let mut renderer = Self {
            gpu_swapchain,
            gpu_device,
            graphics_queue,
            present_queue,
            command_pool,
            command_buffers: Vec::new(),
            image_layouts: Vec::new(),
            image_available_semaphore,
            render_finished_semaphores: Vec::new(),
            in_flight_fence,
        };

        renderer.rebuild_swapchain_resources()?;
        Ok(renderer)
    }

    pub fn resized_window(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        self.gpu_swapchain.resized([size.width, size.height]);
        Ok(())
    }

    pub fn render_frame(&mut self) -> Result<()> {
        if self.gpu_swapchain.recreate_if_needed()? {
            self.rebuild_swapchain_resources()?;
        }

        let swapchain = match self.gpu_swapchain.swapchain() {
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
                    self.gpu_swapchain.mark_recreate();
                    return Ok(());
                }
                Err(err) => return Err(anyhow!(err)),
            }
        };

        if acquire_status == vk::SuccessCode::SUBOPTIMAL_KHR {
            self.gpu_swapchain.mark_recreate();
        }

        let command_buffer = *self
            .command_buffers
            .get(image_index as usize)
            .context("missing command buffer for swapchain image")?;

        self.record_command_buffer(image_index as usize)?;

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
                    self.gpu_swapchain.mark_recreate();
                }
            }
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                self.gpu_swapchain.mark_recreate();
            }
            Err(err) => return Err(anyhow!(err)),
        }

        Ok(())
    }

    fn rebuild_swapchain_resources(&mut self) -> Result<()> {
        if self.gpu_swapchain.swapchain().is_none() {
            return Ok(());
        }

        self.cleanup_swapchain_resources()?;

        let image_count = self.gpu_swapchain.image_count();
        let command_buffers = self.create_command_buffers(image_count)?;
        let render_finished_semaphores = self.create_render_finished_semaphores(image_count)?;

        self.command_buffers = command_buffers;
        self.render_finished_semaphores = render_finished_semaphores;
        self.image_layouts = vec![vk::ImageLayout::UNDEFINED; image_count];

        Ok(())
    }

    fn create_command_buffers(&self, count: usize) -> Result<Vec<vk::CommandBuffer>> {
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
            .command_buffer_count(count as u32);

        let command_buffers = unsafe {
            self.gpu_device
                .get_vk_device()
                .allocate_command_buffers(&alloc_info)
                .context("failed to allocate command buffers")?
        };

        Ok(command_buffers)
    }

    fn record_command_buffer(&mut self, image_index: usize) -> Result<()> {
        let command_buffer = *self
            .command_buffers
            .get(image_index)
            .context("missing command buffer for swapchain image")?;
        let image = self
            .gpu_swapchain
            .image(image_index)
            .context("missing swapchain image")?;
        let image_view = self
            .gpu_swapchain
            .image_view(image_index)
            .context("missing swapchain image view")?;
        let extent = self.gpu_swapchain.extent();
        let old_layout = *self
            .image_layouts
            .get(image_index)
            .context("missing swapchain image layout")?;

        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.gpu_device
                .get_vk_device()
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .context("failed to reset command buffer")?;
            self.gpu_device
                .get_vk_device()
                .begin_command_buffer(command_buffer, &begin_info)
                .context("failed to begin command buffer")?;
        }

        let to_color_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let to_color_info = vk::DependencyInfo::builder()
            .image_memory_barriers(std::slice::from_ref(&to_color_barrier));

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [1.0, 0.0, 0.0, 1.0],
            },
        };
        let color_attachment = vk::RenderingAttachmentInfo::builder()
            .image_view(image_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value);

        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
        let rendering_info = vk::RenderingInfo::builder()
            .render_area(render_area)
            .layer_count(1)
            .color_attachments(std::slice::from_ref(&color_attachment));

        let to_present_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let to_present_info = vk::DependencyInfo::builder()
            .image_memory_barriers(std::slice::from_ref(&to_present_barrier));

        unsafe {
            self.gpu_device
                .get_vk_device()
                .cmd_pipeline_barrier2(command_buffer, &to_color_info);
            self.gpu_device
                .get_vk_device()
                .cmd_begin_rendering(command_buffer, &rendering_info);
            self.gpu_device
                .get_vk_device()
                .cmd_end_rendering(command_buffer);
            self.gpu_device
                .get_vk_device()
                .cmd_pipeline_barrier2(command_buffer, &to_present_info);
            self.gpu_device
                .get_vk_device()
                .end_command_buffer(command_buffer)
                .context("failed to end command buffer")?;
        }

        if let Some(layout) = self.image_layouts.get_mut(image_index) {
            *layout = vk::ImageLayout::PRESENT_SRC_KHR;
        }

        Ok(())
    }

    fn cleanup_swapchain_resources(&mut self) -> Result<()> {
        unsafe {
            if !self.command_buffers.is_empty() {
                self.gpu_device
                    .get_vk_device()
                    .free_command_buffers(self.command_pool, &self.command_buffers);
            }

            for semaphore in self.render_finished_semaphores.drain(..) {
                self.gpu_device
                    .get_vk_device()
                    .destroy_semaphore(semaphore, None);
            }
        }

        self.command_buffers.clear();
        self.image_layouts.clear();

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

            self.gpu_device
                .get_vk_device()
                .destroy_semaphore(self.image_available_semaphore, None);
            self.gpu_device
                .get_vk_device()
                .destroy_fence(self.in_flight_fence, None);
            self.gpu_device
                .get_vk_device()
                .destroy_command_pool(self.command_pool, None);
        }
    }
}
