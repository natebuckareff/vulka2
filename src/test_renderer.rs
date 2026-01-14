use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use vulkano::{
    VulkanLibrary,
    command_buffer::allocator::StandardCommandBufferAllocator,
    command_buffer::{
        AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo, SubpassBeginInfo,
        SubpassEndInfo,
    },
    device::{Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo, QueueFlags},
    image::ImageUsage,
    image::view::ImageView,
    instance::{Instance, InstanceCreateInfo, InstanceExtensions},
    render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass},
    swapchain::{
        PresentMode, Surface, SurfaceInfo, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo,
        acquire_next_image,
    },
    sync::{self, GpuFuture},
};
use winit::{dpi::PhysicalSize, window::Window};

pub struct Renderer {
    library: Arc<VulkanLibrary>,
    instance: Option<Arc<Instance>>,
    surface: Option<Arc<Surface>>,
    device: Option<Arc<Device>>,
    queue: Option<Arc<Queue>>,
    swapchain: Option<Arc<Swapchain>>,
    render_pass: Option<Arc<RenderPass>>,
    framebuffers: Vec<Arc<Framebuffer>>,
    command_buffer_allocator: Option<Arc<StandardCommandBufferAllocator>>,
    recreate_swapchain: bool,
    window_size: [u32; 2],
    previous_frame_end: Option<Box<dyn GpuFuture>>,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        let library = VulkanLibrary::new()?;
        let mut renderer = Self {
            library,
            instance: None,
            surface: None,
            device: None,
            queue: None,
            swapchain: None,
            render_pass: None,
            framebuffers: Vec::new(),
            command_buffer_allocator: None,
            recreate_swapchain: false,
            window_size: [0, 0],
            previous_frame_end: None,
        };

        let required_extensions =
            Surface::required_extensions(&window).context("failed to query surface extensions")?;
        let instance = Instance::new(
            renderer.library.clone(),
            InstanceCreateInfo {
                application_name: Some("voxels2".to_string()),
                max_api_version: Some(vulkano::Version::V1_3),
                enabled_extensions: InstanceExtensions {
                    khr_get_surface_capabilities2: true,
                    ..required_extensions
                },
                ..Default::default()
            },
        )?;

        let surface = Surface::from_window(instance.clone(), window.clone())
            .context("failed to create window surface")?;

        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()?
            .filter_map(|physical_device| {
                let queue_family_index = physical_device
                    .queue_family_properties()
                    .iter()
                    .enumerate()
                    .find(|(index, queue_family)| {
                        queue_family.queue_flags.contains(QueueFlags::GRAPHICS)
                            && physical_device
                                .surface_support(*index as u32, &surface)
                                .unwrap_or(false)
                    })
                    .map(|(index, _)| index as u32);

                queue_family_index.map(|index| (physical_device, index))
            })
            .next()
            .ok_or_else(|| anyhow!("no suitable physical device found"))?;

        let (device, mut queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                enabled_extensions: DeviceExtensions {
                    khr_swapchain: true,
                    ..DeviceExtensions::empty()
                },
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )?;
        let queue = queues.next().context("failed to create graphics queue")?;

        renderer.window_size = window.inner_size().into();
        renderer.instance = Some(instance);
        renderer.surface = Some(surface);
        renderer.device = Some(device.clone());
        renderer.queue = Some(queue);
        renderer.command_buffer_allocator = Some(Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            Default::default(),
        )));
        renderer.previous_frame_end = Some(Box::new(sync::now(device)));
        renderer.recreate_swapchain = true;

        renderer.recreate_swapchain_if_needed()?;
        Ok(renderer)
    }

    pub fn resized_window(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        self.window_size = [size.width, size.height];
        self.recreate_swapchain = true;
        Ok(())
    }

    pub fn render_frame(&mut self) -> Result<()> {
        let device = match self.device.as_ref() {
            Some(device) => device.clone(),
            None => return Ok(()),
        };

        if let Some(previous_frame_end) = &mut self.previous_frame_end {
            previous_frame_end.cleanup_finished();
        }

        if self.recreate_swapchain {
            self.recreate_swapchain_if_needed()?;
        }

        let swapchain = match self.swapchain.as_ref() {
            Some(swapchain) => swapchain.clone(),
            None => return Ok(()),
        };
        let queue = self.queue.as_ref().context("missing queue")?.clone();
        let command_buffer_allocator = self
            .command_buffer_allocator
            .as_ref()
            .context("missing command buffer allocator")?
            .clone();

        let (image_index, suboptimal, acquire_future) =
            match acquire_next_image(swapchain.clone(), None) {
                Ok(result) => result,
                Err(vulkano::Validated::Error(vulkano::VulkanError::OutOfDate)) => {
                    self.recreate_swapchain = true;
                    return Ok(());
                }
                Err(err) => return Err(anyhow!(err)),
            };

        if suboptimal {
            self.recreate_swapchain = true;
        }

        let framebuffer = self
            .framebuffers
            .get(image_index as usize)
            .context("missing framebuffer for swapchain image")?
            .clone();

        let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
            command_buffer_allocator,
            queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;

        let mut render_pass_info = RenderPassBeginInfo::framebuffer(framebuffer);
        render_pass_info.clear_values = vec![Some([1.0, 0.0, 0.0, 1.0].into())];

        command_buffer_builder.begin_render_pass(render_pass_info, SubpassBeginInfo::default())?;
        command_buffer_builder.end_render_pass(SubpassEndInfo::default())?;

        let command_buffer = command_buffer_builder.build()?;

        let future = self
            .previous_frame_end
            .take()
            .unwrap_or_else(|| Box::new(sync::now(device.clone())))
            .join(acquire_future)
            .then_execute(queue.clone(), command_buffer)?
            .then_swapchain_present(
                queue,
                SwapchainPresentInfo::swapchain_image_index(swapchain, image_index),
            )
            .then_signal_fence_and_flush();

        match future {
            Ok(future) => {
                self.previous_frame_end = Some(Box::new(future));
            }
            Err(vulkano::Validated::Error(vulkano::VulkanError::OutOfDate)) => {
                self.recreate_swapchain = true;
                self.previous_frame_end = Some(Box::new(sync::now(device)));
            }
            Err(err) => return Err(anyhow!(err)),
        }

        Ok(())
    }

    fn recreate_swapchain_if_needed(&mut self) -> Result<()> {
        if !self.recreate_swapchain {
            return Ok(());
        }

        let device = self.device.as_ref().context("missing device")?.clone();
        let surface = self.surface.as_ref().context("missing surface")?.clone();
        let physical_device = device.physical_device().clone();

        if self.window_size[0] == 0 || self.window_size[1] == 0 {
            return Ok(());
        }

        let surface_info = SurfaceInfo::default();
        let caps = physical_device.surface_capabilities(&surface, surface_info.clone())?;
        let formats = physical_device.surface_formats(&surface, surface_info.clone())?;
        let present_modes = physical_device.surface_present_modes(&surface, surface_info)?;

        let (image_format, image_color_space) = formats
            .first()
            .context("no surface formats available")?
            .to_owned();

        let mut image_extent = caps.current_extent.unwrap_or(self.window_size);
        image_extent[0] = image_extent[0].clamp(caps.min_image_extent[0], caps.max_image_extent[0]);
        image_extent[1] = image_extent[1].clamp(caps.min_image_extent[1], caps.max_image_extent[1]);

        let min_image_count = caps.min_image_count + 1;
        let min_image_count = if let Some(max_image_count) = caps.max_image_count {
            min_image_count.min(max_image_count)
        } else {
            min_image_count
        };

        let composite_alpha = [
            vulkano::swapchain::CompositeAlpha::Opaque,
            vulkano::swapchain::CompositeAlpha::PreMultiplied,
            vulkano::swapchain::CompositeAlpha::PostMultiplied,
            vulkano::swapchain::CompositeAlpha::Inherit,
        ]
        .into_iter()
        .find(|alpha| caps.supported_composite_alpha.contains_enum(*alpha))
        .context("no supported composite alpha")?;

        let present_mode = if present_modes.contains(&PresentMode::Fifo) {
            PresentMode::Fifo
        } else {
            *present_modes
                .first()
                .context("no supported present modes")?
        };

        let (swapchain, images) = if let Some(swapchain) = &self.swapchain {
            swapchain.recreate(SwapchainCreateInfo {
                min_image_count,
                image_format,
                image_color_space,
                image_extent,
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                composite_alpha,
                present_mode,
                ..swapchain.create_info()
            })?
        } else {
            Swapchain::new(
                device.clone(),
                surface,
                SwapchainCreateInfo {
                    min_image_count,
                    image_format,
                    image_color_space,
                    image_extent,
                    image_usage: ImageUsage::COLOR_ATTACHMENT,
                    composite_alpha,
                    present_mode,
                    ..Default::default()
                },
            )?
        };

        let render_pass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                color: {
                    format: image_format,
                    samples: 1,
                    load_op: Clear,
                    store_op: Store,
                },
            },
            pass: {
                color: [color],
                depth_stencil: {},
            },
        )?;

        let framebuffers = images
            .iter()
            .map(|image| {
                let view = ImageView::new_default(image.clone())?;
                Framebuffer::new(
                    render_pass.clone(),
                    FramebufferCreateInfo {
                        attachments: vec![view],
                        ..Default::default()
                    },
                )
                .map_err(|err| err.into())
            })
            .collect::<Result<Vec<_>>>()?;

        self.swapchain = Some(swapchain);
        self.render_pass = Some(render_pass);
        self.framebuffers = framebuffers;
        self.recreate_swapchain = false;

        Ok(())
    }
}
