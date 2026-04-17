use std::sync::Arc;

use anyhow::{Result, bail};
use smallvec::SmallVec;
use vulkanalia::vk;

use crate::gal::command_buffer::CommandBuffer;
use crate::gal::command_buffer::CommandPacket;
use crate::gal::queue::Lane;
use crate::gal::semaphore_resource::SemaphoreResource;

// TODO: SubmissionAllocator to recycle vecs?

pub struct Submission {
    frame: u32,
    lane: Lane,
    // TODO: make CommandPacket an enum instead?
    packets: SmallVec<[CommandPacket; 1]>,
    waits: SmallVec<[SemaphoreWait; 2]>,
    signals: SmallVec<[SemaphoreSignal; 2]>,
    submitted: bool,
}

impl Submission {
    pub fn new(frame: u32, lane: Lane) -> Self {
        Self {
            frame,
            lane,
            packets: SmallVec::new(),
            waits: SmallVec::new(),
            signals: SmallVec::new(),
            submitted: false,
        }
    }

    pub fn with_capacity(frame: u32, lane: Lane, capacity: usize) -> Self {
        Self {
            frame,
            lane,
            packets: SmallVec::with_capacity(capacity),
            waits: SmallVec::new(),
            signals: SmallVec::new(),
            submitted: false,
        }
    }

    pub fn push(&mut self, cmdbuf: CommandBuffer) -> Result<()> {
        if self.lane.family() != cmdbuf.lane().family() {
            bail!("queue family mismatch");
        }
        if self.frame != cmdbuf.frame() {
            bail!("frame number mismatch");
        }
        self.packets.push(cmdbuf.into_packet()?);
        Ok(())
    }

    // TODO: too low-level of an api
    pub fn wait_semaphore(
        &mut self,
        semaphore: Arc<SemaphoreResource>,
        stage_mask: vk::PipelineStageFlags2,
    ) {
        self.waits.push(SemaphoreWait::new(semaphore, stage_mask));
    }

    // TODO: too low-level of an api
    pub fn signal_semaphore(
        &mut self,
        semaphore: Arc<SemaphoreResource>,
        stage_mask: vk::PipelineStageFlags2,
    ) {
        self.signals
            .push(SemaphoreSignal::new(semaphore, stage_mask));
    }

    // TODO: gross....make CommandPacket an enum instead
    pub(crate) fn into_parts(
        mut self,
    ) -> (
        u32,
        Lane,
        SmallVec<[CommandPacket; 1]>,
        SmallVec<[SemaphoreWait; 2]>,
        SmallVec<[SemaphoreSignal; 2]>,
    ) {
        self.submitted = true;
        let frame = self.frame;
        let lane = self.lane;
        let packets = std::mem::take(&mut self.packets);
        let waits = std::mem::take(&mut self.waits);
        let signals = std::mem::take(&mut self.signals);
        (frame, lane, packets, waits, signals)
    }
}

impl Drop for Submission {
    fn drop(&mut self) {
        assert!(
            self.packets.len() == 0 || self.submitted,
            "submission not used"
        );
    }
}

pub(crate) struct SemaphoreWait {
    semaphore: Arc<SemaphoreResource>,
    stage_mask: vk::PipelineStageFlags2,
}

impl SemaphoreWait {
    fn new(semaphore: Arc<SemaphoreResource>, stage_mask: vk::PipelineStageFlags2) -> Self {
        Self {
            semaphore,
            stage_mask,
        }
    }

    pub(crate) fn semaphore(&self) -> &Arc<SemaphoreResource> {
        &self.semaphore
    }

    pub(crate) fn stage_mask(&self) -> vk::PipelineStageFlags2 {
        self.stage_mask
    }
}

pub(crate) struct SemaphoreSignal {
    semaphore: Arc<SemaphoreResource>,
    stage_mask: vk::PipelineStageFlags2,
}

impl SemaphoreSignal {
    fn new(semaphore: Arc<SemaphoreResource>, stage_mask: vk::PipelineStageFlags2) -> Self {
        Self {
            semaphore,
            stage_mask,
        }
    }

    pub(crate) fn semaphore(&self) -> &Arc<SemaphoreResource> {
        &self.semaphore
    }

    pub(crate) fn stage_mask(&self) -> vk::PipelineStageFlags2 {
        self.stage_mask
    }
}
