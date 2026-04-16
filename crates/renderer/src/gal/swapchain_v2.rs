use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::Result;
use anyhow::bail;
use vulkanalia::vk;

use crate::gal::Device;
use crate::gal::Image;
use crate::gal::ImageToken;
use crate::gal::SampleCount;
use crate::gal::Surface;
use crate::gal::device::DeviceResource;
use crate::gal::image_view::ImageView;
use crate::gal::lazy_fence::LazyFence;
use crate::gal::semaphore_resource::SemaphoreResource;
use crate::gal::swapchain_resource::SwapchainResource;

pub struct Swapchain {
    device: Arc<Device>,
    surface: Arc<Surface>,
    state: Option<Box<SwapchainState>>,
    should_recreate: bool,
}

impl Swapchain {
    pub fn new(device: Arc<Device>, surface: Arc<Surface>, extent: vk::Extent2D) -> Result<Self> {
        let mut swapchain = Self {
            device,
            surface,
            state: None,
            should_recreate: false,
        };
        swapchain.recreate(extent)?;
        Ok(swapchain)
    }

    pub fn recreate(&mut self, extent: vk::Extent2D) -> Result<()> {
        let old = self.state.take();
        let device = self.device.clone();
        let mut new = SwapchainState::new(device, &self.surface, extent, old.as_deref())?;
        new.set_old(old);
        self.state = Some(Box::new(new));
        Ok(())
    }

    pub fn acquire(&mut self) -> Result<SwapchainToken, AcquireError> {
        match self.state_mut().acquire() {
            Ok(token) => {
                if self.state().is_suboptimal {
                    self.should_recreate = true;
                }
                Ok(token)
            }
            Err(e @ (AcquireError::RecreateSwapchain | AcquireError::RecreateSurface)) => {
                if matches!(e, AcquireError::RecreateSurface) {
                    // TODO: how to recreate the surface?
                    todo!()
                }
                match self.recreate(self.state().swapchain.extent()) {
                    Ok(_) => Err(e),
                    Err(error) => Err(AcquireError::RecreateError {
                        cause: Box::new(e),
                        error,
                    }),
                }
            }
            Err(AcquireError::RegainFullScreen) => {
                todo!("not implemented")
            }
            Err(e) => Err(e),
        }
    }

    pub fn retire(&mut self, token: SwapchainToken) -> Result<()> {
        self.state_mut().retire(token)?;
        if self.should_recreate {
            self.recreate(self.state().swapchain.extent())?;
            self.should_recreate = false;
        }
        Ok(())
    }

    fn state(&self) -> &SwapchainState {
        // SAFETY: new() immediately calls recreate() which exits with
        // self.state set
        self.state.as_ref().unwrap()
    }

    fn state_mut(&mut self) -> &mut SwapchainState {
        // SAFETY: new() immediately calls recreate() which exits with
        // self.state set
        self.state.as_mut().unwrap()
    }
}

struct SwapchainState {
    device: Arc<Device>,
    generation: u64,
    swapchain: Arc<SwapchainResource>,
    tokens: Vec<Option<SwapchainToken>>,
    images: Vec<SwapchainImage>,
    history: SwapchainHistory,
    token_index: usize,
    first_present: Option<u32>,
    is_suboptimal: bool,
    old: Option<Box<SwapchainState>>,
}

impl SwapchainState {
    fn new(
        device: Arc<Device>,
        surface: &Surface,
        extent: vk::Extent2D,
        old: Option<&SwapchainState>,
    ) -> Result<Self> {
        let generation = old.as_ref().map(|state| state.generation + 1).unwrap_or(0);
        let old_swapchain = old.as_ref().map(|state| state.swapchain.as_ref());
        let swapchain = create_swapchain(&device, surface, extent, old_swapchain)?;
        let images = create_images(&device, &swapchain)?;
        let image_count = images.len();
        let mut tokens = Vec::with_capacity(image_count);
        for token_index in 0..image_count {
            let device = device.resource().clone();
            let token = SwapchainToken::new(device, generation, token_index)?;
            tokens.push(Some(token))
        }
        Ok(Self {
            device,
            generation,
            swapchain,
            tokens,
            images,
            history: SwapchainHistory::new(image_count)?,
            token_index: 0,
            first_present: None,
            is_suboptimal: false,
            old: None,
        })
    }

    fn set_old(&mut self, old: Option<Box<SwapchainState>>) {
        self.old = old;
    }

    fn acquire(&mut self) -> Result<SwapchainToken, AcquireError> {
        use vulkanalia::prelude::v1_0::*;
        use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;

        // the last acquire token has not been retired yet
        let Some(token) = &self.tokens[self.token_index] else {
            return Err(AcquireError::NotReady);
        };

        assert!(
            matches!(
                token.state.status,
                TokenStatus::Initial | TokenStatus::Retired
            ),
            "swapchain token not retired"
        );

        // Get a fence from the PresentHistory fence pool, to track present
        // completion for the next acquired swapchain image. The image index is
        // not know yet, but the fence must be associated with the acquire image
        // index afterwards
        let mut fence = self.history.get_fence();

        let result = unsafe {
            let device = self.device.resource();
            let swapchain = self.swapchain.handle();
            let semaphore = token.state.image_available.handle();
            let fence = fence
                .get_or_init_unsignalled(device)
                .map_err(AcquireError::Other)?;

            device
                .handle()
                .acquire_next_image_khr(swapchain, u64::MAX, semaphore, fence)
        };

        if let Ok((image_index, _)) = result {
            if self.old.is_some() {
                // If the acquired image is the same as the first image ever
                // presented by this swapchain, and that image index has an
                // acquire fence associated with it that has been signalled,
                // then the first present on this swapchain completed
                //
                // This means that any old swapchains are no longer in use by
                // the presentation engine and may now be cleaned up

                if Some(image_index) == self.first_present {
                    let finished = self
                        .history
                        .is_finished_presenting(image_index)
                        .map_err(AcquireError::Other)?;

                    if finished {
                        self.cleanup_old_swapchains();
                    }
                }
            }

            // After successfully acquiring the next image index, associate the
            // fence with that index to track completion
            self.history
                .set_image_fence(image_index, fence)
                .map_err(AcquireError::Other)?;
        } else {
            // If there was an error acquiring the next swapchain image index,
            // return the fence to the fence pool for reuse. The spec says in
            // this case the fence will not have been signalled and is not in
            // use
            self.history.return_fence(fence);
        }

        match result {
            Ok((image_index, code)) => {
                if code == vk::SuccessCode::SUCCESS || code == vk::SuccessCode::SUBOPTIMAL_KHR {
                    if code == vk::SuccessCode::SUBOPTIMAL_KHR {
                        // schedule the swapchain to be recreated
                        self.is_suboptimal = true;
                    }

                    // get the current token and the acquired image, transition
                    // the token from Retired to Acquired status, increment
                    // token_index, and return the token

                    let image = &mut self.images[image_index as usize];

                    // SAFETY: already checked at beginning that
                    // self.tokens[self.token_index] is Some
                    let mut token = self.tokens[self.token_index].take().unwrap();

                    // transition the Retired token to Acquired, taking the
                    // render target ImageToken out of `image` and storing it in
                    // `token`
                    if let Err(e) = token.acquire(image) {
                        // shouldn't really happen
                        return Err(AcquireError::Other(e));
                    };

                    self.token_index = (self.token_index + 1) % self.tokens.len();

                    Ok(token)
                } else if code == vk::SuccessCode::TIMEOUT {
                    Err(AcquireError::Timeout)
                } else if code == vk::SuccessCode::NOT_READY {
                    Err(AcquireError::NotReady)
                } else {
                    Err(AcquireError::UnknownSuccessCode(code))
                }
            }
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => Err(AcquireError::RecreateSwapchain),
            Err(vk::ErrorCode::SURFACE_LOST_KHR) => Err(AcquireError::RecreateSurface),
            Err(vk::ErrorCode::FULL_SCREEN_EXCLUSIVE_MODE_LOST_EXT) => {
                Err(AcquireError::RegainFullScreen)
            }
            Err(code) => Err(AcquireError::Code(code)),
        }
    }

    fn retire(&mut self, mut token: SwapchainToken) -> Result<()> {
        let TokenStatus::Presenting((image_index, _)) = token.state.status else {
            bail!("swapchain token retired before present");
        };

        let image = &mut self.images[image_index as usize];
        token.retire(image, self.generation)?;

        let token_index = token.state.token_index;
        self.tokens[token_index] = Some(token);

        if self.first_present.is_none() {
            // if this is the first present for this swapchain, record the image
            // index to later attempt cleanup
            self.first_present = Some(image_index)
        }

        Ok(())
    }

    fn cleanup_old_swapchains(&mut self) {
        while let Some(old) = self.old.take() {
            self.old = (*old).old;
        }
    }
}

pub struct SwapchainToken {
    state: Box<TokenState>,
}

impl SwapchainToken {
    fn new(device: Arc<DeviceResource>, generation: u64, token_index: usize) -> Result<Self> {
        let state = Box::new(TokenState::new(device, generation, token_index)?);
        Ok(Self { state })
    }

    fn acquire(&mut self, image: &mut SwapchainImage) -> Result<()> {
        let state = self.state.as_mut();
        if !matches!(state.status, TokenStatus::Initial | TokenStatus::Retired) {
            bail!("swapchain token invalid status for acquiring");
        }
        let Some(target) = image.target.take() else {
            bail!("swapchain image invalid state")
        };
        state.status = TokenStatus::Acquired(target);
        Ok(())
    }

    fn retire(&mut self, image: &mut SwapchainImage, generation: u64) -> Result<()> {
        let state = self.state.as_mut();
        if generation != state.generation {
            bail!("swapchain token generation mismatch");
        }
        if image.target.is_some() {
            bail!("swapchain image invalid state");
        }
        let status = std::mem::replace(&mut state.status, TokenStatus::Retired);
        let TokenStatus::Presenting(target) = status else {
            state.status = status;
            bail!("swapchain token retired before present");
        };
        image.target = Some(target);
        Ok(())
    }
}

struct TokenState {
    generation: u64,
    token_index: usize,
    status: TokenStatus,
    image_available: SemaphoreResource,
    render_finished: SemaphoreResource,
}

impl TokenState {
    fn new(device: Arc<DeviceResource>, generation: u64, token_index: usize) -> Result<Self> {
        let image_available = SemaphoreResource::binary(device.clone())?;
        let render_finished = SemaphoreResource::binary(device)?;
        Ok(Self {
            generation,
            token_index,
            status: TokenStatus::Initial,
            image_available,
            render_finished,
        })
    }
}

// Initial|Retired -> Acquired      Swapchain::acquire()
// Acquired        -> Bound         CommandBuffer::render()
// Bound           -> Recorded      Rendering::present()
// Recorded        -> Rendering     Queue::submit()
// Rendering       -> Presenting    Queue::present()
// Presenting      -> Retired       Swapchain::retire()

enum TokenStatus {
    Initial,
    Acquired((u32, ImageToken)),
    Bound((u32, ImageToken)),
    Recorded((u32, ImageToken)),
    Rendering((u32, ImageToken)),
    Presenting((u32, ImageToken)),
    Retired,
}

// TODO: this feels a bit hacky, but maybe worth it
impl PartialEq for TokenStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Initial, Self::Initial) => true,
            (Self::Acquired(..), Self::Acquired(..)) => true,
            (Self::Bound(..), Self::Bound(..)) => true,
            (Self::Recorded(..), Self::Recorded(..)) => true,
            (Self::Rendering(..), Self::Rendering(..)) => true,
            (Self::Presenting(..), Self::Presenting(..)) => true,
            (Self::Retired, Self::Retired) => true,
            _ => false,
        }
    }
}

struct SwapchainImage {
    target: Option<(u32, ImageToken)>,
    view: ImageView,
}

struct SwapchainHistory {
    image_fences: Vec<LazyFence>,
    pool: VecDeque<LazyFence>,
}

impl SwapchainHistory {
    fn new(image_count: usize) -> Result<Self> {
        let mut images = Vec::with_capacity(image_count);
        images.resize_with(image_count, Default::default);
        Ok(Self {
            image_fences: images,
            pool: VecDeque::new(),
        })
    }

    // called immediately before acquire_next_image()
    fn get_fence(&mut self) -> LazyFence {
        self.pool.pop_front().unwrap_or_else(LazyFence::default)
    }

    // if there's an error on acquire, return fence to pool
    fn return_fence(&mut self, fence: LazyFence) {
        self.pool.push_back(fence)
    }

    // called immediately after acquire_next_image()
    fn set_image_fence(&mut self, image_index: u32, fence: LazyFence) -> Result<()> {
        let i = image_index as usize;
        let mut fence = std::mem::replace(&mut self.image_fences[i], fence);
        if !fence.is_empty() && !fence.is_signalled()? {
            bail!("reacquired image fence unexpectedly unsignalled")
        }
        fence.reset()?;
        self.pool.push_back(fence);
        Ok(())
    }

    // called to check if present is finished for some image
    fn is_finished_presenting(&mut self, image_index: u32) -> Result<bool> {
        self.image_fences[image_index as usize].is_signalled()
    }
}

pub enum AcquireError {
    Timeout,
    NotReady,
    RecreateSwapchain,
    RecreateSurface,
    RegainFullScreen,
    UnknownSuccessCode(vk::SuccessCode),
    Code(vk::ErrorCode),
    Other(anyhow::Error),
    RecreateError {
        cause: Box<AcquireError>,
        error: anyhow::Error,
    },
}

fn create_swapchain(
    device: &Device,
    surface: &Surface,
    extent: vk::Extent2D,
    old: Option<&SwapchainResource>,
) -> Result<Arc<SwapchainResource>> {
    use vulkanalia::prelude::v1_0::*;

    let surface_info = surface.info(device)?;
    let surface_format = surface_info.get_best_surface_format();
    let format = surface_format.format;
    let extent = surface_info.get_clamped_extent(extent);

    let mut create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(unsafe { surface.handle() })
        .min_image_count(surface_info.get_min_image_count())
        .image_format(format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(surface_info.pre_transform())
        .composite_alpha(surface_info.composite_alpha()?)
        .present_mode(surface_info.get_present_mode()?)
        .clipped(true);

    if let Some(old) = old {
        create_info = create_info.old_swapchain(unsafe { old.handle() });
    }

    let device = device.resource().clone();
    let swapchain = SwapchainResource::new(device, format, extent, &create_info)?;

    Ok(Arc::new(swapchain))
}

fn create_images(
    device: &Arc<Device>,
    swapchain: &Arc<SwapchainResource>,
) -> Result<Vec<SwapchainImage>> {
    use vulkanalia::prelude::v1_0::*;
    use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;

    let components = vk::ComponentMapping::builder()
        .r(vk::ComponentSwizzle::IDENTITY)
        .g(vk::ComponentSwizzle::IDENTITY)
        .b(vk::ComponentSwizzle::IDENTITY)
        .a(vk::ComponentSwizzle::IDENTITY)
        .build();

    let range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };

    let sample_count = SampleCount::new(1)?;

    let image_extent = vk::Extent3D {
        width: swapchain.extent().width,
        height: swapchain.extent().height,
        depth: 1,
    };

    let swapchain_images = unsafe {
        device
            .resource()
            .handle()
            .get_swapchain_images_khr(swapchain.handle())?
    };

    let mut images = Vec::with_capacity(swapchain_images.len());

    for (i, handle) in swapchain_images.into_iter().enumerate() {
        let image = Arc::new(Image::from_swapchain(
            device.clone(),
            swapchain.clone(),
            handle,
            vk::ImageType::_2D,
            swapchain.format(),
            image_extent,
            1,
            1,
            sample_count,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST,
        ));
        let mut span = image.span(range)?;
        let token = span.acquire()?;
        let view_type = vk::ImageViewType::_2D;
        let format = swapchain.format();
        let view = span.view(view_type, format, components)?;
        let swapchain_image = SwapchainImage {
            target: token.map(|token| (i as u32, token)),
            view,
        };
        images.push(swapchain_image);
    }

    Ok(images)
}
