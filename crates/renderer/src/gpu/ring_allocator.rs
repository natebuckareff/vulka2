use std::collections::VecDeque;

use anyhow::{Result, anyhow};

use crate::gpu::{
    AlignedRange, AllocHandle, Allocation, AllocatorId, BufferAllocator, BufferStorage,
    BufferToken, Range, RetireQueue, StorageSpan,
};

pub struct RingAllocator<T: StorageSpan> {
    id: AllocatorId,
    storage: T,
    device_start: u64,
    device_end: u64,
    retirement: RetireQueue<Allocation>,
    acquired: Vec<Allocation>,
    allocations: VecDeque<Allocation>,
    next_id: u64,
}

impl<T: StorageSpan> RingAllocator<T> {
    pub fn new(storage: T) -> Result<Self> {
        let buffer = storage.span().buffer();
        let device = buffer.device().clone();
        let retirement = RetireQueue::new(device)?;
        Ok(Self {
            id: AllocatorId::new(),
            storage,
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
        let start = self.storage.align_relative(tail.start(), align);
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

impl<T: StorageSpan> BufferStorage for RingAllocator<T> {
    type Storage = T;

    fn id(&self) -> AllocatorId {
        self.id
    }

    fn storage(&self) -> &Self::Storage {
        &self.storage
    }

    fn free(self) -> Self::Storage {
        self.storage
    }
}

impl<T: StorageSpan> BufferAllocator for RingAllocator<T> {
    fn len(&self) -> u64 {
        self.capacity() - self.host_tail_range().size()
    }

    fn capacity(&self) -> u64 {
        self.storage.span().range().size()
    }

    fn acquire(&mut self, size: u64, align: Option<u64>) -> Result<Option<T>> {
        let align = align.unwrap_or(1);

        loop {
            if let Some(arange) = self.acquire_range(size, align)? {
                let id: u64 = self.next_id;
                let handle = AllocHandle::from_id(id);
                let range = arange.aligned();
                let span_range = range.add(self.storage.span().range().start())?;
                let span = self.storage.suballocate(self, handle, span_range)?;
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
