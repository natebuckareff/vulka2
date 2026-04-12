use std::sync::Arc;

use anyhow::Result;
use vulkanalia::vk;
use vulkanalia::vk::ExtDebugUtilsExtensionInstanceCommands;
use winit::window::Window;

use crate::gal::{engine_builder::ValidationFeatures, engine_vk};

pub(crate) struct EngineCreateInfo {
    pub(crate) application_name: Option<String>,
    pub(crate) application_version: Option<u32>,
    pub(crate) validation_features: Option<ValidationFeatures>,
    pub(crate) debug_message_types: Option<vk::DebugUtilsMessageTypeFlagsEXT>,
    pub(crate) window: Option<Arc<Window>>,
}

pub struct Engine {
    entry: vulkanalia::Entry,
    instance: vulkanalia::Instance,
    debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
    window: Option<Arc<Window>>,
}

impl Engine {
    pub(crate) fn new(create_info: EngineCreateInfo) -> Result<Self> {
        engine_vk::build_engine(create_info)
    }

    pub(crate) fn from_vk_parts(
        entry: vulkanalia::Entry,
        instance: vulkanalia::Instance,
        debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
        window: Option<Arc<Window>>,
    ) -> Self {
        Self {
            entry,
            instance,
            debug_messenger,
            window,
        }
    }

    pub(crate) fn instance(&self) -> &vulkanalia::Instance {
        &self.instance
    }

    pub(crate) fn window(&self) -> Option<&Arc<Window>> {
        self.window.as_ref()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        use vulkanalia::prelude::v1_0::*;

        unsafe {
            if let Some(debug_messenger) = self.debug_messenger.take() {
                self.instance
                    .destroy_debug_utils_messenger_ext(debug_messenger, None);
            }

            self.instance.destroy_instance(None);
        }
    }
}
