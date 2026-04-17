mod app;
mod app_shell;

fn main() -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    let mut app = app_shell::AppShell::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
