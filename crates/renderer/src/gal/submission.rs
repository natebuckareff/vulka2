use anyhow::{Result, bail};
use smallvec::SmallVec;
use vulkanalia::vk;

use crate::gal::command_buffer::CommandBuffer;
use crate::gal::command_buffer::CommandPacket;
use crate::gal::queue::Lane;

pub struct Submission {
    frame: u32,
    lane: Lane,
    packets: SmallVec<[CommandPacket; 1]>,
    submitted: bool,
}

impl Submission {
    pub fn new(frame: u32, lane: Lane) -> Self {
        Self {
            frame,
            lane,
            packets: SmallVec::new(),
            submitted: false,
        }
    }

    pub fn with_capacity(frame: u32, lane: Lane, capacity: usize) -> Self {
        Self {
            frame,
            lane,
            packets: SmallVec::with_capacity(capacity),
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
        self.packets.push(cmdbuf.into_packet());
        Ok(())
    }

    pub(crate) fn finish(mut self) {
        self.submitted = true;
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
