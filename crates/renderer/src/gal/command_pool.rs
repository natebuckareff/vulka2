use anyhow::Result;
use vulkanalia::vk;

use crate::gal::command_buffer::CommandBuffer;
use crate::gal::command_pool_resource::CommandPoolResource;
use crate::gal::queue::Lane;
use crate::gal::usage_token::UsageToken;

pub struct CommandPool {
    frame: u32,
    resource: CommandPoolResource,
    usage: UsageToken,
    handles: Vec<vk::CommandBuffer>,
    index: usize,
}

impl CommandPool {
    pub(crate) fn new(frame: u32, resource: CommandPoolResource) -> Self {
        Self {
            frame,
            resource,
            usage: UsageToken::new(),
            handles: Vec::new(),
            index: 0,
        }
    }

    pub(crate) fn resource(&self) -> &CommandPoolResource {
        &self.resource
    }

    pub(crate) fn swap_usage(&mut self) -> UsageToken {
        std::mem::replace(&mut self.usage, UsageToken::new())
    }

    pub(crate) fn reset(&mut self, frame: u32) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;
        unsafe {
            self.resource
                .device()
                .handle()
                .reset_command_pool(self.resource.handle(), vk::CommandPoolResetFlags::empty())?;
        }
        self.frame = frame;
        self.index = 0;
        Ok(())
    }

    pub fn allocate(&mut self, lane: Lane) -> Result<CommandBuffer<'_>> {
        let handle = self.allocate_handle()?;
        CommandBuffer::new(
            &mut self.usage,
            &self.resource.device(),
            self.frame,
            lane,
            handle,
        )
    }

    fn allocate_handle(&mut self) -> Result<vk::CommandBuffer> {
        use vulkanalia::prelude::v1_0::*;
        if self.index < self.handles.len() {
            let handle = self.handles[self.index];
            self.index += 1;
            return Ok(handle);
        }
        let info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(unsafe { self.resource.handle() })
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let handle = unsafe {
            self.resource
                .device()
                .handle()
                .allocate_command_buffers(&info)?[0]
        };
        self.handles.push(handle);
        self.index += 1;
        Ok(handle)
    }
}
