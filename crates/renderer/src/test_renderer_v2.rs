use std::sync::Arc;

use anyhow::{Context, Result};
use winit::{dpi::PhysicalSize, window::Window};

use crate::gpu_v2::{
    DeviceBuilder, DeviceKind, DeviceProfile, Engine, EngineParams, QueueRoleFlags,
    ValidationFeatures,
};
use crate::renderer::Renderer;

pub struct TestRendererV2 {
    window: Arc<Window>,
    engine: Arc<Engine>,
}

impl Renderer for TestRendererV2 {
    fn new(window: Arc<Window>) -> Result<Box<Self>>
    where
        Self: Sized,
    {
        let params = EngineParams {
            application_name: Some("voxels2".to_string()),
            application_version: None,
            enable_validation_layers: Some(ValidationFeatures {
                best_practices: true,
                debug_printf: false,
                gpu_assisted: false,
                synchronization_validation: true,
            }),
            debug_message_types: None,
            window: Some(window.clone()),
        };
        let engine = Engine::new(params)?;

        let profile = DeviceProfile {
            kind: None,
            roles: QueueRoleFlags::GRAPHICS
                | QueueRoleFlags::PRESENT
                | QueueRoleFlags::COMPUTE
                | QueueRoleFlags::TRANSFER,
        };
        let info = engine.get_best_device(profile)?.unwrap();
        let mut builder = engine.device(info);

        let primary_group = builder
            .queue_group()
            .graphics()
            .present()
            .compute()
            .transfer()
            .build()
            .context("failed to create primary queue group")?;

        let async_compute_group = builder.queue_group().compute().transfer().build().unwrap();
        let async_transfer_group = builder.queue_group().transfer().build().unwrap();

        let device = builder.build()?;

        // println!("info: {:#?}", info);

        // let mut device_builder = DeviceBuilder::new();

        // for info in device_infos {
        //     for family in info.families {
        //         // family.roles.
        //         // device_builder.queue(family.id, family.count);
        //     }
        //     // println!("device: {:#?}", info);
        // }

        Ok(Box::new(Self { window, engine }))
    }

    fn resized_window(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        Ok(())
    }

    fn render_frame(&mut self) -> Result<()> {
        Ok(())
    }
}
