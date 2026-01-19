#![feature(once_cell_try)]

use anyhow::Result;
use winit::event_loop::EventLoop;

mod app;
mod device_address;
mod gpu;
mod test_renderer;

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = app::WindowedApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
