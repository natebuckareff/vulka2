use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowId};

use crate::renderer::Renderer;

pub struct WindowedApp {
    window: Option<Window>,
    next_frame: Instant,
    frame_dt: Duration,
    renderer: Renderer,
}

impl WindowedApp {
    pub fn new(renderer: Renderer) -> Self {
        Self {
            window: None,
            next_frame: Instant::now(),
            frame_dt: Duration::from_micros(16_667), // 60 FPS
            renderer,
        }
    }
}

impl ApplicationHandler for WindowedApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title("voxels2")
            .with_inner_size(PhysicalSize::new(1024, 768))
            .with_visible(true);

        let window = event_loop
            .create_window(attributes)
            .expect("failed to create window");

        self.window = Some(window);
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
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.renderer
                    .render_frame()
                    .expect("failed to render frame");

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
