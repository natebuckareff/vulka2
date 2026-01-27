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
        match binding_type {
            slang::BindingType::Unknown => "Unknown",
            slang::BindingType::Sampler => "Sampler",
            slang::BindingType::Texture => "Texture",
            slang::BindingType::ConstantBuffer => "ConstantBuffer",
            slang::BindingType::ParameterBlock => "ParameterBlock",
            slang::BindingType::TypedBuffer => "TypedBuffer",
            slang::BindingType::RawBuffer => "RawBuffer",
            slang::BindingType::CombinedTextureSampler => "CombinedTextureSampler",
            slang::BindingType::InputRenderTarget => "InputRenderTarget",
            slang::BindingType::InlineUniformData => "InlineUniformData",
            slang::BindingType::RayTracingAccelerationStructure => "RayTracingAccelerationStructure",
            slang::BindingType::VaryingInput => "VaryingInput",
            slang::BindingType::VaryingOutput => "VaryingOutput",
            slang::BindingType::ExistentialValue => "ExistentialValue",
            slang::BindingType::PushConstant => "PushConstant",
            slang::BindingType::MutableFlag => "MutableFlag",
            slang::BindingType::MutableTeture => "MutableTeture",
            slang::BindingType::MutableTypedBuffer => "MutableTypedBuffer",
            slang::BindingType::MutableRawBuffer => "MutableRawBuffer",
            slang::BindingType::BaseMask => "BaseMask",
            slang::BindingType::ExtMask => "ExtMask",
        }
    }

    fn binding_type_from_str(value: &str) -> Option<slang::BindingType> {
        Some(match value {
            "Unknown" => slang::BindingType::Unknown,
            "Sampler" => slang::BindingType::Sampler,
            "Texture" => slang::BindingType::Texture,
            "ConstantBuffer" => slang::BindingType::ConstantBuffer,
            "ParameterBlock" => slang::BindingType::ParameterBlock,
            "TypedBuffer" => slang::BindingType::TypedBuffer,
            "RawBuffer" => slang::BindingType::RawBuffer,
            "CombinedTextureSampler" => slang::BindingType::CombinedTextureSampler,
            "InputRenderTarget" => slang::BindingType::InputRenderTarget,
            "InlineUniformData" => slang::BindingType::InlineUniformData,
            "RayTracingAccelerationStructure" => {
                slang::BindingType::RayTracingAccelerationStructure
            }
            "VaryingInput" => slang::BindingType::VaryingInput,
            "VaryingOutput" => slang::BindingType::VaryingOutput,
            "ExistentialValue" => slang::BindingType::ExistentialValue,
            "PushConstant" => slang::BindingType::PushConstant,
            "MutableFlag" => slang::BindingType::MutableFlag,
            "MutableTeture" => slang::BindingType::MutableTeture,
            "MutableTypedBuffer" => slang::BindingType::MutableTypedBuffer,
            "MutableRawBuffer" => slang::BindingType::MutableRawBuffer,
            "BaseMask" => slang::BindingType::BaseMask,
            "ExtMask" => slang::BindingType::ExtMask,
            _ => return None,
        })
    }
}
