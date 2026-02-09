use anyhow::{Result, anyhow};
use vulkanalia::vk;

use super::{GpuBuffer, GpuBufferView};

#[derive(Clone)]
pub struct UploadBuffer {
    buffer: GpuBuffer,
}

impl UploadBuffer {
    pub fn new(buffer: GpuBuffer) -> Result<Self> {
        if !buffer.usage().contains(vk::BufferUsageFlags::TRANSFER_DST) {
            return Err(anyhow!(
                "upload destination buffer must include TRANSFER_DST usage"
            ));
        }
        Ok(Self { buffer })
    }

    pub fn buffer(&self) -> &GpuBuffer {
        &self.buffer
    }

    pub fn whole_view(&self) -> GpuBufferView {
        self.buffer.whole_view()
    }

    pub fn view(&self, offset: vk::DeviceSize, size: vk::DeviceSize) -> Result<GpuBufferView> {
        self.buffer.view(offset, size)
    }
}
