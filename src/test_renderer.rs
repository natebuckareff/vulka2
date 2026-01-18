use std::ffi::CStr;
use std::sync::Arc;
use std::time::Instant;

use crate::gpu::{
    GpuDevice, GpuDeviceFeatureV12, GpuDeviceFeatureV13, GpuDeviceRequestBuilder, GpuInstance,
    GpuQueueProfile, GpuQueueRequest, GpuSurface, GpuSwapchain,
};
use anyhow::{Context, Result, anyhow};
use glam::{Mat4, Vec2, Vec3};
use shader_slang as slang;
use slang::Downcast;
use slang_struct::slang_include;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;
use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
use vulkanalia_vma::{self as vma, Alloc};
use winit::dpi::PhysicalSize;
use winit::window::Window;

slang_include!("shaders/cube.inl");

impl Vertex {
    fn new(position: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position: Vec3::from_array(position),
            _pad0: 0.0,
            uv: Vec2::from_array(uv),
            _pad1: Vec2::ZERO,
        }
    }
}

const MAX_TEXTURES: u32 = 1;
const TEXTURE_PATH: &str = "assets/debug-texture.png";

pub struct Renderer {
    gpu_instance: Arc<GpuInstance>,
    gpu_swapchain: GpuSwapchain,
    gpu_device: Arc<GpuDevice>,
    physical_device: vk::PhysicalDevice,
    allocator: Option<vma::Allocator>,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_layouts: Vec<vk::ImageLayout>,
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fence: vk::Fence,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    texture_image: vk::Image,
    texture_image_allocation: Option<vma::Allocation>,
    texture_image_view: vk::ImageView,
    texture_sampler: vk::Sampler,
    depth_image: vk::Image,
    depth_image_allocation: Option<vma::Allocation>,
    depth_image_view: vk::ImageView,
    depth_format: vk::Format,
    depth_layout: vk::ImageLayout,
    vertex_buffer: vk::Buffer,
    vertex_buffer_allocation: Option<vma::Allocation>,
    vertex_buffer_address: vk::DeviceAddress,
    index_buffer: vk::Buffer,
    index_buffer_allocation: Option<vma::Allocation>,
    index_buffer_address: vk::DeviceAddress,
    start_time: Instant,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        let loader =
            unsafe { LibloadingLoader::new(LIBRARY) }.context("failed to load Vulkan loader")?;

        let entry = unsafe { Entry::new(loader) }
            .map_err(|err| anyhow!("failed to create Vulkan entry: {}", err))?;

        const VK_LAYER_KHRONOS_VALIDATION: vk::ExtensionName =
            vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");
        const VK_KHR_SHADER_DRAW_PARAMETERS: vk::ExtensionName =
            vk::ExtensionName::from_bytes(b"VK_KHR_shader_draw_parameters");

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
            .required_extension(VK_KHR_SHADER_DRAW_PARAMETERS)
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

        let mut allocator_options = vma::AllocatorOptions::new(
            gpu_instance.get_vk_instance(),
            gpu_device.get_vk_device(),
            physical_device,
        );
        allocator_options.flags = vma::AllocatorCreateFlags::BUFFER_DEVICE_ADDRESS;
        let allocator = unsafe { vma::Allocator::new(&allocator_options) }
            .map_err(|err| anyhow!(err))
            .context("failed to create VMA allocator")?;

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
            gpu_instance,
            gpu_swapchain,
            gpu_device,
            physical_device,
            allocator: Some(allocator),
            graphics_queue,
            present_queue,
            command_pool,
            command_buffers: Vec::new(),
            image_layouts: Vec::new(),
            image_available_semaphore,
            render_finished_semaphores: Vec::new(),
            in_flight_fence,
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_set: vk::DescriptorSet::null(),
            texture_image: vk::Image::null(),
            texture_image_allocation: None,
            texture_image_view: vk::ImageView::null(),
            texture_sampler: vk::Sampler::null(),
            depth_image: vk::Image::null(),
            depth_image_allocation: None,
            depth_image_view: vk::ImageView::null(),
            depth_format: vk::Format::UNDEFINED,
            depth_layout: vk::ImageLayout::UNDEFINED,
            vertex_buffer: vk::Buffer::null(),
            vertex_buffer_allocation: None,
            vertex_buffer_address: 0,
            index_buffer: vk::Buffer::null(),
            index_buffer_allocation: None,
            index_buffer_address: 0,
            start_time: Instant::now(),
        };

        renderer.create_static_resources()?;
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

        let elapsed = self.start_time.elapsed().as_secs_f32();
        self.record_command_buffer(image_index as usize, elapsed)?;

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

    fn create_static_resources(&mut self) -> Result<()> {
        self.create_descriptor_set_layout()?;
        self.create_pipeline_layout()?;
        self.create_texture_resources()?;
        self.create_descriptor_pool_and_set()?;
        self.create_mesh_buffers()?;
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
        self.depth_layout = vk::ImageLayout::UNDEFINED;

        self.create_depth_resources()?;
        self.create_pipeline()?;

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

    fn record_command_buffer(&mut self, image_index: usize, elapsed: f32) -> Result<()> {
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

        let (color_src_stage, color_src_access) = if old_layout == vk::ImageLayout::PRESENT_SRC_KHR
        {
            (
                vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                vk::AccessFlags2::empty(),
            )
        } else {
            (
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                vk::AccessFlags2::empty(),
            )
        };
        let to_color_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(color_src_stage)
            .src_access_mask(color_src_access)
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
            })
            .build();
        let (depth_src_stage, depth_src_access) =
            if self.depth_layout == vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL {
                (
                    vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                )
            } else {
                (
                    vk::PipelineStageFlags2::TOP_OF_PIPE,
                    vk::AccessFlags2::empty(),
                )
            };
        let to_depth_barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(depth_src_stage)
            .src_access_mask(depth_src_access)
            .dst_stage_mask(vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS)
            .dst_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
            .old_layout(self.depth_layout)
            .new_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .image(self.depth_image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build();
        let barriers = [to_color_barrier, to_depth_barrier];
        let barrier_info = vk::DependencyInfo::builder().image_memory_barriers(&barriers);

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.05, 0.05, 0.08, 1.0],
            },
        };
        let color_attachment = vk::RenderingAttachmentInfo::builder()
            .image_view(image_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value);
        let depth_clear = vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        };
        let depth_attachment = vk::RenderingAttachmentInfo::builder()
            .image_view(self.depth_image_view)
            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(depth_clear);

        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
        let rendering_info = vk::RenderingInfo::builder()
            .render_area(render_area)
            .layer_count(1)
            .color_attachments(std::slice::from_ref(&color_attachment))
            .depth_attachment(&depth_attachment);

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
            })
            .build();
        let to_present_info = vk::DependencyInfo::builder()
            .image_memory_barriers(std::slice::from_ref(&to_present_barrier));

        let aspect = extent.width as f32 / extent.height as f32;
        let model = Mat4::from_rotation_y(elapsed);
        let view = Mat4::look_at_rh(Vec3::new(4.0, 3.0, 4.0), Vec3::ZERO, Vec3::Y);
        let mut projection = Mat4::perspective_rh(45.0_f32.to_radians(), aspect, 0.1, 100.0);
        projection.y_axis.y *= -1.0;
        let mvp = projection * view * model;
        let push_constants = PushConstants {
            mvp,
            vertices: self.vertex_buffer_address,
            indices: self.index_buffer_address,
            texture_index: 0,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };
        let push_constants_bytes = unsafe {
            std::slice::from_raw_parts(
                (&push_constants as *const PushConstants) as *const u8,
                std::mem::size_of::<PushConstants>(),
            )
        };

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        unsafe {
            self.gpu_device
                .get_vk_device()
                .cmd_pipeline_barrier2(command_buffer, &barrier_info);
            self.gpu_device
                .get_vk_device()
                .cmd_begin_rendering(command_buffer, &rendering_info);
            self.gpu_device.get_vk_device().cmd_set_viewport(
                command_buffer,
                0,
                std::slice::from_ref(&viewport),
            );
            self.gpu_device.get_vk_device().cmd_set_scissor(
                command_buffer,
                0,
                std::slice::from_ref(&scissor),
            );
            self.gpu_device.get_vk_device().cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );
            self.gpu_device.get_vk_device().cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                std::slice::from_ref(&self.descriptor_set),
                &[],
            );
            self.gpu_device.get_vk_device().cmd_push_constants(
                command_buffer,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                0,
                push_constants_bytes,
            );
            self.gpu_device
                .get_vk_device()
                .cmd_draw(command_buffer, 36, 1, 0, 0);
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
        self.depth_layout = vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL;

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

            if self.pipeline != vk::Pipeline::null() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_pipeline(self.pipeline, None);
                self.pipeline = vk::Pipeline::null();
            }

            if self.depth_image_view != vk::ImageView::null() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_image_view(self.depth_image_view, None);
                self.depth_image_view = vk::ImageView::null();
            }

            if self.depth_image != vk::Image::null() {
                if let Some(allocation) = self.depth_image_allocation.take() {
                    self.allocator().destroy_image(self.depth_image, allocation);
                } else {
                    self.gpu_device
                        .get_vk_device()
                        .destroy_image(self.depth_image, None);
                }
                self.depth_image = vk::Image::null();
            }

            self.depth_image_allocation = None;
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

    fn create_descriptor_set_layout(&mut self) -> Result<()> {
        let bindings = [
            vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(MAX_TEXTURES)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .build(),
        ];

        let binding_flags = [
            vk::DescriptorBindingFlags::empty(),
            vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT,
        ];
        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::builder().binding_flags(&binding_flags);

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .push_next(&mut binding_flags_info);

        let layout = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_descriptor_set_layout(&layout_info, None)
                .context("failed to create descriptor set layout")?
        };
        self.descriptor_set_layout = layout;
        Ok(())
    }

    fn create_pipeline_layout(&mut self) -> Result<()> {
        let push_constant_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(std::mem::size_of::<PushConstants>() as u32);
        let layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(std::slice::from_ref(&self.descriptor_set_layout))
            .push_constant_ranges(std::slice::from_ref(&push_constant_range));
        let layout = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_pipeline_layout(&layout_info, None)
                .context("failed to create pipeline layout")?
        };
        self.pipeline_layout = layout;
        Ok(())
    }

    fn create_descriptor_pool_and_set(&mut self) -> Result<()> {
        let pool_sizes = [
            vk::DescriptorPoolSize {
                type_: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: MAX_TEXTURES,
            },
            vk::DescriptorPoolSize {
                type_: vk::DescriptorType::SAMPLER,
                descriptor_count: 1,
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(1)
            .pool_sizes(&pool_sizes);
        let pool = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_descriptor_pool(&pool_info, None)
                .context("failed to create descriptor pool")?
        };
        self.descriptor_pool = pool;

        let descriptor_counts = [1u32];
        let mut variable_count_info =
            vk::DescriptorSetVariableDescriptorCountAllocateInfo::builder()
                .descriptor_counts(&descriptor_counts);
        let alloc_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(std::slice::from_ref(&self.descriptor_set_layout))
            .push_next(&mut variable_count_info);
        let sets = unsafe {
            self.gpu_device
                .get_vk_device()
                .allocate_descriptor_sets(&alloc_info)
                .context("failed to allocate descriptor sets")?
        };
        self.descriptor_set = sets[0];

        let sampler_info = vk::DescriptorImageInfo::builder()
            .sampler(self.texture_sampler)
            .build();
        let image_info = vk::DescriptorImageInfo::builder()
            .image_view(self.texture_image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .build();
        let writes = [
            vk::WriteDescriptorSet::builder()
                .dst_set(self.descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(std::slice::from_ref(&sampler_info))
                .build(),
            vk::WriteDescriptorSet::builder()
                .dst_set(self.descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .image_info(std::slice::from_ref(&image_info))
                .build(),
        ];

        let copies: [vk::CopyDescriptorSet; 0] = [];
        unsafe {
            self.gpu_device
                .get_vk_device()
                .update_descriptor_sets(&writes, &copies);
        }

        Ok(())
    }

    fn create_texture_resources(&mut self) -> Result<()> {
        let image = image::open(TEXTURE_PATH)
            .with_context(|| format!("failed to load texture at {}", TEXTURE_PATH))?
            .to_rgba8();
        let width = image.width();
        let height = image.height();
        let image_data = image.into_raw();
        let image_size = image_data.len() as vk::DeviceSize;

        let staging_options = vma::AllocationOptions {
            usage: vma::MemoryUsage::AutoPreferHost,
            flags: vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            ..Default::default()
        };
        let (staging_buffer, staging_allocation) = self.create_buffer(
            image_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            &staging_options,
        )?;
        self.write_memory(staging_allocation, &image_data)?;

        let texture_allocation = vma::AllocationOptions {
            usage: vma::MemoryUsage::AutoPreferDevice,
            ..Default::default()
        };
        let (texture_image, texture_allocation) = self.create_image(
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            &texture_allocation,
        )?;
        self.texture_image = texture_image;
        self.texture_image_allocation = Some(texture_allocation);

        self.submit_immediate(|command_buffer| {
            let to_transfer = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
                .src_access_mask(vk::AccessFlags2::empty())
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .image(self.texture_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();
            let to_transfer_info = vk::DependencyInfo::builder()
                .image_memory_barriers(std::slice::from_ref(&to_transfer));
            unsafe {
                self.gpu_device
                    .get_vk_device()
                    .cmd_pipeline_barrier2(command_buffer, &to_transfer_info);
            }

            let region = vk::BufferImageCopy::builder()
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .build();
            unsafe {
                self.gpu_device.get_vk_device().cmd_copy_buffer_to_image(
                    command_buffer,
                    staging_buffer,
                    self.texture_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    std::slice::from_ref(&region),
                );
            }

            let to_shader = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
                .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(self.texture_image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .build();
            let to_shader_info = vk::DependencyInfo::builder()
                .image_memory_barriers(std::slice::from_ref(&to_shader));
            unsafe {
                self.gpu_device
                    .get_vk_device()
                    .cmd_pipeline_barrier2(command_buffer, &to_shader_info);
            }

            Ok(())
        })?;

        unsafe {
            self.allocator()
                .destroy_buffer(staging_buffer, staging_allocation);
        }

        let view_info = vk::ImageViewCreateInfo::builder()
            .image(self.texture_image)
            .view_type(vk::ImageViewType::_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
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
                .create_image_view(&view_info, None)
                .context("failed to create texture image view")?
        };
        self.texture_image_view = view;

        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .anisotropy_enable(false)
            .max_anisotropy(1.0);
        let sampler = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_sampler(&sampler_info, None)
                .context("failed to create sampler")?
        };
        self.texture_sampler = sampler;

        Ok(())
    }

    fn create_mesh_buffers(&mut self) -> Result<()> {
        let vertices: [Vertex; 24] = [
            Vertex::new([-1.0, -1.0, 1.0], [0.0, 1.0]),
            Vertex::new([1.0, -1.0, 1.0], [1.0, 1.0]),
            Vertex::new([1.0, 1.0, 1.0], [1.0, 0.0]),
            Vertex::new([-1.0, 1.0, 1.0], [0.0, 0.0]),
            Vertex::new([1.0, -1.0, -1.0], [0.0, 1.0]),
            Vertex::new([-1.0, -1.0, -1.0], [1.0, 1.0]),
            Vertex::new([-1.0, 1.0, -1.0], [1.0, 0.0]),
            Vertex::new([1.0, 1.0, -1.0], [0.0, 0.0]),
            Vertex::new([-1.0, -1.0, -1.0], [0.0, 1.0]),
            Vertex::new([-1.0, -1.0, 1.0], [1.0, 1.0]),
            Vertex::new([-1.0, 1.0, 1.0], [1.0, 0.0]),
            Vertex::new([-1.0, 1.0, -1.0], [0.0, 0.0]),
            Vertex::new([1.0, -1.0, 1.0], [0.0, 1.0]),
            Vertex::new([1.0, -1.0, -1.0], [1.0, 1.0]),
            Vertex::new([1.0, 1.0, -1.0], [1.0, 0.0]),
            Vertex::new([1.0, 1.0, 1.0], [0.0, 0.0]),
            Vertex::new([-1.0, 1.0, 1.0], [0.0, 1.0]),
            Vertex::new([1.0, 1.0, 1.0], [1.0, 1.0]),
            Vertex::new([1.0, 1.0, -1.0], [1.0, 0.0]),
            Vertex::new([-1.0, 1.0, -1.0], [0.0, 0.0]),
            Vertex::new([-1.0, -1.0, -1.0], [0.0, 1.0]),
            Vertex::new([1.0, -1.0, -1.0], [1.0, 1.0]),
            Vertex::new([1.0, -1.0, 1.0], [1.0, 0.0]),
            Vertex::new([-1.0, -1.0, 1.0], [0.0, 0.0]),
        ];
        let indices: [u32; 36] = [
            0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4, 8, 9, 10, 10, 11, 8, 12, 13, 14, 14, 15, 12, 16,
            17, 18, 18, 19, 16, 20, 21, 22, 22, 23, 20,
        ];

        let host_allocation = vma::AllocationOptions {
            usage: vma::MemoryUsage::AutoPreferHost,
            flags: vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            ..Default::default()
        };
        let (vertex_buffer, vertex_allocation) = self.create_buffer(
            (vertices.len() * std::mem::size_of::<Vertex>()) as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            &host_allocation,
        )?;
        self.write_memory(vertex_allocation, &vertices)?;
        let vertex_address_info = vk::BufferDeviceAddressInfo::builder().buffer(vertex_buffer);
        let vertex_address = unsafe {
            self.gpu_device
                .get_vk_device()
                .get_buffer_device_address(&vertex_address_info)
        };

        let (index_buffer, index_allocation) = self.create_buffer(
            (indices.len() * std::mem::size_of::<u32>()) as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            &host_allocation,
        )?;
        self.write_memory(index_allocation, &indices)?;
        let index_address_info = vk::BufferDeviceAddressInfo::builder().buffer(index_buffer);
        let index_address = unsafe {
            self.gpu_device
                .get_vk_device()
                .get_buffer_device_address(&index_address_info)
        };

        self.vertex_buffer = vertex_buffer;
        self.vertex_buffer_allocation = Some(vertex_allocation);
        self.vertex_buffer_address = vertex_address;
        self.index_buffer = index_buffer;
        self.index_buffer_allocation = Some(index_allocation);
        self.index_buffer_address = index_address;

        Ok(())
    }

    fn create_depth_resources(&mut self) -> Result<()> {
        let depth_format = self.select_depth_format()?;
        let extent = self.gpu_swapchain.extent();
        let depth_allocation = vma::AllocationOptions {
            usage: vma::MemoryUsage::AutoPreferDevice,
            ..Default::default()
        };
        let (depth_image, depth_allocation) = self.create_image(
            extent.width,
            extent.height,
            depth_format,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            &depth_allocation,
        )?;
        let view_info = vk::ImageViewCreateInfo::builder()
            .image(depth_image)
            .view_type(vk::ImageViewType::_2D)
            .format(depth_format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        let view = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_image_view(&view_info, None)
                .context("failed to create depth image view")?
        };

        self.depth_format = depth_format;
        self.depth_image = depth_image;
        self.depth_image_allocation = Some(depth_allocation);
        self.depth_image_view = view;
        Ok(())
    }

    fn create_pipeline(&mut self) -> Result<()> {
        let color_format = self
            .gpu_swapchain
            .format()
            .context("missing swapchain format")?;

        let (vertex_code, fragment_code) = self.compile_slang_shaders()?;

        let vertex_module = self.create_shader_module(&vertex_code)?;
        let fragment_module = self.create_shader_module(&fragment_code)?;
        let vertex_name = std::ffi::CString::new("vertexMain").unwrap();
        let fragment_name = std::ffi::CString::new("fragmentMain").unwrap();
        let stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(vertex_name.as_bytes_with_nul())
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_module)
                .name(fragment_name.as_bytes_with_nul())
                .build(),
        ];

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewport_count(1)
            .scissor_count(1);
        let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
            .rasterization_samples(vk::SampleCountFlags::_1);
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS);
        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            )
            .blend_enable(false)
            .build();
        let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .attachments(std::slice::from_ref(&color_blend_attachment));
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&dynamic_states);

        let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
            .color_attachment_formats(std::slice::from_ref(&color_format))
            .depth_attachment_format(self.depth_format);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .push_next(&mut rendering_info)
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic_state)
            .layout(self.pipeline_layout)
            .render_pass(vk::RenderPass::null())
            .subpass(0);

        let pipeline = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .context("failed to create graphics pipeline")?
        }
        .0[0];

        unsafe {
            self.gpu_device
                .get_vk_device()
                .destroy_shader_module(vertex_module, None);
            self.gpu_device
                .get_vk_device()
                .destroy_shader_module(fragment_module, None);
        }

        self.pipeline = pipeline;
        Ok(())
    }

    fn compile_slang_shaders(&self) -> Result<(Vec<u32>, Vec<u32>)> {
        let global_session =
            slang::GlobalSession::new().context("failed to create Slang session")?;
        let search_path = std::ffi::CString::new("shaders").unwrap();
        let physical_storage = global_session.find_capability("SPV_EXT_physical_storage_buffer");
        if physical_storage.is_unknown() {
            return Err(anyhow!(
                "Slang capability SPV_EXT_physical_storage_buffer is unavailable"
            ));
        }
        let descriptor_indexing = global_session.find_capability("SPV_EXT_descriptor_indexing");
        if descriptor_indexing.is_unknown() {
            return Err(anyhow!(
                "Slang capability SPV_EXT_descriptor_indexing is unavailable"
            ));
        }
        let compiler_options = slang::CompilerOptions::default()
            .optimization(slang::OptimizationLevel::High)
            .matrix_layout_row(true)
            .vulkan_use_entry_point_name(true)
            .capability(physical_storage)
            .capability(descriptor_indexing);
        let profile = global_session.find_profile("glsl_450");
        if profile.is_unknown() {
            return Err(anyhow!("Slang profile glsl_450 is unavailable"));
        }
        let target_desc = slang::TargetDesc::default()
            .format(slang::CompileTarget::Spirv)
            .profile(profile)
            .options(&compiler_options);
        let targets = [target_desc];
        let search_paths = [search_path.as_ptr()];
        let session_desc = slang::SessionDesc::default()
            .targets(&targets)
            .search_paths(&search_paths);
        let session = global_session
            .create_session(&session_desc)
            .context("failed to create Slang session")?;
        let module = session
            .load_module("cube.slang")
            .map_err(|err| anyhow!("failed to load cube.slang: {:?}", err))?;

        let vertex_entry = module
            .find_entry_point_by_name("vertexMain")
            .context("missing vertexMain entry point")?;
        let vertex_program = session
            .create_composite_component_type(&[
                module.downcast().clone(),
                vertex_entry.downcast().clone(),
            ])
            .map_err(|err| anyhow!("failed to link vertex entry point: {:?}", err))?
            .link()
            .map_err(|err| anyhow!("failed to finalize vertex shader: {:?}", err))?;
        let vertex_blob = vertex_program
            .entry_point_code(0, 0)
            .map_err(|err| anyhow!("failed to get vertex shader code: {:?}", err))?;

        let fragment_entry = module
            .find_entry_point_by_name("fragmentMain")
            .context("missing fragmentMain entry point")?;
        let fragment_program = session
            .create_composite_component_type(&[
                module.downcast().clone(),
                fragment_entry.downcast().clone(),
            ])
            .map_err(|err| anyhow!("failed to link fragment entry point: {:?}", err))?
            .link()
            .map_err(|err| anyhow!("failed to finalize fragment shader: {:?}", err))?;
        let fragment_blob = fragment_program
            .entry_point_code(0, 0)
            .map_err(|err| anyhow!("failed to get fragment shader code: {:?}", err))?;

        Ok((
            Self::blob_to_words(&vertex_blob)?,
            Self::blob_to_words(&fragment_blob)?,
        ))
    }

    fn blob_to_words(blob: &slang::Blob) -> Result<Vec<u32>> {
        let bytes = blob.as_slice();
        if bytes.len() % 4 != 0 {
            return Err(anyhow!("shader bytecode size is not 4-byte aligned"));
        }
        if bytes.len() < 4 {
            return Err(anyhow!("shader bytecode is empty"));
        }
        let mut words = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            words.push(word);
        }
        if words.first().copied() != Some(0x07230203) {
            if let Ok(text) = std::str::from_utf8(bytes) {
                return Err(anyhow!(
                    "shader bytecode is not SPIR-V; first bytes: {:?}",
                    &text[..text.len().min(64)]
                ));
            }
            return Err(anyhow!("shader bytecode is not SPIR-V"));
        }
        Ok(words.to_vec())
    }

    fn create_shader_module(&self, code: &[u32]) -> Result<vk::ShaderModule> {
        if code.is_empty() {
            return Err(anyhow!("shader module bytecode is empty"));
        }
        let info = vk::ShaderModuleCreateInfo::builder()
            .code(code)
            .code_size(code.len() * std::mem::size_of::<u32>());
        let module = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_shader_module(&info, None)
                .context("failed to create shader module")?
        };
        Ok(module)
    }

    fn create_image(
        &self,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        allocation_options: &vma::AllocationOptions,
    ) -> Result<(vk::Image, vma::Allocation)> {
        let info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let (image, allocation) =
            unsafe { self.allocator().create_image(info, allocation_options) }
                .map_err(|err| anyhow!(err))
                .context("failed to create VMA image")?;
        Ok((image, allocation))
    }

    fn create_buffer(
        &self,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        allocation_options: &vma::AllocationOptions,
    ) -> Result<(vk::Buffer, vma::Allocation)> {
        let buffer_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let (buffer, allocation) = unsafe {
            self.allocator()
                .create_buffer(buffer_info, allocation_options)
        }
        .map_err(|err| anyhow!(err))
        .context("failed to create VMA buffer")?;
        Ok((buffer, allocation))
    }

    fn write_memory<T: Copy>(&self, allocation: vma::Allocation, data: &[T]) -> Result<()> {
        let size = (data.len() * std::mem::size_of::<T>()) as vk::DeviceSize;
        unsafe {
            let ptr = self
                .allocator()
                .map_memory(allocation)
                .map_err(|err| anyhow!(err))
                .context("failed to map allocation")?;
            std::ptr::copy_nonoverlapping(
                data.as_ptr() as *const u8,
                ptr as *mut u8,
                size as usize,
            );
            self.allocator()
                .flush_allocation(allocation, 0, size)
                .map_err(|err| anyhow!(err))
                .context("failed to flush allocation")?;
            self.allocator().unmap_memory(allocation);
        }
        Ok(())
    }

    fn submit_immediate<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(vk::CommandBuffer) -> Result<()>,
    {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe {
            self.gpu_device
                .get_vk_device()
                .allocate_command_buffers(&alloc_info)
                .context("failed to allocate immediate command buffer")?
        }[0];
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.gpu_device
                .get_vk_device()
                .begin_command_buffer(command_buffer, &begin_info)
                .context("failed to begin immediate command buffer")?;
        }
        f(command_buffer)?;
        unsafe {
            self.gpu_device
                .get_vk_device()
                .end_command_buffer(command_buffer)
                .context("failed to end immediate command buffer")?;
        }
        let submit_info =
            [vk::SubmitInfo::builder().command_buffers(std::slice::from_ref(&command_buffer))];
        unsafe {
            self.gpu_device
                .get_vk_device()
                .queue_submit(self.graphics_queue, &submit_info, vk::Fence::null())
                .context("failed to submit immediate command buffer")?;
            self.gpu_device
                .get_vk_device()
                .queue_wait_idle(self.graphics_queue)
                .context("failed to wait for immediate command buffer")?;
            self.gpu_device
                .get_vk_device()
                .free_command_buffers(self.command_pool, &[command_buffer]);
        }
        Ok(())
    }

    fn allocator(&self) -> &vma::Allocator {
        self.allocator
            .as_ref()
            .expect("VMA allocator must be initialized")
    }

    fn select_depth_format(&self) -> Result<vk::Format> {
        let candidates = [
            vk::Format::D32_SFLOAT,
            vk::Format::D32_SFLOAT_S8_UINT,
            vk::Format::D24_UNORM_S8_UINT,
        ];
        let instance = self.gpu_instance.get_vk_instance();
        for format in candidates {
            let props = unsafe {
                instance.get_physical_device_format_properties(self.physical_device, format)
            };
            if props
                .optimal_tiling_features
                .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
            {
                return Ok(format);
            }
        }
        Err(anyhow!("failed to find supported depth format"))
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

            if self.descriptor_pool != vk::DescriptorPool::null() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
            }

            if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
                self.descriptor_set_layout = vk::DescriptorSetLayout::null();
            }

            if self.pipeline_layout != vk::PipelineLayout::null() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_pipeline_layout(self.pipeline_layout, None);
                self.pipeline_layout = vk::PipelineLayout::null();
            }

            if self.texture_sampler != vk::Sampler::null() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_sampler(self.texture_sampler, None);
                self.texture_sampler = vk::Sampler::null();
            }

            if self.texture_image_view != vk::ImageView::null() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_image_view(self.texture_image_view, None);
                self.texture_image_view = vk::ImageView::null();
            }

            if self.texture_image != vk::Image::null() {
                if let Some(allocation) = self.texture_image_allocation.take() {
                    self.allocator()
                        .destroy_image(self.texture_image, allocation);
                } else {
                    self.gpu_device
                        .get_vk_device()
                        .destroy_image(self.texture_image, None);
                }
                self.texture_image = vk::Image::null();
            }

            self.texture_image_allocation = None;

            if self.vertex_buffer != vk::Buffer::null() {
                if let Some(allocation) = self.vertex_buffer_allocation.take() {
                    self.allocator()
                        .destroy_buffer(self.vertex_buffer, allocation);
                } else {
                    self.gpu_device
                        .get_vk_device()
                        .destroy_buffer(self.vertex_buffer, None);
                }
                self.vertex_buffer = vk::Buffer::null();
            }

            self.vertex_buffer_allocation = None;

            if self.index_buffer != vk::Buffer::null() {
                if let Some(allocation) = self.index_buffer_allocation.take() {
                    self.allocator()
                        .destroy_buffer(self.index_buffer, allocation);
                } else {
                    self.gpu_device
                        .get_vk_device()
                        .destroy_buffer(self.index_buffer, None);
                }
                self.index_buffer = vk::Buffer::null();
            }

            self.index_buffer_allocation = None;

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

        if let Some(allocator) = self.allocator.take() {
            drop(allocator);
        }
    }
}
