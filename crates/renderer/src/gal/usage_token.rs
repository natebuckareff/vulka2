use anyhow::Result;

use crate::gal::Device;
use crate::gal::queue::{Lane, LaneIndex};

pub struct UsageToken {
    frame: u32,
    mask: LaneMask,
}

impl UsageToken {
    pub fn new() -> Self {
        UsageToken {
            frame: 0,
            mask: LaneMask::new(),
        }
    }

    pub fn swap(&mut self) -> Self {
        let old = Self {
            frame: self.frame,
            mask: self.mask,
        };
        self.frame = 0;
        self.mask = LaneMask::new();
        old
    }

    pub fn frame(&self) -> u32 {
        self.frame
    }

    pub fn mask(&self) -> LaneMask {
        self.mask
    }

    pub fn touch(&mut self, frame: u32, lane: Lane) {
        self.mask.set(lane);
        self.frame = self.frame.max(frame);
    }

    pub fn is_reclaimable(&self, device: &Device) -> Result<bool> {
        let timeline = device.timeline();
        for lane in self.mask.iter() {
            if !timeline.poll(self.frame, lane)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn wait_until_reclaimable(&self, device: &Device) -> Result<bool> {
        device.timeline().wait_many(self.frame, self.mask)
    }
}

#[derive(Clone, Copy)]
pub struct LaneMask {
    mask: u64,
}

impl LaneMask {
    pub fn new() -> Self {
        Self { mask: 0 }
    }

    pub fn get(&self, lane: Lane) -> bool {
        (self.mask & Self::bit(lane)) != 0
    }

    pub fn set(&mut self, lane: Lane) {
        self.mask |= Self::bit(lane);
    }

    pub fn iter(&self) -> impl Iterator<Item = LaneIndex> {
        // TODO: can we do better than always iterating 64 times?
        (0..64)
            .into_iter()
            .filter(|i| (self.mask & (1u64 << i)) != 0)
            .map(LaneIndex::new)
    }

    fn bit(lane: Lane) -> u64 {
        1u64 << u64::from(u32::from(lane.index()))
    }
}
