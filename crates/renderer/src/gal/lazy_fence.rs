use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;

use crate::gal::device::DeviceResource;
use crate::gal::fence_resource::FenceResource;

pub struct LazyFence {
    state: State,
}

impl LazyFence {
    // return the raw fence handle in an unsignalled state, either creating a
    // new unsignalled fence, or resetting the existing one
    pub(crate) unsafe fn get_or_init_unsignalled(
        &mut self,
        device: &Arc<DeviceResource>,
    ) -> Result<vk::Fence> {
        if let Some(handle) = unsafe { self.handle() } {
            if self.is_signalled()? {
                self.reset()?;
            }
            Ok(handle)
        } else {
            let fence = FenceResource::unsignalled(device.clone())?;
            let handle = unsafe { fence.handle() };
            self.state = State::Live {
                fence,
                signalled: false,
            };
            Ok(handle)
        }
    }

    unsafe fn handle(&self) -> Option<vk::Fence> {
        match &self.state {
            State::Empty => None,
            State::Live { fence, .. } => Some(unsafe { fence.handle() }),
        }
    }

    // idempotently reset the fence
    pub fn reset(&mut self) -> Result<()> {
        if let State::Live { fence, signalled } = &mut self.state {
            if *signalled {
                fence.reset()?;
                *signalled = false;
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.state, State::Empty)
    }

    // read the fence status, returning if it's signalled, caching the result
    // when possible
    pub fn is_signalled(&mut self) -> Result<bool> {
        if let State::Live { fence, signalled } = &mut self.state {
            if !*signalled {
                *signalled = fence.is_signalled()?;
            }
            Ok(*signalled)
        } else {
            Ok(false)
        }
    }
}

impl Default for LazyFence {
    fn default() -> Self {
        Self {
            state: State::Empty,
        }
    }
}

enum State {
    Empty,
    Live {
        fence: FenceResource,
        signalled: bool,
    },
}
