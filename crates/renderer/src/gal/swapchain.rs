use std::sync::Arc;

use anyhow::{Result, bail};
use vulkanalia::vk;

use crate::gal::{
    device::{Device, DeviceResource},
    image::{Image, SampleCount},
    image_view::ImageView,
    queue::Queue,
    semaphore_resource::SemaphoreResource,
    surface::Surface,
    swapchain_image::AcquiredImage,
    swapchain_resource::SwapchainResource,
};

struct SwapchainSlot {
    image_available: Arc<SemaphoreResource>,
    render_finished: Arc<SemaphoreResource>,
}

pub enum AcquireError {
    Timeout,
    NotReady,
    RecreateSwapchain,
    RecreateSurface,
    RegainFullScreen,
    UnknownSuccessCode(vk::SuccessCode),
    Code(vk::ErrorCode),
    Other(anyhow::Error),
}

pub enum PresentError {
    QueueNotPresentable,
    RecreateSwapchain,
    RecreateSurface,
    RegainFullScreen,
    GenerationMismatch,
    Code(vk::ErrorCode),
}

pub struct Swapchain {
    device: Arc<Device>,
    surface: Arc<Surface>,
    generation: u64,
    state: SwapchainState,
    slots: Vec<SwapchainSlot>,
    slot_index: usize,
    should_recreate: bool,
}

impl Swapchain {
    pub fn new(device: Arc<Device>, surface: Arc<Surface>, extent: vk::Extent2D) -> Result<Self> {
        if extent.width == 0 || extent.height == 0 {
            bail!("extent is zero");
        }

        let state = SwapchainState::new(&device, &surface, extent, None)?;
        let slots = create_slots(device.resource().clone(), state.images.len())?;

        Ok(Self {
            device,
            surface,
            generation: 0,
            state,
            slots,
            slot_index: 0,
            should_recreate: false,
        })
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) unsafe fn current_handle(&self) -> vk::SwapchainKHR {
        unsafe { self.state.swapchain.handle() }
    }

    pub(crate) fn set_should_recreate(&mut self) {
        self.should_recreate = true;
    }

    pub fn should_recreate(&self) -> bool {
        self.should_recreate
    }

    pub fn format(&self) -> vk::Format {
        self.state.format
    }

    pub fn extent(&self) -> vk::Extent2D {
        self.state.extent
    }

    pub fn recreate(&mut self, queue: &Queue, extent: vk::Extent2D) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;

        if extent.width == 0 || extent.height == 0 {
            bail!("extent is zero");
        }

        if !queue.presentable() {
            bail!("queue does not support present");
        }

        unsafe {
            self.device
                .resource()
                .handle()
                .queue_wait_idle(queue.resource().handle())?;
        }

        let old = Some(&self.state);
        self.state = SwapchainState::new(&self.device, &self.surface, extent, old)?;
        self.slots = create_slots(self.device.resource().clone(), self.state.images.len())?;
        self.generation += 1;
        self.slot_index = 0;
        self.should_recreate = false;
        Ok(())
    }

    pub fn acquire(&mut self) -> Result<AcquiredImage, AcquireError> {
        use vulkanalia::prelude::v1_0::*;
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;

        let slot = &self.slots[self.slot_index];
        let result = unsafe {
            self.device.resource().handle().acquire_next_image_khr(
                self.state.swapchain.handle(),
                u64::MAX,
                slot.image_available.handle(),
                vk::Fence::null(),
            )
        };

        match result {
            Ok((index, code)) => {
                self.slot_index = (self.slot_index + 1) % self.slots.len();

                if code == vk::SuccessCode::SUBOPTIMAL_KHR {
                    self.should_recreate = true;
                }

                if code == vk::SuccessCode::SUCCESS || code == vk::SuccessCode::SUBOPTIMAL_KHR {
                    Ok(AcquiredImage::new(
                        self.generation,
                        index,
                        self.state.images[index as usize].clone(),
                        self.state.views[index as usize].clone(),
                        self.state.extent,
                        self.state.format,
                        slot.image_available.clone(),
                        slot.render_finished.clone(),
                    ))
                } else if code == vk::SuccessCode::TIMEOUT {
                    Err(AcquireError::Timeout)
                } else if code == vk::SuccessCode::NOT_READY {
                    Err(AcquireError::NotReady)
                } else {
                    Err(AcquireError::UnknownSuccessCode(code))
                }
            }
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => Err(AcquireError::RecreateSwapchain),
            Err(vk::ErrorCode::SURFACE_LOST_KHR) => Err(AcquireError::RecreateSurface),
            Err(vk::ErrorCode::FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT) => {
                Err(AcquireError::RegainFullScreen)
            }
            Err(error) => Err(AcquireError::Code(error)),
        }
    }
}

fn create_slots(device: Arc<DeviceResource>, count: usize) -> Result<Vec<SwapchainSlot>> {
    let mut slots = Vec::with_capacity(count);

    for _ in 0..count {
        let image_available = Arc::new(SemaphoreResource::binary(device.clone())?);
        let render_finished = Arc::new(SemaphoreResource::binary(device.clone())?);
        slots.push(SwapchainSlot {
            image_available,
            render_finished,
        });
    }

    Ok(slots)
}

struct SwapchainState {
    format: vk::Format,
    extent: vk::Extent2D,
    swapchain: Arc<SwapchainResource>,
    images: Vec<Arc<Image>>,
    views: Vec<Arc<ImageView>>,
}

impl SwapchainState {
    fn new(
        device: &Arc<Device>,
        surface: &Arc<Surface>,
        extent: vk::Extent2D,
        old: Option<&SwapchainState>,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;

        let surface_info = surface.info(device)?;
        let surface_format = surface_info.get_best_surface_format();
        let format = surface_format.format;
        let extent = surface_info.get_clamped_extent(extent);

        let mut create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(unsafe { surface.handle() })
            .min_image_count(surface_info.get_min_image_count())
            .image_format(format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(surface_info.pre_transform())
            .composite_alpha(surface_info.composite_alpha()?)
            .present_mode(surface_info.get_present_mode()?)
            .clipped(true);

        if let Some(old) = old {
            create_info = create_info.old_swapchain(unsafe { old.swapchain.handle() });
        }

        let device_handle = unsafe { device.resource().handle() };
        let device_resource = device.resource();
        let swapchain = Arc::new(SwapchainResource::new(
            device_resource.clone(),
            &create_info,
        )?);
        let swapchain_images =
            unsafe { device_handle.get_swapchain_images_khr(swapchain.handle())? };
        let mut images = Vec::with_capacity(swapchain_images.len());
        let mut views = Vec::with_capacity(swapchain_images.len());

        let components = vk::ComponentMapping::builder()
            .r(vk::ComponentSwizzle::IDENTITY)
            .g(vk::ComponentSwizzle::IDENTITY)
            .b(vk::ComponentSwizzle::IDENTITY)
            .a(vk::ComponentSwizzle::IDENTITY)
            .build();

        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        let sample_count = SampleCount::new(1)?;
        let image_extent = vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        };

        for handle in swapchain_images {
            let image = Arc::new(Image::from_swapchain(
                swapchain.clone(),
                handle,
                vk::ImageType::_2D,
                format,
                image_extent,
                1,
                1,
                sample_count,
                vk::ImageTiling::OPTIMAL,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            let view = Arc::new(ImageView::new(
                device.clone(),
                image.clone(),
                vk::ImageViewType::_2D,
                format,
                components,
                range,
            )?);
            images.push(image);
            views.push(view);
        }

        Ok(Self {
            format,
            extent,
            swapchain,
            images,
            views,
        })
    }
}
