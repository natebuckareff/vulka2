/// Serde support for `vk::ShaderStageFlags` (serialized as u32).
pub(crate) mod serde_shader_stage_flags {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use vulkanalia::vk;

    pub fn serialize<S: Serializer>(
        flags: &vk::ShaderStageFlags,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        flags.bits().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<vk::ShaderStageFlags, D::Error> {
        let bits = u32::deserialize(deserializer)?;
        Ok(vk::ShaderStageFlags::from_bits_truncate(bits))
    }
}

/// Serde support for `vk::DescriptorBindingFlags` (serialized as u32).
pub(crate) mod serde_descriptor_binding_flags {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use vulkanalia::vk;

    pub fn serialize<S: Serializer>(
        flags: &vk::DescriptorBindingFlags,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        flags.bits().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<vk::DescriptorBindingFlags, D::Error> {
        let bits = u32::deserialize(deserializer)?;
        Ok(vk::DescriptorBindingFlags::from_bits_truncate(bits))
    }
}

/// Serde support for `vk::Format` (serialized as i32).
pub(crate) mod serde_format {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use vulkanalia::vk;

    pub fn serialize<S: Serializer>(format: &vk::Format, serializer: S) -> Result<S::Ok, S::Error> {
        format.as_raw().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<vk::Format, D::Error> {
        let raw = i32::deserialize(deserializer)?;
        Ok(vk::Format::from_raw(raw))
    }
}
