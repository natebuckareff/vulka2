use std::collections::VecDeque;

use anyhow::{Result, anyhow};
use vulkanalia_vma as vma;

use crate::gpu::{
    AlignedRange, AllocHandle, Allocation, AllocatorId, BufferAllocator, BufferSpan, BufferStorage,
    BufferToken, Range, RetireQueue, align_up,
};

pub struct RingAllocator {
    id: AllocatorId,
    backing: BufferSpan,
    device_start: u64,
    device_end: u64,
    retirement: RetireQueue<Allocation>,
    acquired: Vec<Allocation>,
    allocations: VecDeque<Allocation>,
    next_id: u64,
}

impl RingAllocator {
    pub fn new(backing: BufferSpan) -> Result<Self> {
        let buffer = backing.buffer();
        buffer.check_flags(
            vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vma::AllocationCreateFlags::MAPPED,
        )?;
        let device = buffer.device().clone();
        let retirement = RetireQueue::new(device)?;
        Ok(Self {
            id: AllocatorId::new(),
            backing,
            device_start: 0,
            device_end: 0,
            retirement,
            acquired: vec![],             // released handles waiting to be recycled
            allocations: VecDeque::new(), // outstand, unretired allocations with full, unaligned size
            next_id: 0,                   // next id to allocate for a new span
        })
    }

    fn acquire_range(&mut self, size: u64, align: u64) -> Result<Option<AlignedRange>> {
        let tail = self.host_tail_range();
        let start = align_up(tail.start(), align);
        let aligned = Range::sized(start, size)?;
        let request = AlignedRange::new(tail.start(), aligned);
        if tail.fits(request.aligned()) {
            Ok(Some(request))
        } else {
            Ok(None)
        }
    }

    // get the next full range that may be allocated from
    fn host_tail_range(&self) -> Range {
        if self.device_start == self.device_end {
            Range::new(0, self.capacity() as u64)
        } else if self.device_start < self.device_end {
            Range::new(self.device_end, self.capacity())
        } else {
            Range::new(self.device_end, self.device_start)
        }
    }

    // TODO: is this correct?
    fn reclaim(&mut self) -> bool {
        let mut reclaimed = false;
        'consume: loop {
            for i in 0..self.acquired.len() {
                let Some(oldest) = self.allocations.front() else {
                    break 'consume;
                };
                let allocation = &self.acquired[i];
                if allocation.handle().id() == oldest.handle().id() {
                    let size = allocation.subrange().size() as u64;
                    self.device_start = (self.device_start + size) % (self.capacity() as u64);
                    self.acquired.swap_remove(i);
                    self.allocations.pop_front();
                    reclaimed = true;
                    continue 'consume;
                }
            }
            break;
        }
        reclaimed
    }

    // TODO: should rename from release->retire in a lot of places to match
    // general API pattern
    pub fn retire(&mut self, token: BufferToken) -> Result<()> {
        if token.allocator() != self.id() {
            return Err(anyhow!("allocator mismatch"));
        }
        let retire = token.into_retire();
        self.retirement.retire(retire)
    }
}

impl BufferStorage for RingAllocator {
    fn id(&self) -> AllocatorId {
        self.id
    }

    fn backing(&self) -> &BufferSpan {
        &self.backing
    }

    fn free(self) -> BufferSpan {
        self.backing
    }
}

impl BufferAllocator for RingAllocator {
    fn len(&self) -> u64 {
        self.capacity() - self.host_tail_range().size()
    }

    fn capacity(&self) -> u64 {
        self.backing.range().size()
    }

    fn acquire(&mut self, size: u64, align: Option<u64>) -> Result<Option<BufferSpan>> {
        let align = align.unwrap_or(1);

        loop {
            if let Some(arange) = self.acquire_range(size, align)? {
                let id = self.next_id;
                let handle = AllocHandle::from_id(id);
                let range = arange.aligned();
                let span = BufferSpan::from_allocator(self, handle, range);
                let allocation = Allocation::new(handle, arange.full());
                self.next_id += 1;
                self.device_end = arange.full().end();
                self.allocations.push_back(allocation);
                return Ok(Some(span));
            }

            let Some(handle) = self.retirement.acquire()? else {
                return Ok(None);
            };

            self.acquired.push(handle);

            if !self.reclaim() {
                return Ok(None);
            }
        }
    }
}
