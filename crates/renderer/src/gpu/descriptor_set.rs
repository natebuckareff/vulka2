use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{
    BlockAllocator, BufferSpan, BufferWriter, DescriptorPool, DescriptorPoolId,
    DescriptorSetLayout, DescriptorSetToken, Device, ParameterBlock, ParameterWriter, RetireRecord,
    RetireToken, VulkanResource,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DescriptorSetId(usize);

impl From<usize> for DescriptorSetId {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

enum RetireState {
    Ready,
    Pending,
    Retired(RetireRecord<DescriptorSetHandle>),
}

pub struct DescriptorSet {
    device: Arc<Device>,
    handle: DescriptorSetHandle,
    set_layout: Arc<DescriptorSetLayout>,
    set: vk::DescriptorSet, // TODO: OwnedDescriptorSet?
    state: RetireState,
}

impl DescriptorSet {
    pub(crate) fn new(
        id: DescriptorSetId,
        device: Arc<Device>,
        pool: &DescriptorPool,
        set_layout: Arc<DescriptorSetLayout>,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let set_layouts = &[unsafe { *set_layout.owned().raw() }];
        let info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(unsafe { *pool.owned().raw() })
            .set_layouts(set_layouts);

        let descriptor_sets = unsafe {
            let handle = device.handle();
            handle.raw().allocate_descriptor_sets(&info)?
        };
        let set = descriptor_sets[0];

        let handle = DescriptorSetHandle {
            id,
            pool: pool.id(),
        };

        Ok(Self {
            device,
            handle,
            set_layout,
            set,
            state: RetireState::Ready,
        })
    }

    pub fn device(&self) -> &Arc<Device> {
        &self.device
    }

    pub unsafe fn raw(&self) -> vk::DescriptorSet {
        self.set
    }

    pub fn handle(&self) -> &DescriptorSetHandle {
        &self.handle
    }

    pub fn set_layout(&self) -> &Arc<DescriptorSetLayout> {
        &self.set_layout
    }

    pub fn acquire(mut self, ubo: Option<BufferWriter>) -> Result<ParameterBlock, AcquireError> {
        match self.acquire_is_completed() {
            Ok(true) => {}
            Ok(false) => {
                return Err(AcquireError {
                    set: self,
                    cause: None,
                });
            }
            Err(e) => {
                return Err(AcquireError {
                    set: self,
                    cause: Some(e),
                });
            }
        };
        if let Some(ubo) = &ubo {
            if let Err(e) = self.write_implicit_ubo_descriptor(ubo.span()) {
                return Err(AcquireError {
                    set: self,
                    cause: Some(e),
                });
            }
        }
        let parameter_writer = ParameterWriter::new(self);
        Ok(ParameterBlock::new(parameter_writer, ubo))
    }

    fn acquire_is_completed(&mut self) -> Result<bool> {
        let state = std::mem::replace(&mut self.state, RetireState::Pending);
        match state {
            RetireState::Ready => Ok(true),
            RetireState::Pending => Ok(false),
            RetireState::Retired(record) => {
                let result = record.is_complete(&self.device);
                let is_complete = match result {
                    Ok(is_complete) => is_complete,
                    Err(e) => {
                        self.state = RetireState::Retired(record);
                        return Err(e);
                    }
                };
                if !is_complete {
                    self.state = RetireState::Retired(record);
                    return Ok(false);
                }
                Ok(true)
            }
        }
    }

    pub fn retire(token: DescriptorSetToken) -> Result<Self> {
        let (mut set, retire) = token.into_parts();
        set.state = RetireState::Retired(retire.retire()?);
        Ok(*set)
    }

    // TODO: this needs to be checked for correctness, feels a bit hacky right now
    pub fn free(mut self) -> Result<FreedDescriptorSet, AcquireError> {
        let state = std::mem::replace(&mut self.state, RetireState::Ready);
        let record = match state {
            RetireState::Ready => None,
            RetireState::Pending => {
                self.state = state;
                return Err(AcquireError {
                    set: self,
                    cause: None,
                });
            }
            RetireState::Retired(record) => Some(record),
        };
        let retire = record.map(RetireToken::from_record);
        Ok(FreedDescriptorSet { set: self, retire })
    }

    pub fn write_implicit_ubo_descriptor(&mut self, span: &BufferSpan) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;

        // TODO: should be validating
        // - size limits
        // - alignment limits

        let buffer = span.buffer();
        buffer.check_usage(vk::BufferUsageFlags::UNIFORM_BUFFER)?;

        let dst_binding = 0; // implicit UBO is always binding 0
        let dst_array_element = 0;

        let info = vk::DescriptorBufferInfo::builder()
            .buffer(unsafe { buffer.raw() })
            .offset(span.range().start())
            .range(span.range().size());

        let buffer_info = &[info];

        let write = vk::WriteDescriptorSet::builder()
            .dst_set(self.set)
            .dst_binding(dst_binding)
            .dst_array_element(dst_array_element)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(buffer_info);

        let descriptor_writes = &[write];
        let descriptor_copies: &[vk::CopyDescriptorSet; 0] = &[];

        unsafe {
            self.device
                .handle()
                .raw()
                .update_descriptor_sets(descriptor_writes, descriptor_copies);
        }

        Ok(())
    }

    // TODO: lot of duplication with `write_buffer_descriptor`
    pub fn write_dynamic_buffer_descriptor(
        &mut self,
        offset: &slang::ShaderOffset,
        allocator: &impl BlockAllocator,
    ) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;

        // TODO: should be validating
        // - size limits
        // - alignment limits

        let parameter_block_layout = self.set_layout.layout().parameter_block_layout()?;
        let binding_layout = &parameter_block_layout
            .find_binding_range(offset.binding_range)
            .context("binding range out-of-bounds")?
            .descriptor;

        let usage = match binding_layout.descriptor_type {
            vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC => vk::BufferUsageFlags::UNIFORM_BUFFER,
            vk::DescriptorType::STORAGE_BUFFER_DYNAMIC => vk::BufferUsageFlags::STORAGE_BUFFER,
            _ => return Err(anyhow!("invalid resource and descriptor type")),
        };

        let region = allocator.backing();

        region.buffer().check_usage(usage)?;

        let buffer = unsafe { allocator.backing().buffer().raw() };
        let dst_binding = binding_layout.binding as u32;
        let dst_array_element = offset.array_index as u32;

        let info = vk::DescriptorBufferInfo::builder()
            .buffer(buffer)
            .offset(region.range().start())
            .range(allocator.block_size());

        let buffer_info = &[info];

        let write = vk::WriteDescriptorSet::builder()
            .dst_set(self.set)
            .dst_binding(dst_binding)
            .dst_array_element(dst_array_element)
            .descriptor_type(binding_layout.descriptor_type)
            .buffer_info(buffer_info);

        let descriptor_writes = &[write];
        let descriptor_copies: &[vk::CopyDescriptorSet; 0] = &[];

        unsafe {
            self.device
                .handle()
                .raw()
                .update_descriptor_sets(descriptor_writes, descriptor_copies);
        }

        Ok(())
    }

    pub fn writer(self) -> ParameterWriter {
        ParameterWriter::new(self)
    }

    pub fn object<'a, T>(self, ubo: Option<BufferSpan>) -> ParameterBlock
    where
        T: BlockAllocator,
    {
        let parameter_writer = self.writer();
        let ubo_writer = ubo.map(BufferSpan::writer);
        ParameterBlock::new(parameter_writer, ubo_writer)
    }
}

pub struct AcquireError {
    pub set: DescriptorSet,
    pub cause: Option<anyhow::Error>,
}

// TODO: same clunky feeling handle as command buffers...is it a smell?
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorSetHandle {
    id: DescriptorSetId,
    pool: DescriptorPoolId,
}

impl DescriptorSetHandle {
    pub fn id(&self) -> DescriptorSetId {
        self.id
    }

    pub fn pool(&self) -> DescriptorPoolId {
        self.pool
    }
}

pub struct FreedDescriptorSet {
    set: DescriptorSet,
    retire: Option<RetireToken<DescriptorSetHandle>>,
}

impl FreedDescriptorSet {
    pub fn into_parts(self) -> (DescriptorSet, Option<RetireToken<DescriptorSetHandle>>) {
        (self.set, self.retire)
    }
}

pub struct BufferBinding<'a, T: BlockAllocator> {
    allocator: &'a T,
    span: BufferSpan,
}

impl<'a, T: BlockAllocator> BufferBinding<'a, T> {
    pub fn allocator(&self) -> &'a T {
        &self.allocator
    }

    pub fn span(&self) -> &BufferSpan {
        &self.span
    }
}

pub trait ShaderDescriptor {
    fn encode_into(self, layout: &slang::LayoutCursor, set: &mut DescriptorSet) -> Result<()>;
}

impl<'a, T: BlockAllocator> ShaderDescriptor for &BufferBinding<'a, T> {
    fn encode_into(self, layout: &slang::LayoutCursor, set: &mut DescriptorSet) -> Result<()> {
        set.write_dynamic_buffer_descriptor(layout.offset(), self.allocator())
    }
}
