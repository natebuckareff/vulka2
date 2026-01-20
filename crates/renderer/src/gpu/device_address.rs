use bytemuck::{Pod, Zeroable};
use crevice::{
    std140::{AsStd140, Std140},
    std430::{AsStd430, Std430},
};

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceAddress(pub u64);

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct UInt64(pub u64);

unsafe impl Zeroable for UInt64 {}
unsafe impl Pod for UInt64 {}

/// SAFETY: for both
/// - `#[repr(transparent)]` over `u64` means identical layout (size 8)
/// - alignment for a 64-bit scalar is 8 bytes
unsafe impl Std140 for UInt64 {
    const ALIGNMENT: usize = 8;
}
unsafe impl Std430 for UInt64 {
    const ALIGNMENT: usize = 8;
}

impl AsStd140 for DeviceAddress {
    type Output = UInt64;

    #[inline]
    fn as_std140(&self) -> Self::Output {
        UInt64(self.0)
    }

    #[inline]
    fn from_std140(val: Self::Output) -> Self {
        DeviceAddress(val.0)
    }
}

impl AsStd430 for DeviceAddress {
    type Output = UInt64;

    #[inline]
    fn as_std430(&self) -> Self::Output {
        UInt64(self.0)
    }

    #[inline]
    fn from_std430(val: Self::Output) -> Self {
        DeviceAddress(val.0)
    }
}
