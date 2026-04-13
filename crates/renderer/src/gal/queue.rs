use std::hash::Hash;
use std::sync::Arc;

use anyhow::{Result, bail};
use bitflags::bitflags;

use crate::gal::Device;
use crate::gal::QueueKind;
use crate::gal::Swapchain;
use crate::gal::queue_resource::QueueResource;
use crate::gal::semaphore_resource::SemaphoreResource;
use crate::gal::submission::Submission;
use crate::gal::swapchain::PresentError;
use crate::gal::swapchain_image::PresentToken;

pub struct Queue {
    device: Arc<Device>,
    semaphore: Arc<SemaphoreResource>,
    resource: QueueResource,
    kind: QueueKind,
    present: bool,
    frame: u32,
    lane: Lane,
    submissions: u32,
}

impl Queue {
    pub(crate) fn new(
        device: Arc<Device>,
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
            frame: 0,
            lane,
            submissions: 0,
        }
    }

    pub(crate) fn semaphore(&self) -> &Arc<SemaphoreResource> {
        &self.semaphore
    }

    pub(crate) fn device(&self) -> &Arc<Device> {
        &self.device
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

    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn lane(&self) -> Lane {
        self.lane
    }

    pub fn submissions(&self) -> u32 {
        self.submissions
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

        if token.family() != self.resource.family() {
            return Err(PresentError::OwnershipTransferUnsupported);
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
                .resource()
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

    pub fn submit(&mut self, submission: Submission) -> Result<()> {
        todo!()
    }

    fn increment(&mut self, frame: u32) -> Result<()> {
        if frame == self.frame + 1 {
            // if the frame advanced, update the timeline with the final
            // submission count for the last frame
            self.device
                .timeline()
                .update(self.frame, self.lane.index(), self.submissions);
        } else if self.frame != frame {
            bail!("frame number out-of-order")
        }
        self.frame = frame;
        self.submissions += 1;
        Ok(())
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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
