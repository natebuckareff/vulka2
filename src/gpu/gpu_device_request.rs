use std::collections::HashMap;

use anyhow::{Context, Result};
use vulkanalia::vk;

use crate::gpu::GpuDeviceFeature;

pub enum GpuDeviceRequest {
    MinimumApiVersion(u32),
    IsDiscrete,
    RequiredExtension(vk::ExtensionName),
    OptionalExtension(vk::ExtensionName),
    RequiredFeature(GpuDeviceFeature),
    OptionalFeature(GpuDeviceFeature),
    HasQueue(GpuQueueProfile),
}

pub struct GpuQueueProfile {
    pub priority: f32,
    pub requests: Vec<GpuQueueRequest>,
}

pub enum GpuQueueRequest {
    HasGraphics,
    CanPresentTo(vk::SurfaceKHR),
}

pub struct GpuDeviceRequestBuilder {
    requests: Vec<GpuDeviceRequest>,
    queue_requests: HashMap<String, usize>,
}

impl GpuDeviceRequestBuilder {
    pub fn new() -> Self {
        Self {
            requests: vec![],
            queue_requests: HashMap::new(),
        }
    }

    pub fn minimum_api_version(mut self, api_version: u32) -> Self {
        self.requests
            .push(GpuDeviceRequest::MinimumApiVersion(api_version));
        self
    }

    pub fn is_discrete(mut self) -> Self {
        self.requests.push(GpuDeviceRequest::IsDiscrete);
        self
    }

    pub fn required_extension(mut self, extension: vk::ExtensionName) -> Self {
        self.requests
            .push(GpuDeviceRequest::RequiredExtension(extension));
        self
    }

    pub fn optional_extension(mut self, extension: vk::ExtensionName) -> Self {
        self.requests
            .push(GpuDeviceRequest::OptionalExtension(extension));
        self
    }

    pub fn required_feature(mut self, feature: GpuDeviceFeature) -> Self {
        self.requests
            .push(GpuDeviceRequest::RequiredFeature(feature));
        self
    }

    pub fn optional_feature(mut self, feature: GpuDeviceFeature) -> Self {
        self.requests
            .push(GpuDeviceRequest::OptionalFeature(feature));
        self
    }

    pub fn has_queue(mut self, name: &str, queue_profile: GpuQueueProfile) -> Self {
        let index = self.requests.len();
        self.queue_requests.insert(name.to_string(), index);
        self.requests
            .push(GpuDeviceRequest::HasQueue(queue_profile));
        self
    }

    pub fn queue_request_index(&self, name: &str) -> Result<usize> {
        self.queue_requests
            .get(name)
            .copied()
            .context(format!("unknown queue request: {}", name))
    }

    pub fn requests(&self) -> &[GpuDeviceRequest] {
        &self.requests
    }
}
