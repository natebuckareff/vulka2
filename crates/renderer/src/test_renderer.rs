use std::any::Any;
use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::Arc;
use std::time::Instant;

use crate::gpu::DeviceAddress;
use crate::gpu::{
    GpuDevice, GpuDeviceFeatureExt, GpuDeviceFeatureV12, GpuDeviceFeatureV13,
    GpuDeviceRequestBuilder, GpuInstance, GpuQueueProfile, GpuQueueRequest, GpuSurface,
    GpuSwapchain,
};
use anyhow::{Context, Result, anyhow};
use crevice::std140::AsStd140;
use crevice::std430::AsStd430;
use glam::{Mat4, Vec2, Vec3};
use slang::{
    CursorLayout, CursorLayoutView, DescriptorBinding as SlangDescriptorBinding, DescriptorClass,
    DescriptorProfile, DescriptorSet as SlangDescriptorSet, ElementCount, NodeKind,
    ResourceDescriptor, ShaderCursor, ShaderLayout, ShaderObject, ShaderOffset,
    ShaderParameterBlock, ShaderResource, SlangCompilerBuilder, SlangProgram, SlangShaderStage,
    Type,
};
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;
use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
use vulkanalia_vma::{self as vma, Alloc};
use winit::dpi::PhysicalSize;
use winit::window::Window;

impl Vertex {
    fn new(position: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position: Vec3::from_array(position),
            uv: Vec2::from_array(uv),
        }
    }
}

#[repr(C)]
#[derive(AsStd430, Clone, Copy)]
struct Vertex {
    position: Vec3,
    uv: Vec2,
}

#[repr(C)]
#[derive(AsStd140, Clone, Copy)]
struct DrawData {
    mvp: Mat4,
    vertices: DeviceAddress,
    indices: DeviceAddress,
}

const TEXTURE_PATH: &str = "assets/debug-texture.png";
const FRAME_PARAMETER_BLOCK_NAME: &str = "frame";
const MATERIAL_PARAMETER_BLOCK_NAME: &str = "material";

#[derive(Clone)]
struct ParameterBlockLayoutInfo {
    name: String,
    set: u32,
    descriptor_set: SlangDescriptorSet,
}

#[derive(Clone, Copy)]
enum VkDescriptorValue {
    UniformBuffer(vk::DescriptorBufferInfo),
    StorageBuffer(vk::DescriptorBufferInfo),
    Sampler(vk::DescriptorImageInfo),
    SampledImage(vk::DescriptorImageInfo),
}

impl VkDescriptorValue {
    fn descriptor_type(self) -> vk::DescriptorType {
        match self {
            Self::UniformBuffer(_) => vk::DescriptorType::UNIFORM_BUFFER,
            Self::StorageBuffer(_) => vk::DescriptorType::STORAGE_BUFFER,
            Self::Sampler(_) => vk::DescriptorType::SAMPLER,
            Self::SampledImage(_) => vk::DescriptorType::SAMPLED_IMAGE,
        }
    }

    fn profile(self) -> DescriptorProfile {
        let class = match self {
            Self::UniformBuffer(_) => DescriptorClass::UniformBuffer,
            Self::StorageBuffer(_) => DescriptorClass::StorageBuffer,
            Self::Sampler(_) => DescriptorClass::Sampler,
            Self::SampledImage(_) => DescriptorClass::SampledImage,
        };
        DescriptorProfile {
            class,
            writable: false,
        }
    }
}

#[derive(Clone, Copy)]
struct VkResourceDescriptor {
    value: VkDescriptorValue,
}

impl ResourceDescriptor for VkResourceDescriptor {
    fn profile(&self) -> DescriptorProfile {
        self.value.profile()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone)]
struct BufferObject {
    allocator: Arc<vma::Allocator>,
    buffer: vk::Buffer,
    allocation: vma::Allocation,
    size: vk::DeviceSize,
    descriptor_class: DescriptorClass,
}

impl BufferObject {
    fn uniform(
        allocator: Arc<vma::Allocator>,
        buffer: vk::Buffer,
        allocation: vma::Allocation,
        size: vk::DeviceSize,
    ) -> Self {
        Self {
            allocator,
            buffer,
            allocation,
            size,
            descriptor_class: DescriptorClass::UniformBuffer,
        }
    }
}

impl ShaderResource for BufferObject {
    fn descriptor(&self) -> Box<dyn ResourceDescriptor> {
        let info = vk::DescriptorBufferInfo::builder()
            .buffer(self.buffer)
            .offset(0)
            .range(self.size)
            .build();
        let value = match self.descriptor_class {
            DescriptorClass::UniformBuffer => VkDescriptorValue::UniformBuffer(info),
            DescriptorClass::StorageBuffer => VkDescriptorValue::StorageBuffer(info),
            _ => VkDescriptorValue::StorageBuffer(info),
        };
        Box::new(VkResourceDescriptor { value })
    }
}

impl ShaderObject for BufferObject {
    fn as_shader_block(&mut self) -> Option<&mut dyn ShaderParameterBlock> {
        None
    }

    fn write(&mut self, offset: ShaderOffset, bytes: &[u8]) -> Result<()> {
        let start = offset.bytes as vk::DeviceSize;
        let end = start + bytes.len() as vk::DeviceSize;
        if end > self.size {
            return Err(anyhow!(
                "buffer write out of bounds: start={} end={} size={}",
                start,
                end,
                self.size
            ));
        }

        unsafe {
            let ptr = self
                .allocator
                .map_memory(self.allocation)
                .map_err(|err| anyhow!(err))
                .context("failed to map buffer allocation")?;
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                (ptr as *mut u8).add(offset.bytes),
                bytes.len(),
            );
            self.allocator
                .flush_allocation(self.allocation, start, bytes.len() as vk::DeviceSize)
                .map_err(|err| anyhow!(err))
                .context("failed to flush buffer allocation")?;
            self.allocator.unmap_memory(self.allocation);
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
struct SamplerObject {
    sampler: vk::Sampler,
}

impl ShaderResource for SamplerObject {
    fn descriptor(&self) -> Box<dyn ResourceDescriptor> {
        let info = vk::DescriptorImageInfo::builder()
            .sampler(self.sampler)
            .build();
        Box::new(VkResourceDescriptor {
            value: VkDescriptorValue::Sampler(info),
        })
    }
}

#[derive(Clone, Copy)]
struct ImageObject {
    image_view: vk::ImageView,
    image_layout: vk::ImageLayout,
}

impl ShaderResource for ImageObject {
    fn descriptor(&self) -> Box<dyn ResourceDescriptor> {
        let info = vk::DescriptorImageInfo::builder()
            .image_view(self.image_view)
            .image_layout(self.image_layout)
            .build();
        Box::new(VkResourceDescriptor {
            value: VkDescriptorValue::SampledImage(info),
        })
    }
}

#[derive(Clone)]
struct ParameterObject {
    gpu_device: Arc<GpuDevice>,
    descriptor_set: vk::DescriptorSet,
    descriptor_layout: SlangDescriptorSet,
}

impl ParameterObject {
    fn resolve_binding(&self, offset: ShaderOffset) -> Result<&SlangDescriptorBinding> {
        if let Some(binding_range) = self.descriptor_layout.find_binding_range(offset.binding_range) {
            return Ok(&binding_range.descriptor);
        }

        if offset.binding_range == 0 {
            if let Some(binding) = self.descriptor_layout.implicit_ubo.as_ref() {
                return Ok(binding);
            }
        }

        Err(anyhow!(
            "binding range {} not found in parameter block set {:?}",
            offset.binding_range,
            self.descriptor_layout.set
        ))
    }
}

impl ShaderObject for ParameterObject {
    fn as_shader_block(&mut self) -> Option<&mut dyn ShaderParameterBlock> {
        Some(self)
    }

    fn write(&mut self, _offset: ShaderOffset, _bytes: &[u8]) -> Result<()> {
        Err(anyhow!("parameter objects do not support byte writes"))
    }
}

impl ShaderParameterBlock for ParameterObject {
    fn bind(
        &mut self,
        offset: ShaderOffset,
        descriptor: Box<dyn ResourceDescriptor>,
    ) -> Result<()> {
        let binding = self.resolve_binding(offset)?;
        let count = match binding.count {
            ElementCount::Bounded(count) => count,
            ElementCount::Runtime => {
                return Err(anyhow!(
                    "runtime-sized descriptor arrays are not supported in test renderer"
                ));
            }
        };

        if offset.array_index >= count {
            return Err(anyhow!(
                "descriptor array index {} out of bounds for binding {} with count {}",
                offset.array_index,
                binding.binding,
                count
            ));
        }

        let vk_descriptor = descriptor
            .as_any()
            .downcast_ref::<VkResourceDescriptor>()
            .ok_or_else(|| anyhow!("unsupported resource descriptor implementation"))?;

        let descriptor_type = vk_descriptor.value.descriptor_type();
        if descriptor_type != binding.descriptor_type {
            return Err(anyhow!(
                "descriptor type mismatch for binding {}: expected {:?}, got {:?}",
                binding.binding,
                binding.descriptor_type,
                descriptor_type
            ));
        }
        if binding.binding < 0 {
            return Err(anyhow!(
                "reflected binding index {} is negative for descriptor set {:?}",
                binding.binding,
                self.descriptor_layout.set
            ));
        }
        let binding_index = binding.binding as u32;

        match vk_descriptor.value {
            VkDescriptorValue::UniformBuffer(info) | VkDescriptorValue::StorageBuffer(info) => {
                let infos = [info];
                let writes = [vk::WriteDescriptorSet::builder()
                    .dst_set(self.descriptor_set)
                    .dst_binding(binding_index)
                    .dst_array_element(offset.array_index as u32)
                    .descriptor_type(descriptor_type)
                    .buffer_info(&infos)
                    .build()];
                let copies: [vk::CopyDescriptorSet; 0] = [];
                unsafe {
                    self.gpu_device
                        .get_vk_device()
                        .update_descriptor_sets(&writes, &copies);
                }
            }
            VkDescriptorValue::Sampler(info) | VkDescriptorValue::SampledImage(info) => {
                let infos = [info];
                let writes = [vk::WriteDescriptorSet::builder()
                    .dst_set(self.descriptor_set)
                    .dst_binding(binding_index)
                    .dst_array_element(offset.array_index as u32)
                    .descriptor_type(descriptor_type)
                    .image_info(&infos)
                    .build()];
                let copies: [vk::CopyDescriptorSet; 0] = [];
                unsafe {
                    self.gpu_device
                        .get_vk_device()
                        .update_descriptor_sets(&writes, &copies);
                }
            }
        }

        Ok(())
    }
}

pub struct Renderer {
    gpu_instance: Arc<GpuInstance>,
    gpu_swapchain: GpuSwapchain,
    gpu_device: Arc<GpuDevice>,
    physical_device: vk::PhysicalDevice,
    allocator: Option<Arc<vma::Allocator>>,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_layouts: Vec<vk::ImageLayout>,
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fence: vk::Fence,
    slang_program: Option<Arc<SlangProgram>>,
    cursor_layout: Option<Arc<CursorLayout>>,
    parameter_blocks: Vec<ParameterBlockLayoutInfo>,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    frame_parameter: Option<ParameterObject>,
    material_parameter: Option<ParameterObject>,
    frame_uniform_object: Option<BufferObject>,
    texture_sampler_object: Option<SamplerObject>,
    texture_image_object: Option<ImageObject>,
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
    frame_uniform_buffer: vk::Buffer,
    frame_uniform_buffer_allocation: Option<vma::Allocation>,
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
        const VK_EXT_MUTABLE_DESCRIPTOR_TYPE: vk::ExtensionName =
            vk::ExtensionName::from_bytes(b"VK_EXT_mutable_descriptor_type");

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
            .required_extension(VK_EXT_MUTABLE_DESCRIPTOR_TYPE)
            // TODO: derive required_* extensions/features from SlangLayout once it exists.
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
            .required_feature_ext(GpuDeviceFeatureExt::MutableDescriptorType)
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
        let allocator = Arc::new(allocator);

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
            slang_program: None,
            cursor_layout: None,
            parameter_blocks: Vec::new(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            descriptor_set_layouts: Vec::new(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            frame_parameter: None,
            material_parameter: None,
            frame_uniform_object: None,
            texture_sampler_object: None,
            texture_image_object: None,
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
            frame_uniform_buffer: vk::Buffer::null(),
            frame_uniform_buffer_allocation: None,
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
        self.create_slang_program()?;
        self.create_descriptor_set_layouts()?;
        self.create_pipeline_layout()?;
        self.create_texture_resources()?;
        self.create_mesh_buffers()?;
        self.create_frame_uniform_buffer()?;
        self.create_descriptor_pool_and_set()?;
        self.bind_material_resources()?;
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
        self.update_frame_data(mvp)?;

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
                &self.descriptor_sets,
                &[],
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

    fn create_slang_program(&mut self) -> Result<()> {
        let shader_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders");
        let mut compiler = SlangCompilerBuilder::new()?.search_path(&shader_dir).build()?;
        let (vertex_entry, fragment_entry) = {
            let module = compiler.load_module("cube.slang")?;
            (
                module.entrypoint(SlangShaderStage::Vertex, "vertexMain")?,
                module.entrypoint(SlangShaderStage::Fragment, "fragmentMain")?,
            )
        };

        let program = compiler
            .linker()
            .add_entrypoint(vertex_entry)?
            .add_entrypoint(fragment_entry)?
            .link()?;

        let cursor_layout = Arc::new(CursorLayout::build(program.layout().clone())?);
        self.slang_program = Some(program);
        self.cursor_layout = Some(cursor_layout);
        Ok(())
    }

    fn create_descriptor_set_layouts(&mut self) -> Result<()> {
        let program = self
            .slang_program
            .as_ref()
            .context("missing Slang program")?;
        let blocks = Self::collect_parameter_blocks(program.layout())?;

        if !self.descriptor_set_layouts.is_empty() {
            for handle in self.descriptor_set_layouts.drain(..) {
                unsafe {
                    self.gpu_device
                        .get_vk_device()
                        .destroy_descriptor_set_layout(handle, None);
                }
            }
        }

        let mut descriptor_set_layouts = Vec::with_capacity(blocks.len());
        for block in &blocks {
            let mut bindings = Vec::new();
            for descriptor in Self::descriptor_bindings(&block.descriptor_set) {
                if descriptor.binding < 0 {
                    return Err(anyhow!(
                        "parameter block '{}' has negative descriptor binding {}",
                        block.name,
                        descriptor.binding
                    ));
                }
                bindings.push(
                    vk::DescriptorSetLayoutBinding::builder()
                        .binding(descriptor.binding as u32)
                        .descriptor_type(descriptor.descriptor_type)
                        .descriptor_count(Self::descriptor_count(&descriptor.count)?)
                        .stage_flags(descriptor.stages)
                        .build(),
                );
            }
            bindings.sort_by_key(|binding| binding.binding);

            let layout_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
            let layout_handle = unsafe {
                self.gpu_device
                    .get_vk_device()
                    .create_descriptor_set_layout(&layout_info, None)
                    .context("failed to create descriptor set layout")?
            };
            descriptor_set_layouts.push(layout_handle);
        }

        self.parameter_blocks = blocks;
        self.descriptor_set_layouts = descriptor_set_layouts;
        Ok(())
    }

    fn create_pipeline_layout(&mut self) -> Result<()> {
        let layout_info = vk::PipelineLayoutCreateInfo::builder().set_layouts(&self.descriptor_set_layouts);
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
        let mut pool_counts: HashMap<vk::DescriptorType, u32> = HashMap::new();
        for block in &self.parameter_blocks {
            for descriptor in Self::descriptor_bindings(&block.descriptor_set) {
                *pool_counts.entry(descriptor.descriptor_type).or_insert(0) +=
                    Self::descriptor_count(&descriptor.count)?;
            }
        }
        let pool_sizes: Vec<_> = pool_counts
            .iter()
            .map(|(descriptor_type, count)| vk::DescriptorPoolSize {
                type_: *descriptor_type,
                descriptor_count: *count,
            })
            .collect();
        let pool_info = vk::DescriptorPoolCreateInfo::builder()
            .max_sets(self.parameter_blocks.len() as u32)
            .pool_sizes(&pool_sizes);
        let pool = unsafe {
            self.gpu_device
                .get_vk_device()
                .create_descriptor_pool(&pool_info, None)
                .context("failed to create descriptor pool")?
        };
        self.descriptor_pool = pool;

        let alloc_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&self.descriptor_set_layouts);
        let sets = unsafe {
            self.gpu_device
                .get_vk_device()
                .allocate_descriptor_sets(&alloc_info)
                .context("failed to allocate descriptor sets")?
        };
        self.descriptor_sets = sets.to_vec();

        self.frame_parameter = None;
        self.material_parameter = None;
        for block in &self.parameter_blocks {
            let descriptor_set = *self
                .descriptor_sets
                .get(block.set as usize)
                .context("missing descriptor set for reflected set index")?;
            let parameter_object = ParameterObject {
                gpu_device: self.gpu_device.clone(),
                descriptor_set,
                descriptor_layout: block.descriptor_set.clone(),
            };

            match block.name.as_str() {
                FRAME_PARAMETER_BLOCK_NAME => self.frame_parameter = Some(parameter_object),
                MATERIAL_PARAMETER_BLOCK_NAME => self.material_parameter = Some(parameter_object),
                _ => {}
            }
        }

        if self.frame_parameter.is_none() {
            return Err(anyhow!("missing '{}' parameter block", FRAME_PARAMETER_BLOCK_NAME));
        }
        if self.material_parameter.is_none() {
            return Err(anyhow!(
                "missing '{}' parameter block",
                MATERIAL_PARAMETER_BLOCK_NAME
            ));
        }

        self.texture_sampler_object = Some(SamplerObject {
            sampler: self.texture_sampler,
        });
        self.texture_image_object = Some(ImageObject {
            image_view: self.texture_image_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        });

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
        let vertices_std430: Vec<<Vertex as AsStd430>::Output> =
            vertices.iter().map(Vertex::as_std430).collect();
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
            (vertices_std430.len() * std::mem::size_of::<<Vertex as AsStd430>::Output>())
                as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            &host_allocation,
        )?;
        self.write_memory(vertex_allocation, &vertices_std430)?;
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

        let program = self
            .slang_program
            .as_ref()
            .context("missing Slang program")?;
        let pipeline_program = program.select_graphics()?;
        let vertex_entry = pipeline_program
            .entrypoint(SlangShaderStage::Vertex)
            .context("missing vertex entry point")?;
        let fragment_entry = pipeline_program
            .entrypoint(SlangShaderStage::Fragment)
            .context("missing fragment entry point")?;
        let vertex_code = pipeline_program
            .code(SlangShaderStage::Vertex)
            .context("missing vertex SPIR-V")?;
        let fragment_code = pipeline_program
            .code(SlangShaderStage::Fragment)
            .context("missing fragment SPIR-V")?;

        let vertex_module = self.create_shader_module(vertex_code.as_ref())?;
        let fragment_module = self.create_shader_module(fragment_code.as_ref())?;
        let vertex_name = std::ffi::CString::new(vertex_entry.name())
            .context("vertex entrypoint name contains NUL")?;
        let fragment_name = std::ffi::CString::new(fragment_entry.name())
            .context("fragment entrypoint name contains NUL")?;
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

    fn create_frame_uniform_buffer(&mut self) -> Result<()> {
        let size = std::mem::size_of::<<DrawData as AsStd140>::Output>() as vk::DeviceSize;
        let allocation_options = vma::AllocationOptions {
            usage: vma::MemoryUsage::AutoPreferHost,
            flags: vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            ..Default::default()
        };
        let (buffer, allocation) =
            self.create_buffer(size, vk::BufferUsageFlags::UNIFORM_BUFFER, &allocation_options)?;

        self.frame_uniform_buffer = buffer;
        self.frame_uniform_buffer_allocation = Some(allocation);
        self.frame_uniform_object = Some(BufferObject::uniform(
            self.allocator_arc(),
            buffer,
            allocation,
            size,
        ));

        Ok(())
    }

    fn bind_material_resources(&mut self) -> Result<()> {
        let material_parameter = self
            .material_parameter
            .clone()
            .context("missing material parameter object")?;
        let sampler = self
            .texture_sampler_object
            .context("missing texture sampler object")?;
        let image = self
            .texture_image_object
            .context("missing texture image object")?;
        let material_view = self.parameter_block_root_view(MATERIAL_PARAMETER_BLOCK_NAME)?;

        let mut sampler_cursor =
            ShaderCursor::new(material_view.clone(), Box::new(material_parameter.clone()))
                .field("textureSampler")?;
        sampler_cursor.bind(&sampler)?;

        let mut image_cursor = ShaderCursor::new(material_view, Box::new(material_parameter))
            .field("texture")?;
        image_cursor.bind(&image)?;

        Ok(())
    }

    fn update_frame_data(&mut self, mvp: Mat4) -> Result<()> {
        let frame_parameter = self
            .frame_parameter
            .clone()
            .context("missing frame parameter object")?;
        let frame_uniform = self
            .frame_uniform_object
            .clone()
            .context("missing frame uniform object")?;
        let frame_view = self.parameter_block_root_view(FRAME_PARAMETER_BLOCK_NAME)?;
        let draw_data = DrawData {
            mvp,
            vertices: DeviceAddress(self.vertex_buffer_address),
            indices: DeviceAddress(self.index_buffer_address),
        };
        let draw_data_std140 = draw_data.as_std140();

        let frame_cursor = ShaderCursor::new(frame_view, Box::new(frame_parameter));
        let mut draw_cursor = frame_cursor
            .field("draw")?
            .bind_and_resolve(Box::new(frame_uniform))?;
        draw_cursor.write_bytes(bytemuck::bytes_of(&draw_data_std140))?;
        Ok(())
    }

    fn parameter_block_root_view(&self, name: &str) -> Result<CursorLayoutView> {
        let layout = self
            .cursor_layout
            .as_ref()
            .context("missing cursor layout")?;
        let global_view = layout.global_view().context("missing global cursor root")?;
        let field_view = global_view
            .field(name)
            .ok_or_else(|| anyhow!("global parameter block '{}' not found", name))?;

        let node = field_view
            .layout
            .node(field_view.node)
            .context("parameter block cursor node not found")?;
        let NodeKind::ParameterBlock { element, .. } = &node.kind else {
            return Err(anyhow!(
                "global field '{}' is not a parameter block in cursor layout",
                name
            ));
        };

        Ok(CursorLayoutView {
            layout: field_view.layout.clone(),
            node: *element,
            base: ShaderOffset::default(),
        })
    }

    fn collect_parameter_blocks(layout: &ShaderLayout) -> Result<Vec<ParameterBlockLayoutInfo>> {
        let globals = layout
            .globals
            .as_ref()
            .context("missing global reflection layout")?;
        let Type::Struct(root_struct) = &globals.value.ty else {
            return Err(anyhow!("global reflection root is not a struct"));
        };

        let mut blocks = Vec::new();
        for field in &root_struct.fields {
            let Some(name) = field.name.as_ref() else {
                continue;
            };
            let Type::ParameterBlock(pb) = &field.value.ty else {
                continue;
            };
            let set = pb
                .descriptor_set
                .set
                .ok_or_else(|| anyhow!("parameter block '{}' does not declare a set", name))?;
            if set < 0 {
                return Err(anyhow!("parameter block '{}' has negative set {}", name, set));
            }

            blocks.push(ParameterBlockLayoutInfo {
                name: name.to_string(),
                set: set as u32,
                descriptor_set: pb.descriptor_set.clone(),
            });
        }

        if blocks.is_empty() {
            return Err(anyhow!("no global parameter blocks were reflected"));
        }

        blocks.sort_by_key(|block| block.set);
        let max_set = blocks.last().map(|block| block.set).unwrap_or(0);
        if max_set as usize + 1 != blocks.len() {
            return Err(anyhow!(
                "parameter block set indices must be contiguous from 0"
            ));
        }

        Ok(blocks)
    }

    fn descriptor_bindings(descriptor_set: &SlangDescriptorSet) -> Vec<SlangDescriptorBinding> {
        let mut bindings = Vec::new();
        if let Some(implicit_ubo) = descriptor_set.implicit_ubo.clone() {
            bindings.push(implicit_ubo);
        }
        for range in &descriptor_set.binding_ranges {
            bindings.push(range.descriptor.clone());
        }
        bindings
    }

    fn descriptor_count(count: &ElementCount) -> Result<u32> {
        match count {
            ElementCount::Bounded(count) => Ok(*count as u32),
            ElementCount::Runtime => Err(anyhow!(
                "runtime-sized descriptor arrays are not supported in test renderer"
            )),
        }
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
            .as_ref()
    }

    fn allocator_arc(&self) -> Arc<vma::Allocator> {
        self.allocator
            .as_ref()
            .expect("VMA allocator must be initialized")
            .clone()
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
        self.frame_parameter = None;
        self.material_parameter = None;
        self.frame_uniform_object = None;
        self.texture_sampler_object = None;
        self.texture_image_object = None;
        self.parameter_blocks.clear();
        self.slang_program = None;
        self.cursor_layout = None;

        unsafe {
            let _ = self.gpu_device.get_vk_device().device_wait_idle();
            let _ = self.cleanup_swapchain_resources();

            if self.descriptor_pool != vk::DescriptorPool::null() {
                self.gpu_device
                    .get_vk_device()
                    .destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
            }
            self.descriptor_sets.clear();

            for layout in self.descriptor_set_layouts.drain(..) {
                self.gpu_device
                    .get_vk_device()
                    .destroy_descriptor_set_layout(layout, None);
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

            if self.frame_uniform_buffer != vk::Buffer::null() {
                if let Some(allocation) = self.frame_uniform_buffer_allocation.take() {
                    self.allocator()
                        .destroy_buffer(self.frame_uniform_buffer, allocation);
                } else {
                    self.gpu_device
                        .get_vk_device()
                        .destroy_buffer(self.frame_uniform_buffer, None);
                }
                self.frame_uniform_buffer = vk::Buffer::null();
            }

            self.frame_uniform_buffer_allocation = None;

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
