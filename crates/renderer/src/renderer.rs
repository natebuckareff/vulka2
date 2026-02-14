use std::sync::Arc;

use anyhow::Result;
use winit::{dpi::PhysicalSize, window::Window};

pub trait Renderer {
    fn new(window: Arc<Window>) -> Result<Box<Self>>
    where
        Self: Sized;

    fn resized_window(&mut self, size: PhysicalSize<u32>) -> Result<()>;
    fn render_frame(&mut self) -> Result<()>;
}
