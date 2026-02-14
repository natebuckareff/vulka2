use std::sync::Arc;

use anyhow::{Context, Result};
use winit::{dpi::PhysicalSize, window::Window};

use crate::gpu_v2::{
    Device, DeviceProfile, Engine, EngineParams, QueueRoleFlags, ValidationFeatures,
};
use crate::renderer::Renderer;

pub struct TestRendererV2 {
    window: Arc<Window>,
    engine: Arc<Engine>,
    device: Arc<Device>,
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

        let primary_group_id = builder
            .queue_group()
            .graphics()
            .present()
            .compute()
            .transfer()
            .build()
            .context("failed to create primary queue group")?;

        let async_compute_group_id = builder
            .queue_group()
            .compute()
            .transfer()
            .build()
            .context("failed to create async compute queue group")?;

        let async_transfer_group_id = builder
            .queue_group()
            .transfer()
            .build()
            .context("failed to create async transfer queue group")?;

        let device = builder.build()?;

        let primary_group = device
            .take_queue_group(primary_group_id)
            .context("failed to take primary queue group")?;

        let async_compute_group = device
            .take_queue_group(async_compute_group_id)
            .context("failed to take async compute queue group")?;

        let async_transfer_group = device
            .take_queue_group(async_transfer_group_id)
            .context("failed to take async transfer queue group")?;

        dbg!(&primary_group);
        dbg!(&async_compute_group);
        dbg!(&async_transfer_group);

        Ok(Box::new(Self {
            window,
            engine,
            device,
        }))
    }

    fn resized_window(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        Ok(())
    }

    fn render_frame(&mut self) -> Result<()> {
        Ok(())
    }
}
