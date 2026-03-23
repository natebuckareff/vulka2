use serde::{Deserialize, Serialize};
use shader_slang;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum SlangBindingType {
    Unknown,
    Sampler,
    Texture,
    ConstantBuffer,
    ParameterBlock,
    TypedBuffer,
    RawBuffer,
    CombinedTextureSampler,
    InputRenderTarget,
    InlineUniformData,
    RayTracingAccelerationStructure,
    VaryingInput,
    VaryingOutput,
    ExistentialValue,
    PushConstant,
    MutableFlag,
    MutableTexture,
    MutableTypedBuffer,
    MutableRawBuffer,
    BaseMask,
    ExtMask,
}

impl From<shader_slang::BindingType> for SlangBindingType {
    fn from(value: shader_slang::BindingType) -> Self {
        use shader_slang::BindingType::*;
        match value {
            Unknown => Self::Unknown,
            Sampler => Self::Sampler,
            Texture => Self::Texture,
            ConstantBuffer => Self::ConstantBuffer,
            ParameterBlock => Self::ParameterBlock,
            TypedBuffer => Self::TypedBuffer,
            RawBuffer => Self::RawBuffer,
            CombinedTextureSampler => Self::CombinedTextureSampler,
            InputRenderTarget => Self::InputRenderTarget,
            InlineUniformData => Self::InlineUniformData,
            RayTracingAccelerationStructure => Self::RayTracingAccelerationStructure,
            VaryingInput => Self::VaryingInput,
            VaryingOutput => Self::VaryingOutput,
            ExistentialValue => Self::ExistentialValue,
            PushConstant => Self::PushConstant,
            MutableFlag => Self::MutableFlag,
            MutableTeture => Self::MutableTexture,
            MutableTypedBuffer => Self::MutableTypedBuffer,
            MutableRawBuffer => Self::MutableRawBuffer,
            BaseMask => Self::BaseMask,
            ExtMask => Self::ExtMask,
        }
    }
}

impl From<SlangBindingType> for shader_slang::BindingType {
    fn from(value: SlangBindingType) -> Self {
        use SlangBindingType::*;
        match value {
            Unknown => Self::Unknown,
            Sampler => Self::Sampler,
            Texture => Self::Texture,
            ConstantBuffer => Self::ConstantBuffer,
            ParameterBlock => Self::ParameterBlock,
            TypedBuffer => Self::TypedBuffer,
            RawBuffer => Self::RawBuffer,
            CombinedTextureSampler => Self::CombinedTextureSampler,
            InputRenderTarget => Self::InputRenderTarget,
            InlineUniformData => Self::InlineUniformData,
            RayTracingAccelerationStructure => Self::RayTracingAccelerationStructure,
            VaryingInput => Self::VaryingInput,
            VaryingOutput => Self::VaryingOutput,
            ExistentialValue => Self::ExistentialValue,
            PushConstant => Self::PushConstant,
            MutableFlag => Self::MutableFlag,
            MutableTexture => Self::MutableTeture,
            MutableTypedBuffer => Self::MutableTypedBuffer,
            MutableRawBuffer => Self::MutableRawBuffer,
            BaseMask => Self::BaseMask,
            ExtMask => Self::ExtMask,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum SlangResourceShape {
    ResourceBaseShapeMask,
    ResourceNone,
    Texture1d,
    Texture2d,
    Texture3d,
    TextureCube,
    TextureBuffer,
    StructuredBuffer,
    ByteAddressBuffer,
    ResourceUnknown,
    AccelerationStructure,
    TextureSubpass,
    ResourceExtShapeMask,
    TextureFeedbackFlag,
    TextureShadowFlag,
    TextureArrayFlag,
    TextureMultisampleFlag,
    TextureCombinedFlag,
    Texture1dArray,
    Texture2dArray,
    TextureCubeArray,
    Texture2dMultisample,
    Texture2dMultisampleArray,
    TextureSubpassMultisample,
}

impl From<shader_slang::ResourceShape> for SlangResourceShape {
    fn from(value: shader_slang::ResourceShape) -> Self {
        use shader_slang::ResourceShape::*;
        match value {
            SlangResourceBaseShapeMask => Self::ResourceBaseShapeMask,
            SlangResourceNone => Self::ResourceNone,
            SlangTexture1d => Self::Texture1d,
            SlangTexture2d => Self::Texture2d,
            SlangTexture3d => Self::Texture3d,
            SlangTextureCube => Self::TextureCube,
            SlangTextureBuffer => Self::TextureBuffer,
            SlangStructuredBuffer => Self::StructuredBuffer,
            SlangByteAddressBuffer => Self::ByteAddressBuffer,
            SlangResourceUnknown => Self::ResourceUnknown,
            SlangAccelerationStructure => Self::AccelerationStructure,
            SlangTextureSubpass => Self::TextureSubpass,
            SlangResourceExtShapeMask => Self::ResourceExtShapeMask,
            SlangTextureFeedbackFlag => Self::TextureFeedbackFlag,
            SlangTextureShadowFlag => Self::TextureShadowFlag,
            SlangTextureArrayFlag => Self::TextureArrayFlag,
            SlangTextureMultisampleFlag => Self::TextureMultisampleFlag,
            SlangTextureCombinedFlag => Self::TextureCombinedFlag,
            SlangTexture1dArray => Self::Texture1dArray,
            SlangTexture2dArray => Self::Texture2dArray,
            SlangTextureCubeArray => Self::TextureCubeArray,
            SlangTexture2dMultisample => Self::Texture2dMultisample,
            SlangTexture2dMultisampleArray => Self::Texture2dMultisampleArray,
            SlangTextureSubpassMultisample => Self::TextureSubpassMultisample,
        }
    }
}

impl From<SlangResourceShape> for shader_slang::ResourceShape {
    fn from(value: SlangResourceShape) -> Self {
        use SlangResourceShape::*;
        match value {
            ResourceBaseShapeMask => Self::SlangResourceBaseShapeMask,
            ResourceNone => Self::SlangResourceNone,
            Texture1d => Self::SlangTexture1d,
            Texture2d => Self::SlangTexture2d,
            Texture3d => Self::SlangTexture3d,
            TextureCube => Self::SlangTextureCube,
            TextureBuffer => Self::SlangTextureBuffer,
            StructuredBuffer => Self::SlangStructuredBuffer,
            ByteAddressBuffer => Self::SlangByteAddressBuffer,
            ResourceUnknown => Self::SlangResourceUnknown,
            AccelerationStructure => Self::SlangAccelerationStructure,
            TextureSubpass => Self::SlangTextureSubpass,
            ResourceExtShapeMask => Self::SlangResourceExtShapeMask,
            TextureFeedbackFlag => Self::SlangTextureFeedbackFlag,
            TextureShadowFlag => Self::SlangTextureShadowFlag,
            TextureArrayFlag => Self::SlangTextureArrayFlag,
            TextureMultisampleFlag => Self::SlangTextureMultisampleFlag,
            TextureCombinedFlag => Self::SlangTextureCombinedFlag,
            Texture1dArray => Self::SlangTexture1dArray,
            Texture2dArray => Self::SlangTexture2dArray,
            TextureCubeArray => Self::SlangTextureCubeArray,
            Texture2dMultisample => Self::SlangTexture2dMultisample,
            Texture2dMultisampleArray => Self::SlangTexture2dMultisampleArray,
            TextureSubpassMultisample => Self::SlangTextureSubpassMultisample,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum SlangResourceAccess {
    None,
    Read,
    ReadWrite,
    RasterOrdered,
    Append,
    Consume,
    Write,
    Feedback,
    Unknown,
}

impl From<shader_slang::ResourceAccess> for SlangResourceAccess {
    fn from(value: shader_slang::ResourceAccess) -> Self {
        use shader_slang::ResourceAccess::*;
        match value {
            None => Self::None,
            Read => Self::Read,
            ReadWrite => Self::ReadWrite,
            RasterOrdered => Self::RasterOrdered,
            Append => Self::Append,
            Consume => Self::Consume,
            Write => Self::Write,
            Feedback => Self::Feedback,
            Unknown => Self::Unknown,
        }
    }
}

impl From<SlangResourceAccess> for shader_slang::ResourceAccess {
    fn from(value: SlangResourceAccess) -> Self {
        use SlangResourceAccess::*;
        match value {
            None => Self::None,
            Read => Self::Read,
            ReadWrite => Self::ReadWrite,
            RasterOrdered => Self::RasterOrdered,
            Append => Self::Append,
            Consume => Self::Consume,
            Write => Self::Write,
            Feedback => Self::Feedback,
            Unknown => Self::Unknown,
        }
    }
}
