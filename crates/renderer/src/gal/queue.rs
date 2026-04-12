use std::sync::Arc;

use anyhow::Result;
use bitflags::bitflags;

use crate::gal::{
    command_pool::Submission,
    device::DeviceResource,
    device_builder::QueueKind,
    queue_resource::QueueResource,
    semaphore_resource::SemaphoreResource,
    swapchain::{PresentError, Swapchain},
    swapchain_image::PresentToken,
};

pub struct Queue {
    device: Arc<DeviceResource>,
    semaphore: Arc<SemaphoreResource>,
    resource: QueueResource,
    kind: QueueKind,
    present: bool,
    lane: Lane,
}

impl Queue {
    pub(crate) fn new(
        device: Arc<DeviceResource>,
        semaphore: Arc<SemaphoreResource>,
        resource: QueueResource,
        kind: QueueKind,
        present: bool,
        lane: Lane,
    ) -> Self {
        Self {
            resource,
            device,
            semaphore,
            kind,
            present,
            lane,
        }
    }

    pub(crate) fn semaphore(&self) -> &Arc<SemaphoreResource> {
        &self.semaphore
    }

    pub(crate) fn resource(&self) -> &QueueResource {
        &self.resource
    }

    pub fn kind(&self) -> QueueKind {
        self.kind
    }

    pub fn presentable(&self) -> bool {
        self.present
    }

    fn lane(&self) -> Lane {
        self.lane
    }

    pub fn present(
        &self,
        swapchain: &mut Swapchain,
        token: PresentToken,
    ) -> Result<(), PresentError> {
        use vulkanalia::prelude::v1_0::*;
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;

        if !self.presentable() {
            return Err(PresentError::QueueNotPresentable);
        }

        if token.generation() != swapchain.generation() {
            return Err(PresentError::GenerationMismatch);
        }

        let wait_semaphores = [unsafe { token.render_finished().handle() }];
        let swapchains = [unsafe { swapchain.current_handle() }];
        let indices = [token.index()];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&indices);

        let result = unsafe {
            self.device
                .handle()
                .queue_present_khr(self.resource.handle(), &present_info)
        };

        match result {
            Ok(code) => {
                if code == vk::SuccessCode::SUBOPTIMAL_KHR {
                    swapchain.set_should_recreate();
                }
                Ok(())
            }
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => Err(PresentError::RecreateSwapchain),
            Err(vk::ErrorCode::SURFACE_LOST_KHR) => Err(PresentError::RecreateSurface),
            Err(vk::ErrorCode::FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT) => {
                Err(PresentError::RegainFullScreen)
            }
            Err(error) => Err(PresentError::Code(error)),
        }
    }

    fn submit(&mut self, submission: Submission) -> Result<()> {
        todo!()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Lane {
    index: LaneIndex,
    family: QueueFamily,
}

impl Lane {
    pub(crate) fn new(index: LaneIndex, family: QueueFamily) -> Self {
        Self { index, family }
    }

    pub(crate) fn index(&self) -> LaneIndex {
        self.index
    }

    pub(crate) fn family(&self) -> QueueFamily {
        self.family
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LaneIndex(u32);

impl LaneIndex {
    pub(crate) fn new(index: u32) -> Self {
        assert!(index < 64);
        Self(index)
    }
}

impl From<LaneIndex> for u32 {
    fn from(value: LaneIndex) -> Self {
        value.0
    }
}

impl From<LaneIndex> for usize {
    fn from(value: LaneIndex) -> Self {
        value.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFamily(u32);

impl From<u32> for QueueFamily {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<QueueFamily> for u32 {
    fn from(value: QueueFamily) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueueCap {
    Graphics,
    Compute,
    Transfer,
}

impl From<QueueCap> for char {
    fn from(value: QueueCap) -> Self {
        match value {
            QueueCap::Graphics => 'g',
            QueueCap::Compute => 'c',
            QueueCap::Transfer => 't',
        }
    }
}

bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct QueueCapFlags: u8 {
        const GRAPHICS = 0b001;
        const COMPUTE  = 0b010;
        const TRANSFER = 0b100;
    }
}

impl From<QueueCap> for QueueCapFlags {
    fn from(value: QueueCap) -> Self {
        match value {
            QueueCap::Graphics => QueueCapFlags::GRAPHICS,
            QueueCap::Compute => QueueCapFlags::COMPUTE,
            QueueCap::Transfer => QueueCapFlags::TRANSFER,
        }
    }
}

impl From<vulkanalia::vk::QueueFlags> for QueueCapFlags {
    fn from(flags: vulkanalia::vk::QueueFlags) -> Self {
        let mut caps = Self::empty();
        if flags.contains(vulkanalia::vk::QueueFlags::GRAPHICS) {
            caps |= Self::GRAPHICS;
        }
        if flags.contains(vulkanalia::vk::QueueFlags::COMPUTE) {
            caps |= Self::COMPUTE;
        }
        if flags.contains(vulkanalia::vk::QueueFlags::TRANSFER) {
            caps |= Self::TRANSFER;
        }
        caps
    }
}
