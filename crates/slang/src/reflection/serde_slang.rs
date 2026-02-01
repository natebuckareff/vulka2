/// Serde support for `slang::BindingType` (serialized as string).
pub(crate) mod serde_binding_type {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use shader_slang as slang;

    pub fn serialize<S: Serializer>(
        binding_type: &slang::BindingType,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        binding_type_to_str(*binding_type).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<slang::BindingType, D::Error> {
        let value = String::deserialize(deserializer)?;
        binding_type_from_str(&value).ok_or_else(|| {
            serde::de::Error::custom(format!("unknown Slang binding type '{value}'"))
        })
    }

    fn binding_type_to_str(binding_type: slang::BindingType) -> &'static str {
        use slang::BindingType::*;
        match binding_type {
            Unknown => "Unknown",
            Sampler => "Sampler",
            Texture => "Texture",
            ConstantBuffer => "ConstantBuffer",
            ParameterBlock => "ParameterBlock",
            TypedBuffer => "TypedBuffer",
            RawBuffer => "RawBuffer",
            CombinedTextureSampler => "CombinedTextureSampler",
            InputRenderTarget => "InputRenderTarget",
            InlineUniformData => "InlineUniformData",
            RayTracingAccelerationStructure => "RayTracingAccelerationStructure",
            VaryingInput => "VaryingInput",
            VaryingOutput => "VaryingOutput",
            ExistentialValue => "ExistentialValue",
            PushConstant => "PushConstant",
            MutableFlag => "MutableFlag",
            MutableTeture => "MutableTeture",
            MutableTypedBuffer => "MutableTypedBuffer",
            MutableRawBuffer => "MutableRawBuffer",
            BaseMask => "BaseMask",
            ExtMask => "ExtMask",
        }
    }

    fn binding_type_from_str(value: &str) -> Option<slang::BindingType> {
        use slang::BindingType::*;
        Some(match value {
            "Unknown" => Unknown,
            "Sampler" => Sampler,
            "Texture" => Texture,
            "ConstantBuffer" => ConstantBuffer,
            "ParameterBlock" => ParameterBlock,
            "TypedBuffer" => TypedBuffer,
            "RawBuffer" => RawBuffer,
            "CombinedTextureSampler" => CombinedTextureSampler,
            "InputRenderTarget" => InputRenderTarget,
            "InlineUniformData" => InlineUniformData,
            "RayTracingAccelerationStructure" => RayTracingAccelerationStructure,
            "VaryingInput" => VaryingInput,
            "VaryingOutput" => VaryingOutput,
            "ExistentialValue" => ExistentialValue,
            "PushConstant" => PushConstant,
            "MutableFlag" => MutableFlag,
            "MutableTeture" => MutableTeture,
            "MutableTypedBuffer" => MutableTypedBuffer,
            "MutableRawBuffer" => MutableRawBuffer,
            "BaseMask" => BaseMask,
            "ExtMask" => ExtMask,
            _ => return None,
        })
    }
}

pub(crate) mod serde_resource_access {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use shader_slang as slang;

    pub fn serialize<S: Serializer>(
        resource_access: &slang::ResourceAccess,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        resource_access_to_str(*resource_access).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<slang::ResourceAccess, D::Error> {
        let value = String::deserialize(deserializer)?;
        resource_access_from_str(&value).ok_or_else(|| {
            serde::de::Error::custom(format!("unknown Slang resource access '{value}'"))
        })
    }

    fn resource_access_to_str(resource_access: slang::ResourceAccess) -> &'static str {
        use slang::ResourceAccess::*;
        match resource_access {
            None => "None",
            Read => "Read",
            ReadWrite => "ReadWrite",
            RasterOrdered => "RasterOrdered",
            Append => "Append",
            Consume => "Consume",
            Write => "Write",
            Feedback => "Feedback",
            Unknown => "Unknown",
        }
    }

    fn resource_access_from_str(value: &str) -> Option<slang::ResourceAccess> {
        use slang::ResourceAccess::*;
        Some(match value {
            "None" => None,
            "Read" => Read,
            "ReadWrite" => ReadWrite,
            "RasterOrdered" => RasterOrdered,
            "Append" => Append,
            "Consume" => Consume,
            "Write" => Write,
            "Feedback" => Feedback,
            "Unknown" => Unknown,
            _ => return Option::None,
        })
    }
}

pub(crate) mod serde_resource_shape {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use shader_slang as slang;

    pub fn serialize<S: Serializer>(
        resource_shape: &slang::ResourceShape,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        resource_shape_to_str(*resource_shape).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<slang::ResourceShape, D::Error> {
        let value = String::deserialize(deserializer)?;
        resource_shape_from_str(&value).ok_or_else(|| {
            serde::de::Error::custom(format!("unknown Slang resource shape '{value}'"))
        })
    }

    fn resource_shape_to_str(resource_shape: slang::ResourceShape) -> &'static str {
        use slang::ResourceShape::*;
        match resource_shape {
            SlangResourceBaseShapeMask => "SlangResourceBaseShapeMask",
            SlangResourceNone => "SlangResourceNone",
            SlangTexture1d => "SlangTexture1d",
            SlangTexture2d => "SlangTexture2d",
            SlangTexture3d => "SlangTexture3d",
            SlangTextureCube => "SlangTextureCube",
            SlangTextureBuffer => "SlangTextureBuffer",
            SlangStructuredBuffer => "SlangStructuredBuffer",
            SlangByteAddressBuffer => "SlangByteAddressBuffer",
            SlangResourceUnknown => "SlangResourceUnknown",
            SlangAccelerationStructure => "SlangAccelerationStructure",
            SlangTextureSubpass => "SlangTextureSubpass",
            SlangResourceExtShapeMask => "SlangResourceExtShapeMask",
            SlangTextureFeedbackFlag => "SlangTextureFeedbackFlag",
            SlangTextureShadowFlag => "SlangTextureShadowFlag",
            SlangTextureArrayFlag => "SlangTextureArrayFlag",
            SlangTextureMultisampleFlag => "SlangTextureMultisampleFlag",
            SlangTextureCombinedFlag => "SlangTextureCombinedFlag",
            SlangTexture1dArray => "SlangTexture1dArray",
            SlangTexture2dArray => "SlangTexture2dArray",
            SlangTextureCubeArray => "SlangTextureCubeArray",
            SlangTexture2dMultisample => "SlangTexture2dMultisample",
            SlangTexture2dMultisampleArray => "SlangTexture2dMultisampleArray",
            SlangTextureSubpassMultisample => "SlangTextureSubpassMultisample",
        }
    }

    fn resource_shape_from_str(value: &str) -> Option<slang::ResourceShape> {
        use slang::ResourceShape::*;
        Some(match value {
            "SlangResourceBaseShapeMask" => SlangResourceBaseShapeMask,
            "SlangResourceNone" => SlangResourceNone,
            "SlangTexture1d" => SlangTexture1d,
            "SlangTexture2d" => SlangTexture2d,
            "SlangTexture3d" => SlangTexture3d,
            "SlangTextureCube" => SlangTextureCube,
            "SlangTextureBuffer" => SlangTextureBuffer,
            "SlangStructuredBuffer" => SlangStructuredBuffer,
            "SlangByteAddressBuffer" => SlangByteAddressBuffer,
            "SlangResourceUnknown" => SlangResourceUnknown,
            "SlangAccelerationStructure" => SlangAccelerationStructure,
            "SlangTextureSubpass" => SlangTextureSubpass,
            "SlangResourceExtShapeMask" => SlangResourceExtShapeMask,
            "SlangTextureFeedbackFlag" => SlangTextureFeedbackFlag,
            "SlangTextureShadowFlag" => SlangTextureShadowFlag,
            "SlangTextureArrayFlag" => SlangTextureArrayFlag,
            "SlangTextureMultisampleFlag" => SlangTextureMultisampleFlag,
            "SlangTextureCombinedFlag" => SlangTextureCombinedFlag,
            "SlangTexture1dArray" => SlangTexture1dArray,
            "SlangTexture2dArray" => SlangTexture2dArray,
            "SlangTextureCubeArray" => SlangTextureCubeArray,
            "SlangTexture2dMultisample" => SlangTexture2dMultisample,
            "SlangTexture2dMultisampleArray" => SlangTexture2dMultisampleArray,
            "SlangTextureSubpassMultisample" => SlangTextureSubpassMultisample,
            _ => return Option::None,
        })
    }
}
