use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;
use vulkanalia_vma::{self as vma};

use super::{GpuAllocator, GpuBuffer, GpuBufferView, GpuDevice, MappedBuffer};

pub struct RendererWriteCtx<'a> {
    pub upload_batch: &'a mut UploadBatch,
}

impl<'a> RendererWriteCtx<'a> {
    pub fn new(upload_batch: &'a mut UploadBatch) -> Self {
        Self { upload_batch }
    }
}

#[derive(Clone)]
pub struct UploadTicket {
    pub dst: GpuBufferView,
    pub semaphore: vk::Semaphore,
    pub value: u64,
}

pub struct UploadSystem {
    device: Arc<GpuDevice>,
    allocator: Arc<GpuAllocator>,
    queue: vk::Queue,
    timeline_semaphore: vk::Semaphore,
    next_timeline_value: u64,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    pending_resources: VecDeque<(u64, Vec<GpuBuffer>)>,
}

pub struct UploadBatch {
    allocator: Arc<GpuAllocator>,
    ops: Vec<UploadOp>,
    temp_resources: Vec<GpuBuffer>,
}

enum UploadOp {
    HostWrite { dst: GpuBufferView },
    StagedCopy {
        src: GpuBuffer,
        dst: GpuBufferView,
        size: vk::DeviceSize,
    },
}

impl UploadSystem {
    pub fn new(
        device: Arc<GpuDevice>,
        allocator: Arc<GpuAllocator>,
        queue: vk::Queue,
        queue_family_index: u32,
    ) -> Result<Self> {
        let command_pool = unsafe {
            device
                .get_vk_device()
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::builder()
                        .queue_family_index(queue_family_index)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )
                .context("failed to create upload command pool")?
        };

        let command_buffer = Self::allocate_command_buffer(device.as_ref(), command_pool)?;

        let mut semaphore_type = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        let semaphore_info = vk::SemaphoreCreateInfo::builder().push_next(&mut semaphore_type);
        let timeline_semaphore = unsafe {
            device
                .get_vk_device()
                .create_semaphore(&semaphore_info, None)
                .context("failed to create upload timeline semaphore")?
        };

        Ok(Self {
            device,
            allocator,
            queue,
            timeline_semaphore,
            next_timeline_value: 0,
            command_pool,
            command_buffer,
            pending_resources: VecDeque::new(),
        })
    }

    pub fn begin_batch(&self) -> UploadBatch {
        UploadBatch {
            allocator: self.allocator.clone(),
            ops: Vec::new(),
            temp_resources: Vec::new(),
        }
    }

    pub fn timeline_semaphore(&self) -> vk::Semaphore {
        self.timeline_semaphore
    }

    pub fn reset_command_resources(&mut self) -> Result<()> {
        unsafe {
            self.device
                .get_vk_device()
                .reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty())
                .context("failed to reset upload command pool")?;
        }
        self.command_buffer = Self::allocate_command_buffer(self.device.as_ref(), self.command_pool)?;
        Ok(())
    }

    pub fn poll_completed(&mut self) -> Result<()> {
        let completed = unsafe {
            self.device
                .get_vk_device()
                .get_semaphore_counter_value(self.timeline_semaphore)
                .context("failed to read upload timeline counter")?
        };

        while let Some((value, _)) = self.pending_resources.front() {
            if *value > completed {
                break;
            }
            self.pending_resources.pop_front();
        }

        Ok(())
    }

    pub fn submit(&mut self, mut batch: UploadBatch) -> Result<Vec<UploadTicket>> {
        if batch.ops.is_empty() {
            return Ok(vec![]);
        }

        let command_buffer = self.command_buffer;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            self.device
                .get_vk_device()
                .begin_command_buffer(command_buffer, &begin_info)
                .context("failed to begin upload command buffer")?;
        }

        let mut host_barriers = Vec::new();
        let mut copy_barriers = Vec::new();

        for op in &batch.ops {
            if let UploadOp::HostWrite { dst } = op {
                host_barriers.push(
                    vk::BufferMemoryBarrier2::builder()
                        .src_stage_mask(vk::PipelineStageFlags2::HOST)
                        .src_access_mask(vk::AccessFlags2::HOST_WRITE)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_access_mask(
                            vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
                        )
                        .buffer(dst.handle())
                        .offset(dst.offset())
                        .size(dst.size())
                        .build(),
                );
            }
        }

        if !host_barriers.is_empty() {
            let dependency = vk::DependencyInfo::builder().buffer_memory_barriers(&host_barriers);
            unsafe {
                self.device
                    .get_vk_device()
                    .cmd_pipeline_barrier2(command_buffer, &dependency);
            }
        }

        for op in &batch.ops {
            if let UploadOp::StagedCopy { src, dst, size } = op {
                let copy_region = vk::BufferCopy::builder()
                    .src_offset(0)
                    .dst_offset(dst.offset())
                    .size(*size)
                    .build();
                unsafe {
                    self.device.get_vk_device().cmd_copy_buffer(
                        command_buffer,
                        src.handle(),
                        dst.handle(),
                        &[copy_region],
                    );
                }

                copy_barriers.push(
                    vk::BufferMemoryBarrier2::builder()
                        .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_access_mask(
                            vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE,
                        )
                        .buffer(dst.handle())
                        .offset(dst.offset())
                        .size(dst.size())
                        .build(),
                );
            }
        }

        if !copy_barriers.is_empty() {
            let dependency = vk::DependencyInfo::builder().buffer_memory_barriers(&copy_barriers);
            unsafe {
                self.device
                    .get_vk_device()
                    .cmd_pipeline_barrier2(command_buffer, &dependency);
            }
        }

        unsafe {
            self.device
                .get_vk_device()
                .end_command_buffer(command_buffer)
                .context("failed to end upload command buffer")?;
        }

        self.next_timeline_value += 1;
        let signal_value = self.next_timeline_value;

        let signal_semaphores = [self.timeline_semaphore];
        let command_buffers = [command_buffer];
        let signal_values = [signal_value];
        let mut timeline = vk::TimelineSemaphoreSubmitInfo::builder()
            .wait_semaphore_values(&[])
            .signal_semaphore_values(&signal_values);
        let submit = [vk::SubmitInfo::builder()
            .push_next(&mut timeline)
            .wait_semaphores(&[])
            .wait_dst_stage_mask(&[])
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores)
            .build()];

        unsafe {
            self.device
                .get_vk_device()
                .queue_submit(self.queue, &submit, vk::Fence::null())
                .context("failed to submit upload command buffer")?;
        }

        if !batch.temp_resources.is_empty() {
            self.pending_resources
                .push_back((signal_value, std::mem::take(&mut batch.temp_resources)));
        }

        let mut tickets = Vec::with_capacity(batch.ops.len());
        for op in batch.ops {
            let dst = match op {
                UploadOp::HostWrite { dst } => dst,
                UploadOp::StagedCopy { dst, .. } => dst,
            };
            tickets.push(UploadTicket {
                dst,
                semaphore: self.timeline_semaphore,
                value: signal_value,
            });
        }

        Ok(tickets)
    }

    pub fn wait_idle(&mut self) -> Result<()> {
        unsafe {
            self.device
                .get_vk_device()
                .queue_wait_idle(self.queue)
                .context("failed to wait for upload queue idle")
        }?;
        self.pending_resources.clear();
        Ok(())
    }

    fn allocate_command_buffer(device: &GpuDevice, command_pool: vk::CommandPool) -> Result<vk::CommandBuffer> {
        let alloc_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffers = unsafe {
            device
                .get_vk_device()
                .allocate_command_buffers(&alloc_info)
                .context("failed to allocate upload command buffer")?
        };
        command_buffers
            .first()
            .copied()
            .ok_or_else(|| anyhow!("upload command buffer allocation returned no command buffers"))
    }
}

impl Drop for UploadSystem {
    fn drop(&mut self) {
        let _ = self.wait_idle();
        unsafe {
            self.device
                .get_vk_device()
                .destroy_semaphore(self.timeline_semaphore, None);
            self.device
                .get_vk_device()
                .destroy_command_pool(self.command_pool, None);
        }
    }
}

impl UploadBatch {
    pub fn write_mapped(
        &mut self,
        mapped: &MappedBuffer,
        dst: GpuBufferView,
        dst_local_offset: vk::DeviceSize,
        bytes: &[u8],
    ) -> Result<()> {
        mapped.write_and_flush(&dst, dst_local_offset, bytes)?;
        let range = dst.subview(dst_local_offset, bytes.len() as vk::DeviceSize)?;
        self.ops.push(UploadOp::HostWrite { dst: range });
        Ok(())
    }

    pub fn upload_bytes(
        &mut self,
        dst: GpuBufferView,
        dst_local_offset: vk::DeviceSize,
        bytes: &[u8],
    ) -> Result<()> {
        let size = bytes.len() as vk::DeviceSize;
        let dst = dst.subview(dst_local_offset, size)?;

        let allocation_options = vma::AllocationOptions {
            usage: vma::MemoryUsage::AutoPreferHost,
            flags: vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            ..Default::default()
        };
        let staging = GpuBuffer::create(
            self.allocator.clone(),
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            &allocation_options,
        )?;

        let ptr = unsafe { staging.allocator().map_memory(staging.allocation()) }
            .map_err(|err| anyhow!(err))
            .context("failed to map staging buffer")?;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        }
        unsafe {
            staging
                .allocator()
                .flush_allocation(staging.allocation(), 0, size)
                .map_err(|err| anyhow!(err))
                .context("failed to flush staging buffer")?;
            staging.allocator().unmap_memory(staging.allocation());
        }

        self.ops.push(UploadOp::StagedCopy {
            src: staging.clone(),
            dst,
            size,
        });
        self.temp_resources.push(staging);
        Ok(())
    }
}
