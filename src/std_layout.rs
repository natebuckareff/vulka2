use bytemuck::{Pod, Zeroable};

#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct Std140U64(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct Std430U64(pub u64);

unsafe impl Zeroable for Std140U64 {}
unsafe impl Pod for Std140U64 {}
unsafe impl Zeroable for Std430U64 {}
unsafe impl Pod for Std430U64 {}

unsafe impl crevice::std140::Std140 for Std140U64 {
    const ALIGNMENT: usize = 8;
}

unsafe impl crevice::std430::Std430 for Std430U64 {
    const ALIGNMENT: usize = 8;
}

impl From<u64> for Std140U64 {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<Std140U64> for u64 {
    fn from(value: Std140U64) -> Self {
        value.0
    }
}

impl From<u64> for Std430U64 {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<Std430U64> for u64 {
    fn from(value: Std430U64) -> Self {
        value.0
    }
}
