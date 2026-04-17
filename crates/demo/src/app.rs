use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use renderer::gal::AcquireError;
use renderer::gal::ColorTarget;
use renderer::gal::CommandAllocator;
use renderer::gal::Device;
use renderer::gal::DeviceBuilder;
use renderer::gal::Engine;
use renderer::gal::EngineBuilder;
use renderer::gal::Queue;
use renderer::gal::QueueKind;
use renderer::gal::QueueRequest;
use renderer::gal::RenderTargets;
use renderer::gal::RenderingLayout;
use renderer::gal::SampleCount;
use renderer::gal::Submission;
use renderer::gal::Surface;
use renderer::gal::Swapchain;
use renderer::gal::vk;
use winit::dpi::PhysicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::ControlFlow;
use winit::window::Window;

pub struct App {
    pub window: Arc<Window>,
    pub device: Arc<Device>,
    pub swapchain: Swapchain,
    pub command_allocator: CommandAllocator,
    pub queues: Vec<Queue>,
    pub frame: u32,
    pub fps_report_started_at: Instant,
    pub fps_report_started_frame: u32,
}

impl App {
    pub fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        let window = Self::create_window(event_loop)?;
        let engine = Self::create_engine(window.clone())?;
        let surface = Self::create_surface(engine.clone())?;
        let (queues, device) = Self::create_device(engine.clone(), &surface)?;
        let queue = queues.first().context("queue not created")?;
        let size = window.inner_size();
        let swapchain = Swapchain::new(device.clone(), surface, Self::extent_2d(size))?;
        let command_allocator = CommandAllocator::new(device.clone(), queue.family(), 3);
        let frame = 0;
        let fps_report_started_at = Instant::now();
        let fps_report_started_frame = 0;
        Ok(Self {
            window,
            device,
            swapchain,
            command_allocator,
            queues,
            frame,
            fps_report_started_at,
            fps_report_started_frame,
        })
    }

    fn create_window(event_loop: &ActiveEventLoop) -> Result<Arc<Window>> {
        let attributes = Window::default_attributes()
            .with_title("floating: vulka demo")
            .with_inner_size(PhysicalSize::new(1440, 1080))
            .with_visible(true);
        let window = event_loop.create_window(attributes)?;
        window.set_visible(true);
        event_loop.set_control_flow(ControlFlow::Poll);
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

    fn create_surface(engine: Arc<Engine>) -> Result<Surface> {
        Ok(Surface::new(engine)?)
    }

    fn create_device(engine: Arc<Engine>, surface: &Surface) -> Result<(Vec<Queue>, Arc<Device>)> {
        let mut graphics = QueueRequest::new(QueueKind::Graphics);
        DeviceBuilder::new(engine)
            .allocate_queue(&mut graphics)?
            .present(&surface)
            .build()
    }

    fn extent_2d(size: PhysicalSize<u32>) -> vk::Extent2D {
        vk::Extent2D {
            width: size.width,
            height: size.height,
        }
    }

    pub fn print_fps(&mut self) {
        let elapsed = self.fps_report_started_at.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }

        let frames = self.frame.saturating_sub(self.fps_report_started_frame);
        let fps = frames as f64 / elapsed.as_secs_f64();
        println!("FPS: {fps:.1}");

        self.fps_report_started_at = Instant::now();
        self.fps_report_started_frame = self.frame;
    }

    pub fn render(&mut self) -> Result<()> {
        let allocator = &mut self.command_allocator;

        // TODO: need better way to grab different queues keyed by cap
        let queue = self.queues.first_mut().expect("queue not created");

        if self.swapchain.will_recreate() {
            let size = self.window.inner_size();
            if size.width == 0 || size.height == 0 {
                return Ok(());
            }
            self.swapchain.recreate(Self::extent_2d(size))?;
        }

        let mut token = match self.swapchain.acquire() {
            Ok(token) => token,
            Err(AcquireError::NotReady | AcquireError::Timeout) => return Ok(()),
            Err(AcquireError::RecreateSwapchain) => {
                let size = self.window.inner_size();
                if size.width == 0 || size.height == 0 {
                    return Ok(());
                }
                self.swapchain.recreate(Self::extent_2d(size))?;
                return Ok(());
            }
            Err(AcquireError::RecreateSurface) => {
                anyhow::bail!("surface recreation not implemented")
            }
            Err(AcquireError::RegainFullScreen) => {
                anyhow::bail!("fullscreen recovery not implemented")
            }
            Err(AcquireError::UnknownSuccessCode(code)) => {
                anyhow::bail!("unexpected acquire success code: {:?}", code)
            }
            Err(AcquireError::Code(code)) => anyhow::bail!("swapchain acquire failed: {:?}", code),
            Err(AcquireError::Other(error)) => return Err(error),
            Err(AcquireError::RecreateError { cause, error }) => {
                anyhow::bail!(
                    "swapchain recreate failed after {:?}: {error}",
                    std::mem::discriminant(&*cause)
                )
            }
        };

        let mut pool = match allocator.acquire(self.frame)? {
            Some(pool) => pool,
            None => return Ok(()),
        };

        let clear = vk::ClearColorValue {
            float32: [0.08, 0.12, 0.18, 1.0],
        };

        let area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: self.swapchain.extent(),
        };

        let layout = Arc::new(RenderingLayout {
            color_formats: Box::new([self.swapchain.format()]),
            depth_format: None,
            stencil_format: None,
            samples: SampleCount::new(1)?,
        });

        let color = ColorTarget::new(
            self.swapchain.color_view(&token)?,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::AttachmentLoadOp::CLEAR,
            vk::AttachmentStoreOp::STORE,
            clear,
        );

        let targets = RenderTargets::new(layout, area, vec![color].into_boxed_slice(), None, None)?;

        let mut cmdbuf = pool.allocate(queue.lane())?;

        let rendering = {
            let mut bound = targets.bind().swapchain_color(0, &mut token)?.build()?;
            cmdbuf.graphics().render(&mut bound)?
        };
        rendering.present(&mut token)?;

        let mut submission = Submission::new(self.frame, queue.lane());
        submission.push(cmdbuf)?;
        queue.submit(&mut self.swapchain, submission, &mut token)?;

        self.window.pre_present_notify();

        allocator.retire(pool)?;

        queue
            .present(&mut self.swapchain, &mut token)
            .map_err(|error| {
                anyhow::anyhow!("present failed: {:?}", std::mem::discriminant(&error))
            })?;

        self.swapchain.retire(token)?;
        self.frame += 1;

        Ok(())
    }
}
