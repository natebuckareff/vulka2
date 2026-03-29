use anyhow::{Context, Result};

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range {
    start: u64,
    end: u64,
}

impl Range {
    pub fn new(start: u64, end: u64) -> Self {
        debug_assert!(start <= end, "invalid range");
        Self { start, end }
    }

    pub fn sized(start: u64, size: u64) -> Result<Self> {
        let end = start.checked_add(size).context("range overflow")?;
        Ok(Self { start, end })
    }

    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn end(&self) -> u64 {
        self.end
    }

    pub fn size(&self) -> u64 {
        // OVERFLOW: safe as long as invariant start <= end
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end == self.start
    }

    pub fn fits(&self, other: Range) -> bool {
        other.start >= self.start && other.end <= self.end
    }

    pub fn clamp(&self, other: Range) -> Self {
        let start = self.start.clamp(other.start, other.end);
        let end = self.end.clamp(other.start, other.end);
        Range { start, end }
    }

    pub fn add(&self, offset: u64) -> Result<Self> {
        Ok(Self {
            start: self
                .start
                .checked_add(offset)
                .context("range add overflow")?,
            end: self.end.checked_add(offset).context("range add overflow")?,
        })
    }

    pub fn sub(&self, offset: u64) -> Result<Self> {
        Ok(Self {
            start: self
                .start
                .checked_sub(offset)
                .context("range sub overflow")?,
            end: self.end.checked_sub(offset).context("range sub overflow")?,
        })
    }
}

pub struct AlignedRange {
    start: u64,
    aligned: Range,
}

impl AlignedRange {
    pub fn new(start: u64, aligned: Range) -> Self {
        debug_assert!(start <= aligned.start);
        Self { start, aligned }
    }

    pub fn full(&self) -> Range {
        Range::new(self.start, self.aligned.end)
    }

    pub fn aligned(&self) -> Range {
        self.aligned
    }
}
