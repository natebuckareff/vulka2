use std::{ffi::CString, sync::Arc};

use anyhow::{Context, Result};
use vulkanalia::prelude::v1_3::*;

use crate::gpu::{
    ExtensionNameArray, ExtensionSupport, GpuDeviceFeature, GpuDeviceProfile,
    GpuDeviceProfileRejection, GpuDeviceProfileResult,
};

pub enum GpuDeviceRequest<'a> {
    MinimumApiVersion(u32),
    IsDiscrete,
    RequiredExtension(vk::ExtensionName),
    OptionalExtension(vk::ExtensionName),
    RequiredFeature(GpuDeviceFeature),
    OptionalFeature(GpuDeviceFeature),
    HasQueue(GpuQueueProfile<'a>),
}

pub struct GpuQueueProfile<'a> {
    pub priority: f32,
    pub requests: &'a [GpuQueueRequest],
}

pub enum GpuQueueRequest {
    HasGraphics,
    CanPresentTo(vk::SurfaceKHR),
}

pub struct GpuInstanceBuilder<'a> {
    entry: &'a Entry,
    application_name: CString,
    extensions_required: ExtensionNameArray,
    extensions_optional: ExtensionNameArray,
    layers_required: ExtensionNameArray,
    layers_optional: ExtensionNameArray,
}

impl<'a> GpuInstanceBuilder<'a> {
    fn new(entry: &'a Entry) -> Self {
        Self {
            entry,
            application_name: CString::new("").unwrap(),
            extensions_required: ExtensionNameArray::default(),
            extensions_optional: ExtensionNameArray::default(),
            layers_required: ExtensionNameArray::default(),
            layers_optional: ExtensionNameArray::default(),
        }
    }

    pub fn application_name(mut self, application_name: String) -> Result<Self> {
        self.application_name =
            CString::new(application_name.as_str()).context("invalid application name")?;
        Ok(self)
    }

    pub fn require_extension(mut self, extension: vk::Extension) -> Result<Self> {
        if self.extensions_required.contains(&extension.name) {
            return Ok(self);
        }
        self.extensions_required.push(extension.name);
        Ok(self)
    }

    pub fn optional_extension(mut self, extension: vk::Extension) -> Result<Self> {
        if self.extensions_optional.contains(&extension.name) {
            return Ok(self);
        }
        self.extensions_optional.push(extension.name);
        Ok(self)
    }

    pub fn require_layer(mut self, layer: vk::ExtensionName) -> Result<Self> {
        if self.layers_required.contains(&layer) {
            return Ok(self);
        }
        self.layers_required.push(layer);
        Ok(self)
    }

    pub fn optional_layer(mut self, layer: vk::ExtensionName) -> Result<Self> {
        if self.layers_optional.contains(&layer) {
            return Ok(self);
        }
        self.layers_optional.push(layer);
        Ok(self)
    }

    pub fn build(self) -> Result<Arc<GpuInstance>> {
        let mut exts =
            ExtensionSupport::from_instance_extensions(self.entry, self.extensions_required)?;

        let mut layers = ExtensionSupport::from_instance_layers(self.entry, self.layers_required)?;

        exts.validate_required("extensions")?;
        layers.validate_required("layers")?;

        exts.extend(ExtensionSupport::from_instance_extensions(
            self.entry,
            self.extensions_optional,
        )?);

        layers.extend(ExtensionSupport::from_instance_layers(
            self.entry,
            self.layers_optional,
        )?);

        let extensions = ExtensionNameArray::from(exts.supported);
        let layers = ExtensionNameArray::from(layers.supported);

        let instance = GpuInstance::create(self.entry, self.application_name, extensions, layers)?;

        Ok(Arc::new(instance))
    }
}

pub enum GpuFindDeviceProfileResult {
    Fulfilled(GpuDeviceProfile),
    Rejected(Vec<GpuDeviceProfileRejection>),
}

pub struct GpuInstance {
    instance: Instance,
}

impl GpuInstance {
    pub fn build(entry: &'_ Entry) -> GpuInstanceBuilder<'_> {
        GpuInstanceBuilder::new(entry)
    }

    fn create(
        entry: &'_ Entry,
        application_name: CString,
        extensions: ExtensionNameArray,
        layers: ExtensionNameArray,
    ) -> Result<Self> {
        let application_info = vk::ApplicationInfo::builder()
            .api_version(vk::make_version(1, 3, 0))
            .application_name(application_name.as_bytes_with_nul())
            .application_version(0)
            .engine_name(application_name.as_bytes_with_nul())
            .engine_version(0);

        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&application_info)
            .enabled_extension_names(extensions.as_ptrs())
            .enabled_layer_names(layers.as_ptrs())
            .flags(vk::InstanceCreateFlags::empty());

        let instance = unsafe {
            entry
                .create_instance(&create_info, None)
                .context("failed to create instance")
        }?;

        Ok(Self { instance })
    }

    pub(crate) fn get_vk_instance(&self) -> &Instance {
        &self.instance
    }

    pub fn find_device_profile(
        self: &Arc<Self>,
        requests: &[GpuDeviceRequest<'_>],
    ) -> Result<GpuFindDeviceProfileResult> {
        let mut scored = vec![];
        let mut mismatches = vec![];
        let physical_devices = unsafe {
            self.instance
                .enumerate_physical_devices()
                .context("failed to enumerate physical devices")
        }?;
        for physical_device in physical_devices {
            let profile = GpuDeviceProfile::new(&self.instance, physical_device, requests)?;
            match profile {
                GpuDeviceProfileResult::Fulfilled(profile) => {
                    scored.push(profile);
                }
                GpuDeviceProfileResult::Rejected(mismatch) => {
                    mismatches.push(mismatch);
                }
            }
        }
        scored.sort_by_key(|profile| profile.score());
        match scored.pop() {
            Some(profile) => Ok(GpuFindDeviceProfileResult::Fulfilled(profile)),
            None => Ok(GpuFindDeviceProfileResult::Rejected(mismatches)),
        }
    }
}

impl Drop for GpuInstance {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}
