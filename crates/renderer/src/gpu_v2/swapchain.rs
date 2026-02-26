use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk;

use crate::gpu_v2::{
    Device, Fence, LaneIndex, OwnedImageView, OwnedSemaphore, OwnedSwapchain, QueueGroup,
    QueueGroupId, QueueRoleFlags, ResourceArena, VulkanHandle,
};

struct Slot {
    image_available: VulkanHandle<vk::Semaphore>,
    render_finished: VulkanHandle<vk::Semaphore>,
    in_flight: Fence,
}

pub struct AcquiredImage {
    generation: u64,
    index: u32,
    // XXX: this is a raw handle because it is managed internall by the
    // vk::Swapchain; should think about how to make this safer
    image: vk::Image,
    view: VulkanHandle<vk::ImageView>,
    extent: vk::Extent2D,
    format: vk::Format,
    image_available: VulkanHandle<vk::Semaphore>,
    render_finished: VulkanHandle<vk::Semaphore>,
    in_flight: Fence,
}

// TODO: wip
pub struct PresentToken {
    generation: u64,
    index: u32,
    render_finished: VulkanHandle<vk::Semaphore>,
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

pub struct Swapchain {
    device: Arc<Device>,
    queue_group_id: QueueGroupId,
    lane_index: LaneIndex,
    arena: ResourceArena,
    surface: VulkanHandle<vk::SurfaceKHR>,
    generation: u64,
    resources: SwapchainResources,
    slots: Vec<Slot>,
    slot_index: usize,
    should_recreate: bool,
}

impl Swapchain {
    fn new(
        device: Arc<Device>,
        queue_group: &QueueGroup,
        extent: vk::Extent2D,
        frames_in_flight: usize,
    ) -> Result<Self> {
        let mut lane_index = None;
        for (index, queue) in queue_group.queues().iter_entries() {
            if queue.roles().contains(QueueRoleFlags::PRESENT) {
                lane_index = Some(index);
                break;
            }
        }
        if extent.width == 0 || extent.height == 0 {
            return Err(anyhow!("extent is zero"));
        }
        if frames_in_flight == 0 {
            return Err(anyhow!("frames in flight must be at least 1"));
        }
        let Some(lane_index) = lane_index else {
            return Err(anyhow!("no presentable queue found"));
        };
        let Some(surface) = device.engine().surface() else {
            return Err(anyhow!("surface not found"));
        };
        let arena = ResourceArena::new("swapchain");
        let surface = surface.clone();
        let slots = Self::create_slots(device.handle().clone(), &arena, frames_in_flight)?;
        let resources = SwapchainResources::new(&device, &surface, &arena, extent, None)?;
        Ok(Self {
            device,
            queue_group_id: queue_group.id(),
            lane_index,
            arena,
            surface,
            generation: 0,
            slots,
            slot_index: 0,
            resources,
            should_recreate: false,
        })
    }

    fn create_slots(
        device: VulkanHandle<Arc<vulkanalia::Device>>,
        arena: &ResourceArena,
        frames_in_flight: usize,
    ) -> Result<Vec<Slot>> {
        use vulkanalia::prelude::v1_0::*;
        let info = vk::SemaphoreCreateInfo::builder();
        let mut slots = Vec::with_capacity(frames_in_flight);
        for _ in 0..frames_in_flight {
            let image_available = OwnedSemaphore::new(device.clone(), &info)?;
            let image_available = arena.add(image_available)?;
            let render_finished = OwnedSemaphore::new(device.clone(), &info)?;
            let render_finished = arena.add(render_finished)?;
            let in_flight = Fence::new(device.clone(), arena)?;
            let slot = Slot {
                image_available,
                render_finished,
                in_flight,
            };
            slots.push(slot);
        }
        Ok(slots)
    }

    pub fn should_recreate(&self) -> bool {
        self.should_recreate
    }

    pub fn recreate(&mut self, queue_group: &QueueGroup, extent: vk::Extent2D) -> Result<()> {
        if self.queue_group_id != queue_group.id() {
            return Err(anyhow!("queue group mismatch"));
        }

        if extent.width == 0 || extent.height == 0 {
            return Err(anyhow!("extent is zero"));
        }

        for slot in &mut self.slots {
            slot.in_flight.wait()?;
        }

        let queue = queue_group.queues().get(self.lane_index);
        queue.wait_idle()?;

        let device = self.device.handle().clone();
        let arena = ResourceArena::new("swapchain");
        let old = Some(&self.resources);

        let slots = Self::create_slots(device, &arena, self.slots.len())?;
        let resources = SwapchainResources::new(&self.device, &self.surface, &arena, extent, old)?;

        // replace old resources with new
        self.generation += 1;
        self.slots = slots;
        self.resources = resources;
        self.arena = arena;
        self.should_recreate = false;
        Ok(())
    }

    pub fn acquire(&mut self) -> Result<AcquiredImage, AcquireError> {
        match self.acquire_inner() {
            Ok(image) => {
                self.slot_index = (self.slot_index + 1) % self.slots.len();
                Ok(image)
            }
            Err(e) => Err(e),
        }
    }

    fn acquire_inner(&mut self) -> Result<AcquiredImage, AcquireError> {
        use vulkanalia::prelude::v1_0::*;
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;

        let slot = &mut self.slots[self.slot_index];

        // TODO: should probably use timeouts

        slot.in_flight.wait().map_err(AcquireError::Other)?;

        // TODO: remaining timeout

        let result = unsafe {
            self.device.handle().raw().acquire_next_image_khr(
                *self.resources.swapchain.raw(),
                u64::MAX,
                *slot.image_available.raw(),
                vk::Fence::null(),
            )
        };

        match result {
            Ok((index, code)) => {
                if code == vk::SuccessCode::SUBOPTIMAL_KHR {
                    self.should_recreate = true;
                }

                if code == vk::SuccessCode::SUCCESS {
                    Ok(AcquiredImage {
                        generation: self.generation,
                        index,
                        image: self.resources.images[index as usize],
                        view: self.resources.views[index as usize].clone(),
                        extent: self.resources.extent,
                        format: self.resources.format,
                        image_available: slot.image_available.clone(),
                        render_finished: slot.render_finished.clone(),
                        in_flight: slot.in_flight.clone(),
                    })
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
            Err(e) => Err(AcquireError::Code(e)),
        }
    }

    // TODO: wip
    pub fn present(&self, token: PresentToken) -> Result<()> {
        if token.generation != self.generation {
            return Err(anyhow!("generation mismatch"));
        }

        todo!()
    }
}

struct SurfaceSupport {
    capabilities: vk::SurfaceCapabilitiesKHR,
    formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
}

impl SurfaceSupport {
    fn new(device: &Device, surface: &VulkanHandle<vk::SurfaceKHR>) -> Result<Self> {
        use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;

        let instance = device.engine().instance();
        let physical_device = device.info().physical_device;

        let capabilities = unsafe {
            instance
                .raw()
                .get_physical_device_surface_capabilities_khr(physical_device, *surface.raw())?
        };

        let formats = unsafe {
            instance
                .raw()
                .get_physical_device_surface_formats_khr(physical_device, *surface.raw())?
        };

        let present_modes = unsafe {
            instance
                .raw()
                .get_physical_device_surface_present_modes_khr(physical_device, *surface.raw())?
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

struct SwapchainResources {
    format: vk::Format,
    extent: vk::Extent2D,
    swapchain: VulkanHandle<vk::SwapchainKHR>,
    images: Vec<vk::Image>,
    views: Vec<VulkanHandle<vk::ImageView>>,
}

impl SwapchainResources {
    fn new(
        device: &Device,
        surface: &VulkanHandle<vk::SurfaceKHR>,
        arena: &ResourceArena,
        extent: vk::Extent2D,
        old: Option<&SwapchainResources>,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;

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
            .surface(unsafe { *surface.raw() })
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

        if let Some(old) = &old {
            info = info.old_swapchain(unsafe { *old.swapchain.raw() });
        }

        let device = device.handle();
        let swapchain = OwnedSwapchain::new(device.clone(), &info)?;
        let swapchain = arena.add(swapchain)?;

        let images = unsafe { device.raw().get_swapchain_images_khr(*swapchain.raw())? };
        let mut views = Vec::with_capacity(images.len());

        let components = vk::ComponentMapping::builder()
            .r(vk::ComponentSwizzle::IDENTITY)
            .g(vk::ComponentSwizzle::IDENTITY)
            .b(vk::ComponentSwizzle::IDENTITY)
            .a(vk::ComponentSwizzle::IDENTITY);

        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };

        for image in &images {
            let info = vk::ImageViewCreateInfo::builder()
                .image(*image)
                .view_type(vk::ImageViewType::_2D)
                .format(image_format)
                .components(components)
                .subresource_range(range);

            let view = OwnedImageView::new(device.clone(), &info)?;
            let view = arena.add(view)?;

            views.push(view);
        }

        Ok(Self {
            format: image_format,
            extent,
            swapchain,
            images,
            views,
        })
    }
}
