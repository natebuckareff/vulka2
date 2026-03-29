use std::hash::{Hash, Hasher};

use crate::gpu::Range;

// TODO: overhashing?
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Allocation {
    handle: AllocHandle,
    range: Range,
}

impl Allocation {
    pub fn new(handle: AllocHandle, range: Range) -> Self {
        Self { handle, range }
    }

    pub fn handle(&self) -> AllocHandle {
        self.handle
    }

    pub fn subrange(&self) -> Range {
        self.range
    }
}

#[repr(C)]
#[derive(Clone, Copy, Eq)]
pub union AllocHandle {
    id: u64,
    record: SlotRecord,
}

impl AllocHandle {
    pub fn dummy() -> Self {
        Self::from_id(u64::MAX)
    }

    pub fn from_id(id: u64) -> Self {
        Self { id }
    }

    pub fn from_slot(slot: u32, generation: u32) -> Self {
        let record = SlotRecord { slot, generation };
        Self { record }
    }

    pub fn id(&self) -> u64 {
        unsafe { self.id }
    }

    pub fn record(&self) -> &SlotRecord {
        unsafe { &self.record }
    }
}

impl PartialEq for AllocHandle {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.id == other.id }
    }
}

impl Hash for AllocHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe { self.id.hash(state) }
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SlotRecord {
    slot: u32,
    generation: u32,
}
