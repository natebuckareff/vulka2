use anyhow::Result;
use std::sync::Arc;

use vulkanalia::vk;
use winit::window::Window;

use crate::gal::engine::{Engine, EngineCreateInfo};

#[derive(Default)]
pub struct EngineBuilder {
    application_name: Option<String>,
    application_version: Option<u32>,
    enable_validation_layers: Option<ValidationFeatures>,
    debug_message_types: Option<vk::DebugUtilsMessageTypeFlagsEXT>,
    window: Option<Arc<Window>>,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn application_name(mut self, name: String) -> Self {
        self.application_name = Some(name);
        self
    }

    pub fn application_version(mut self, version: u32) -> Self {
        self.application_version = Some(version);
        self
    }

    pub fn enable_best_practices(mut self) -> Self {
        self.validation_features_mut().best_practices = true;
        self
    }

    pub fn enable_debug_printf(mut self) -> Self {
        self.validation_features_mut().debug_printf = true;
        self
    }

    pub fn enable_gpu_assisted(mut self) -> Self {
        self.validation_features_mut().gpu_assisted = true;
        self
    }

    pub fn enable_sync_validation(mut self) -> Self {
        self.validation_features_mut().synchronization_validation = true;
        self
    }

    pub fn enable_general_debug_messages(self) -> Self {
        self.enable_debug_message_type(vk::DebugUtilsMessageTypeFlagsEXT::GENERAL)
    }

    pub fn enable_validation_debug_messages(self) -> Self {
        self.enable_debug_message_type(vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION)
    }

    pub fn enable_performance_debug_messages(self) -> Self {
        self.enable_debug_message_type(vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE)
    }

    pub fn enable_device_address_binding_debug_messages(self) -> Self {
        self.enable_debug_message_type(vk::DebugUtilsMessageTypeFlagsEXT::DEVICE_ADDRESS_BINDING)
    }

    pub fn window(mut self, window: Arc<Window>) -> Self {
        self.window = Some(window);
        self
    }

    fn validation_features_mut(&mut self) -> &mut ValidationFeatures {
        self.enable_validation_layers
            .get_or_insert_with(ValidationFeatures::default)
    }

    fn enable_debug_message_type(
        mut self,
        message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    ) -> Self {
        let message_types = self
            .debug_message_types
            .get_or_insert_with(vk::DebugUtilsMessageTypeFlagsEXT::empty);
        *message_types |= message_type;
        self
    }

    pub fn build(self) -> Result<Engine> {
        let create_info = EngineCreateInfo {
            application_name: self.application_name,
            application_version: self.application_version,
            validation_features: self.enable_validation_layers,
            debug_message_types: self.debug_message_types,
            window: self.window,
        };
        Engine::new(create_info)
    }
}

#[derive(Default, Clone, Copy)]
pub struct ValidationFeatures {
    pub best_practices: bool,
    pub debug_printf: bool,
    pub gpu_assisted: bool,
    pub synchronization_validation: bool,
}
