use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::gpu_v2::{FrameToken, SettledLanes, SubmissionProgress};

pub struct FrameAllocator {
    settled: Arc<SettledLanes>,
    progress: SubmissionProgress,
    first_frame: Option<FrameToken>,
}

impl FrameAllocator {
    pub fn new(settled: Arc<SettledLanes>, progress: SubmissionProgress) -> Self {
        let first_frame = Some(progress.current().token.clone());
        Self {
            settled,
            progress,
            first_frame,
        }
    }

    pub fn frames_in_flight(&self) -> u64 {
        if self.first_frame.is_some() {
            return 0;
        }
        self.progress.frames_in_flight()
    }

    pub fn wait_until(&mut self, max_frames_in_flight: u64) -> Result<bool> {
        if max_frames_in_flight == 0 {
            return Err(anyhow!("max_frames_in_flight must be > 0"));
        }

        if self.first_frame.is_some() {
            // haven't started the first frame yet
            return Ok(true);
        }

        if self.frames_in_flight() < max_frames_in_flight {
            // can early out
            return Ok(true);
        }

        let current = self.progress.current().frame.number;
        let until = current.saturating_sub(max_frames_in_flight - 1);
        self.progress.wait(&self.settled, until)
    }

    pub fn next_frame(&mut self) -> Result<FrameToken> {
        let token = match self.first_frame.take() {
            Some(token) => token,
            None => self.progress.next(&self.settled)?,
        };
        Ok(token)
    }

    pub fn update(&mut self) -> Result<()> {
        self.progress.update(&self.settled)
    }
}
