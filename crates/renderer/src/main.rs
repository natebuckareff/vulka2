#![feature(once_cell_try)]

use anyhow::Result;
use winit::event_loop::EventLoop;

use crate::test_renderer_v2::TestRendererV2;

mod app;
mod gpu;
mod gpu_v2;
mod renderer;
mod test_renderer;
mod test_renderer_v2;

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = app::WindowedApp::<TestRendererV2>::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
