use anyhow::{Result, anyhow};
use vulkanalia_vma as vma;

use crate::gpu::{
    AlignedRange, AllocId, BufferBlock, BufferSpan, BufferToken, Range, RetireQueue, align_up,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RingHandle {
    id: u64,
    size: u64,
}

pub struct RingAllocator<Storage: Copy> {
    id: AllocId,
    storage: BufferSpan<Storage>,
    device_start: u64,
    device_end: u64,
    retirement: RetireQueue<RingHandle>,
    acquired: Vec<RingHandle>,
    first_id: u64,
    next_id: u64,
}

impl<Storage: Copy> RingAllocator<Storage> {
    pub fn new(storage: BufferSpan<Storage>) -> Result<Self> {
        storage.buffer().check_flags(
            vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vma::AllocationCreateFlags::MAPPED,
        )?;
        let device = storage.buffer().device().clone();
        let retirement = RetireQueue::new(device)?;
        Ok(Self {
            id: AllocId::new(),
            storage,
            device_start: 0,
            device_end: 0,
            retirement,
            acquired: vec![], // released handles waiting to be recycled
            first_id: 0,      // oldest id unretired id
            next_id: 0,       // next id to allocate for a new span
        })
    }

    // TODO: replace with BufferBlock trait methods for available()/len() and capacity()
    pub fn capacity(&self) -> u64 {
        self.storage.range().size()
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
            Range::new(0, self.capacity())
        } else if self.device_start < self.device_end {
            Range::new(self.device_end, self.capacity())
        } else {
            Range::new(self.device_end, self.device_start)
        }
    }

    fn reclaim(&mut self) -> bool {
        let mut reclaimed = false;
        'consume: loop {
            for i in 0..self.acquired.len() {
                let handle = &self.acquired[i];
                if handle.id == self.first_id {
                    self.device_start = (self.device_start + handle.size) % self.capacity();
                    self.first_id += 1;
                    self.acquired.swap_remove(i);
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
    pub fn retire(&mut self, token: BufferToken<RingHandle>) -> Result<()> {
        if !self.owns_token(&token) {
            return Err(anyhow!("allocator mismatch"));
        }
        let (_, retire) = token.parts();
        self.retirement.retire(retire)
    }
}

impl<Storage: Copy> BufferBlock for RingAllocator<Storage> {
    type Storage = Storage;
    type Handle = RingHandle;

    fn id(&self) -> AllocId {
        self.id
    }

    fn acquire(
        &mut self,
        size: u64,
        align: Option<u64>,
    ) -> Result<Option<BufferSpan<Self::Handle>>> {
        let align = align.unwrap_or(1);

        loop {
            if let Some(range) = self.acquire_range(size, align)? {
                let buffer = self.storage.buffer().clone();
                let id = self.next_id;
                let size = range.full().size();
                let handle = RingHandle { id, size };
                let offset = self.storage.range().start();
                let span_range = range.aligned().add(offset)?;
                let span = BufferSpan::new(Some(self.id), buffer, handle, span_range);
                self.next_id += 1;
                self.device_end = range.full().end();
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

    fn free(self) -> BufferSpan<Self::Storage> {
        self.storage
    }
}
