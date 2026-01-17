#[derive(Clone)]
pub struct DescriptorIndexingFeatures {
    pub runtime_descriptor_array: bool,
    pub descriptor_binding_partially_bound: bool,
    pub descriptor_binding_variable_descriptor_count: bool,
    pub descriptor_binding_update_unused_while_pending: bool,
    pub shader_sampled_image_array_non_uniform_indexing: bool,
    pub shader_storage_buffer_array_non_uniform_indexing: bool,
    pub shader_storage_image_array_non_uniform_indexing: bool,
}

impl Default for DescriptorIndexingFeatures {
    fn default() -> Self {
        Self {
            runtime_descriptor_array: true,
            descriptor_binding_partially_bound: true,
            descriptor_binding_variable_descriptor_count: true,
            descriptor_binding_update_unused_while_pending: true,
            shader_sampled_image_array_non_uniform_indexing: true,
            shader_storage_buffer_array_non_uniform_indexing: true,
            shader_storage_image_array_non_uniform_indexing: true,
        }
    }
}

#[derive(Clone)]
pub struct GpuDeviceFeatures {
    pub dynamic_rendering: bool,
    pub synchronization2: bool,
    pub buffer_device_address: bool,
    pub descriptor_indexing: DescriptorIndexingFeatures,
}

impl Default for GpuDeviceFeatures {
    fn default() -> Self {
        Self {
            dynamic_rendering: true,
            synchronization2: true,
            buffer_device_address: true,
            descriptor_indexing: DescriptorIndexingFeatures::default(),
        }
    }
}

impl GpuDeviceFeatures {
    pub fn vulkan13_default() -> Self {
        Self::default()
    }
}
