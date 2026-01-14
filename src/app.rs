use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::test_renderer::Renderer;

pub struct WindowedApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    next_frame: Instant,
    frame_dt: Duration,
}

impl WindowedApp {
    pub fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            next_frame: Instant::now(),
            frame_dt: Duration::from_micros(16_667), // 60 FPS
        }
    }

    pub fn create_window(event_loop: &ActiveEventLoop) -> Result<Arc<Window>> {
        let attributes = Window::default_attributes()
            .with_title("floating: voxels2")
            .with_inner_size(PhysicalSize::new(1920, 1080))
            .with_visible(true);

        let window = event_loop.create_window(attributes)?;
        Ok(Arc::new(window))
    }

    pub fn create_renderer(window: Arc<Window>) -> Result<Renderer> {
        let renderer = Renderer::new(window.clone())?;
        Ok(renderer)
    }
}

impl ApplicationHandler for WindowedApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(Self::create_window(event_loop).expect("failed to create window"));

        let window = self.window.as_ref().unwrap();

        self.renderer =
            Some(Self::create_renderer(window.clone()).expect("failed to create renderer"));

        self.next_frame = Instant::now();
    }

    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                println!("close requested; stopping event loop");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let Some(renderer) = &mut self.renderer {
                        renderer
                            .resized_window(size)
                            .expect("failed to resize window");
                    }

                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.render_frame().expect("failed to render frame");
                }

                // schedule next frame
                self.next_frame += self.frame_dt;

                // if the frame we just rendered took longer than `frame_dt`,
                // then advance `next_frame` until the next scheduled frame is in
                // the future
                let now = Instant::now();
                while self.next_frame <= now {
                    self.next_frame += self.frame_dt;
                }

                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
            }
            _ => (),
        }
    }
}
