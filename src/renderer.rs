use std::sync::Arc;

use anyhow::Result;
use vulkano::{
    VulkanLibrary,
    instance::{Instance, InstanceCreateInfo, InstanceExtensions},
};

pub struct Renderer {
    instance: Arc<Instance>,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        let library = VulkanLibrary::new()?;

        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                application_name: Some("voxels2".to_string()),
                max_api_version: Some(vulkano::Version::V1_3),
                enabled_extensions: InstanceExtensions::empty(),
                ..Default::default()
            },
        )?;
        Ok(Self { instance })
    }

    pub fn render_frame(&self) -> Result<()> {
        // TODO
        Ok(())
    }
}
