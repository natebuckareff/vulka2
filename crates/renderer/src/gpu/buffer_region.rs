use anyhow::{Result, anyhow};
use vulkanalia_vma as vma;

use crate::gpu::{AllocId, BufferSpan, BufferToken, RetireRecord};

enum BufferRegionState {
    Ready(BufferSpan<()>),
    Acquired,
    Retired(RetireRecord<()>),
}

pub struct BufferRegion<Storage: Copy> {
    id: AllocId,
    storage: BufferSpan<Storage>,
    state: BufferRegionState,
}

impl<Storage: Copy> BufferRegion<Storage> {
    pub fn new(storage: BufferSpan<Storage>) -> Result<Self> {
        let buffer = storage.buffer();
        buffer.check_flags(
            vma::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
                | vma::AllocationCreateFlags::MAPPED,
        )?;
        let id = AllocId::new();
        let range = storage.range();
        let span = BufferSpan::new(Some(id), buffer.clone(), (), range);
        Ok(Self {
            id,
            storage,
            state: BufferRegionState::Ready(span),
        })
    }

    pub fn acquire(&mut self) -> Result<Option<BufferSpan<()>>> {
        let buffer = self.storage.buffer();
        let device = buffer.device();
        let state = std::mem::replace(&mut self.state, BufferRegionState::Acquired);
        match state {
            BufferRegionState::Ready(span) => Ok(Some(span)),
            BufferRegionState::Acquired => Ok(None),
            BufferRegionState::Retired(record) => {
                let result = record.is_complete(device);
                if matches!(result, Ok(true)) {
                    Ok(Some(self.acquire_span()))
                } else {
                    self.state = BufferRegionState::Retired(record);
                    result?;
                    Ok(None)
                }
            }
        }
    }

    pub fn retire(&mut self, token: BufferToken<()>) -> Result<()> {
        if token.id() != Some(self.id) {
            return Err(anyhow!("buffer span mismatch"));
        }
        if !matches!(self.state, BufferRegionState::Acquired) {
            return Err(anyhow!("buffer region already retired"));
        }
        let (_, retire) = token.parts();
        let record = retire.retire()?;
        let result = record.is_complete(self.storage.buffer().device());
        if matches!(result, Ok(true)) {
            self.state = BufferRegionState::Ready(self.acquire_span())
        } else {
            self.state = BufferRegionState::Retired(record);
            result?;
        }
        Ok(())
    }

    fn acquire_span(&self) -> BufferSpan<()> {
        let buffer = self.storage.buffer().clone();
        let range = self.storage.range();
        BufferSpan::new(Some(self.id), buffer, (), range)
    }
}
