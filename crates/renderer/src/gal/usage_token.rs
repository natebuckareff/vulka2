use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, bail};

use crate::gal::device::Device;
use crate::gal::queue::{Lane, LaneIndex};
use crate::gal::queue_timeline::TimelineValue;

#[derive(Clone)]
pub struct UsageToken {
    inner: Arc<UsageInner<[AtomicU64]>>,
}

struct UsageInner<Lanes: ?Sized> {
    mask: u64,
    lanes: Lanes,
}

impl UsageToken {
    fn new<const N: usize>() -> Self {
        assert!(N < 64);
        let lanes: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
        let inner = Arc::new(UsageInner { mask: 0, lanes });
        UsageToken { inner }
    }

    pub(crate) fn exclusive() -> Self {
        Self::new::<1>()
    }

    fn len(&self) -> usize {
        self.inner.lanes.len()
    }

    fn get(&self, lane: Lane) -> Option<TimelineValue> {
        let Some(index) = self.index_of(lane) else {
            return None;
        };
        let value = self.inner.lanes[index].load(Ordering::Relaxed);
        Some(TimelineValue::new(value))
    }

    fn set_max(&mut self, lane: Lane, value: TimelineValue) -> Result<()> {
        let Some(index) = self.index_of(lane) else {
            bail!("lane key out-of-bounds")
        };
        self.inner.lanes[index].fetch_max(value.into(), Ordering::Relaxed);
        Ok(())
    }

    pub(crate) fn is_reclaimable(&self, device: &Device) -> Result<bool> {
        for index in self.iter() {
            let timeline = device.timeline(index); // XXX
            let current = self.inner.lanes[usize::from(index)].load(Ordering::Relaxed);
            if current >= timeline.poll()?.into() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn iter(&self) -> impl Iterator<Item = LaneIndex> {
        (0..64)
            .into_iter()
            .filter_map(|i| rank_of_bit(self.inner.mask, i))
            .map(|index| LaneIndex::new(index as u32))
    }

    fn index_of(&self, lane: Lane) -> Option<usize> {
        rank_of_bit(self.inner.mask, lane.index().into())
    }
}

fn rank_of_bit(bits: u64, i: u32) -> Option<usize> {
    if i >= 64 || (bits & (1u64 << i)) == 0 {
        return None;
    }
    let below = if i == 0 { 0 } else { bits & ((1u64 << i) - 1) };
    Some((below.count_ones() + 1) as usize)
}
