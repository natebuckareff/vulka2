use anyhow::Result;
use winit::event_loop::EventLoop;

mod app;
mod gpu;
mod renderer;

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let renderer = renderer::Renderer::new()?;
    let mut app = app::WindowedApp::new(renderer);
    event_loop.run_app(&mut app)?;
    Ok(())
}
