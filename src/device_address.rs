use bytemuck::{Pod, Zeroable};
use crevice::std140::{AsStd140, Std140};

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceAddress(pub u64);

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Std140U64(pub u64);

unsafe impl Zeroable for Std140U64 {}
unsafe impl Pod for Std140U64 {}

/// SAFETY:
/// - `#[repr(transparent)]` over `u64` means identical layout (size 8)
/// - alignment for a 64-bit scalar is 8 bytes
unsafe impl Std140 for Std140U64 {
    const ALIGNMENT: usize = 8;
}

impl AsStd140 for DeviceAddress {
    type Output = Std140U64;

    #[inline]
    fn as_std140(&self) -> Self::Output {
        Std140U64(self.0)
    }

    #[inline]
    fn from_std140(val: Self::Output) -> Self {
        DeviceAddress(val.0)
    }
}
