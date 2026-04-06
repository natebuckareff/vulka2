use anyhow::{Result, anyhow};
use bytemuck::Pod;
use vulkanalia::vk;

use crate::gpu::{BufferMap, BufferToken, BufferView, FrameToken, LaneKey};

pub trait IndexElement: Pod {
    const INDEX_TYPE: vk::IndexType;
}

impl IndexElement for u16 {
    const INDEX_TYPE: vk::IndexType = vk::IndexType::UINT16;
}

impl IndexElement for u32 {
    const INDEX_TYPE: vk::IndexType = vk::IndexType::UINT32;
}

pub struct IndexView<T: IndexElement> {
    view: BufferView<T>,
}

impl<T: IndexElement> IndexView<T> {
    pub fn new(map: BufferMap) -> Result<Self> {
        let view = BufferView::new(map)?;
        if view.capacity() > u32::MAX as u64 {
            return Err(anyhow!("map size exceeds maximum number of index elements"));
        }
        Ok(Self { view })
    }

    pub fn len(&self) -> u64 {
        self.view.len()
    }

    pub fn capacity(&self) -> u64 {
        self.view.capacity()
    }

    pub fn set_len(&mut self, new_len: u64) -> Result<()> {
        self.view.set_len(new_len)
    }

    pub fn push(&mut self, value: T) -> Result<()> {
        self.view.push(value)
    }

    pub fn finish(self) -> Result<IndexToken> {
        let index_type = T::INDEX_TYPE;
        // SAFETY: safe because `view.capacity() <= u32::MAX`
        let index_count = self.view.len() as u32;
        let token = self.view.finish()?;
        Ok(IndexToken::new(token, index_type, index_count))
    }
}

impl<T: IndexElement> std::ops::Index<u64> for IndexView<T> {
    type Output = T;

    fn index(&self, index: u64) -> &Self::Output {
        &self.view[index]
    }
}

impl<T: IndexElement> std::ops::IndexMut<u64> for IndexView<T> {
    fn index_mut(&mut self, index: u64) -> &mut Self::Output {
        &mut self.view[index]
    }
}

impl<T: IndexElement> std::ops::Index<std::ops::Range<u64>> for IndexView<T> {
    type Output = [T];

    fn index(&self, range: std::ops::Range<u64>) -> &Self::Output {
        &self.view[range]
    }
}

impl<T: IndexElement> std::ops::IndexMut<std::ops::Range<u64>> for IndexView<T> {
    fn index_mut(&mut self, range: std::ops::Range<u64>) -> &mut Self::Output {
        &mut self.view[range]
    }
}

pub struct IndexToken {
    token: BufferToken,
    index_type: vk::IndexType,
    index_count: u32,
}

impl IndexToken {
    fn new(token: BufferToken, index_type: vk::IndexType, index_count: u32) -> Self {
        Self {
            token,
            index_type,
            index_count,
        }
    }

    pub fn token(&self) -> &BufferToken {
        &self.token
    }

    pub fn index_type(&self) -> vk::IndexType {
        self.index_type
    }

    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    // TODO: trait?
    pub fn touch(&mut self, key: LaneKey, frame: &FrameToken) {
        self.token.touch(key, frame)
    }
}
