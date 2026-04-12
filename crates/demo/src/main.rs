use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use renderer::gal::Device;
use renderer::gal::DeviceBuilder;
use renderer::gal::Engine;
use renderer::gal::EngineBuilder;
use renderer::gal::Queue;
use renderer::gal::QueueKind;
use renderer::gal::QueueRequest;
use renderer::gal::Surface;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::ElementState;
use winit::event::StartCause;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::ControlFlow;
use winit::event_loop::EventLoop;
use winit::keyboard::Key;
use winit::keyboard::NamedKey;
use winit::window::Window;
use winit::window::WindowId;

struct DemoApp {
    window: Option<Arc<Window>>,
    engine: Option<Arc<Engine>>,
    surface: Option<Arc<Surface>>,
    device: Option<Device>,
    queues: Vec<Queue>,
    next_frame: Instant,
    frame_dt: Duration,
}

impl DemoApp {
    fn new() -> Self {
        Self {
            window: None,
            engine: None,
            surface: None,
            device: None,
            queues: Vec::new(),
            next_frame: Instant::now(),
            frame_dt: Duration::from_micros(16_667),
        }
    }

    fn exit(&mut self, event_loop: &ActiveEventLoop) {
        self.queues.clear();
        self.device.take();
        self.surface.take();
        self.engine.take();
        self.window.take();
        event_loop.exit();
    }

    fn create_window(event_loop: &ActiveEventLoop) -> Result<Arc<Window>> {
        let attributes = Window::default_attributes()
            .with_title("VOXELS2 DEMO WINDOW")
            .with_inner_size(PhysicalSize::new(800, 600))
            .with_visible(true);

        let window = event_loop.create_window(attributes)?;
        window.set_visible(true);

        Ok(Arc::new(window))
    }

    fn create_engine(window: Arc<Window>) -> Result<Arc<Engine>> {
        let engine = EngineBuilder::new()
            .application_name("demo".to_string())
            .application_version(1)
            .enable_best_practices()
            .enable_sync_validation()
            .window(window)
            .build()?;

        Ok(Arc::new(engine))
    }

    fn create_surface(engine: Arc<Engine>) -> Result<Arc<Surface>> {
        Ok(Arc::new(Surface::new(engine)?))
    }

    fn create_device(engine: Arc<Engine>, surface: Arc<Surface>) -> Result<(Vec<Queue>, Device)> {
        let mut graphics = QueueRequest::new(QueueKind::Graphics);
        DeviceBuilder::new(engine)
            .allocate_queue(&mut graphics)?
            .present(surface)
            .build()
    }
}

impl ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(Self::create_window(event_loop).expect("failed to create window"));

        let window = self.window.as_ref().expect("window not created").clone();
        let engine = Self::create_engine(window.clone()).expect("failed to create engine");
        let surface = Self::create_surface(engine.clone()).expect("failed to create surface");
        let (queues, device) =
            Self::create_device(engine.clone(), surface.clone()).expect("failed to create device");

        self.engine = Some(engine);
        self.surface = Some(surface);
        self.device = Some(device);
        self.queues = queues;
        self.next_frame = Instant::now();

        window.request_redraw();
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
            WindowEvent::CloseRequested => self.exit(event_loop),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let Some(window) = &self.window {
                        window.request_redraw();
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
                self.next_frame += self.frame_dt;

                let now = Instant::now();
                while self.next_frame <= now {
                    self.next_frame += self.frame_dt;
                }

                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = DemoApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
