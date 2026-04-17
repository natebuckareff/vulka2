use anyhow::Context;
use winit::application::ApplicationHandler;
use winit::event::ElementState;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::Key;
use winit::keyboard::NamedKey;
use winit::window::WindowId;

use crate::app::App;

pub struct AppShell {
    app: Option<App>,
}

impl AppShell {
    pub fn new() -> Self {
        Self { app: None }
    }

    fn exit(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(app) = self.app.as_ref() {
            // TODO XXX: hack until we have proper token drain mechanism
            let _ = app.device.wait_idle();
        }
        let _ = self.app.take();
        event_loop.exit();
    }
}

impl ApplicationHandler for AppShell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let app = App::new(event_loop)
            .context("failed to create app")
            .unwrap();
        app.window.request_redraw();
        self.app = Some(app);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.exit(event_loop),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let Some(app) = &self.app {
                        app.window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                        self.exit(event_loop);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(app) = self.app.as_mut() {
                    if let Err(error) = app.render() {
                        panic!("render failed: {error:#}");
                    }
                    app.print_fps();
                    app.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}
