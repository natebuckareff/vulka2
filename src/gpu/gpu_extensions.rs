use std::collections::HashSet;
use std::ffi::CStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;

#[derive(Default)]
pub struct GpuExtensionsBuilder {
    names: Vec<vk::ExtensionName>,
    seen: HashSet<vk::ExtensionName>,
}

impl GpuExtensionsBuilder {
    fn new() -> Self {
        Self::default()
    }

    pub fn add(mut self, name: vk::ExtensionName) -> Self {
        if self.seen.insert(name) {
            self.names.push(name);
        }
        self
    }

    pub fn build(self) -> Arc<GpuExtensions> {
        Arc::new(GpuExtensions { names: self.names })
    }
}

pub struct GpuExtensions {
    names: Vec<vk::ExtensionName>,
}

impl GpuExtensions {
    pub fn builder() -> GpuExtensionsBuilder {
        GpuExtensionsBuilder::new()
    }

    pub fn empty() -> Arc<Self> {
        GpuExtensionsBuilder::new().build()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &CStr> {
        self.names.iter().map(|name| name.as_cstr())
    }

    pub fn handles(self: &Arc<Self>) -> impl Iterator<Item = GpuExtensionHandle> {
        (0..self.names.len()).map(|index| GpuExtensionHandle {
            extensions: self.clone(),
            index,
        })
    }

    pub(crate) fn iter_names(&self) -> impl Iterator<Item = vk::ExtensionName> + '_ {
        self.names.iter().copied()
    }

    pub fn handle(self: &Arc<Self>, name: vk::ExtensionName) -> Option<GpuExtensionHandle> {
        self.names
            .iter()
            .position(|candidate| *candidate == name)
            .map(|index| GpuExtensionHandle {
                extensions: self.clone(),
                index,
            })
    }

    pub(crate) fn support_for(
        self: &Arc<Self>,
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Result<GpuExtensionSupport> {
        if self.names.is_empty() {
            return Ok(GpuExtensionSupport::new(self.clone(), Vec::new()));
        }

        let properties = unsafe {
            instance
                .enumerate_device_extension_properties(physical_device, None)
                .context("failed to enumerate device extension properties")?
        };
        let supported: HashSet<vk::ExtensionName> =
            properties.iter().map(|prop| prop.extension_name).collect();

        let mut support = Vec::with_capacity(self.names.len());
        for name in self.names.iter().copied() {
            support.push(supported.contains(&name));
        }

        Ok(GpuExtensionSupport::new(self.clone(), support))
    }

    pub(crate) fn with_ptrs<T>(&self, f: impl FnOnce(&[*const i8]) -> T) -> T {
        let ptrs = self
            .names
            .iter()
            .map(|name| name.as_ptr())
            .collect::<Vec<_>>();
        f(&ptrs)
    }
}

#[derive(Clone)]
pub struct GpuExtensionHandle {
    extensions: Arc<GpuExtensions>,
    index: usize,
}

impl GpuExtensionHandle {
    pub fn name(&self) -> &CStr {
        self.extensions.names[self.index].as_cstr()
    }

    pub fn extensions(&self) -> &Arc<GpuExtensions> {
        &self.extensions
    }
}

pub struct GpuExtensionSupport {
    extensions: Arc<GpuExtensions>,
    supported: Vec<bool>,
}

impl GpuExtensionSupport {
    fn new(extensions: Arc<GpuExtensions>, supported: Vec<bool>) -> Self {
        Self {
            extensions,
            supported,
        }
    }

    pub fn is_supported(&self, handle: &GpuExtensionHandle) -> bool {
        if !Arc::ptr_eq(&self.extensions, &handle.extensions) {
            return false;
        }
        self.supported.get(handle.index).copied().unwrap_or(false)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&CStr, bool)> {
        self.extensions
            .names
            .iter()
            .zip(self.supported.iter())
            .map(|(name, supported)| (name.as_cstr(), *supported))
    }

    pub fn missing_extension_names(&self) -> Vec<String> {
        self.extensions
            .names
            .iter()
            .zip(self.supported.iter())
            .filter_map(|(name, supported)| {
                if *supported {
                    None
                } else {
                    Some(name.as_cstr().to_string_lossy().to_string())
                }
            })
            .collect()
    }
}
