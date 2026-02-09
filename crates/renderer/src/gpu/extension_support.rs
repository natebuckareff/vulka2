use std::collections::HashSet;

use anyhow::{Context, Result, anyhow};
use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;

use crate::gpu::ExtensionNameArray;

pub struct ExtensionSupport {
    pub supported: Vec<vk::ExtensionName>,
    pub missing: Vec<vk::ExtensionName>,
}

impl ExtensionSupport {
    pub fn extend(&mut self, other: Self) {
        for extension in other.supported {
            if !self.supported.contains(&extension) {
                self.supported.push(extension);
            }
        }
        for extension in other.missing {
            if !self.missing.contains(&extension) {
                self.missing.push(extension);
            }
        }
    }

    pub(crate) fn from_instance_extensions(
        entry: &'_ Entry,
        request: ExtensionNameArray,
    ) -> Result<Self> {
        let extension_properties = unsafe {
            entry
                .enumerate_instance_extension_properties(None)
                .context("failed to enumerate instance extension properties")
        }?;
        Ok(Self::from_extension_properties(&extension_properties, request))
    }

    pub(crate) fn from_instance_extensions_with_layers(
        entry: &'_ Entry,
        request: ExtensionNameArray,
        layers: &[vk::ExtensionName],
    ) -> Result<Self> {
        let mut supported_extensions = HashSet::new();

        let global_properties = unsafe {
            entry
                .enumerate_instance_extension_properties(None)
                .context("failed to enumerate instance extension properties")
        }?;
        for property in global_properties {
            supported_extensions.insert(property.extension_name);
        }

        for layer in layers {
            let layer_name = layer.as_cstr().to_bytes_with_nul();
            let layer_properties = unsafe {
                entry
                    .enumerate_instance_extension_properties(Some(layer_name))
                    .with_context(|| {
                        format!(
                            "failed to enumerate instance extension properties for layer `{}`",
                            layer
                        )
                    })
            }?;
            for property in layer_properties {
                supported_extensions.insert(property.extension_name);
            }
        }

        Ok(Self::from_supported_extensions(
            &supported_extensions,
            request,
        ))
    }

    pub(crate) fn from_device_extensions(
        instance: &'_ Instance,
        physical_device: vk::PhysicalDevice,
        request: ExtensionNameArray,
    ) -> Result<Self> {
        let extension_properties = unsafe {
            instance
                .enumerate_device_extension_properties(physical_device, None)
                .context("failed to enumerate device extension properties")
        }?;
        Ok(Self::from_extension_properties(&extension_properties, request))
    }

    pub(crate) fn from_instance_layers(
        entry: &'_ Entry,
        request: ExtensionNameArray,
    ) -> Result<Self> {
        let layer_properties = unsafe {
            entry
                .enumerate_instance_layer_properties()
                .context("failed to enumerate instance layer properties")
        }?;

        let mut supported_layers = HashSet::new();
        for property in layer_properties {
            supported_layers.insert(property.layer_name);
        }

        let mut result = Self {
            supported: vec![],
            missing: vec![],
        };

        for layer in request.into_iter() {
            if supported_layers.contains(&layer) {
                result.supported.push(layer);
            } else {
                result.missing.push(layer);
            }
        }

        Ok(result)
    }

    fn from_extension_properties(
        extension_properties: &[vk::ExtensionProperties],
        request: ExtensionNameArray,
    ) -> Self {
        let mut supported_extensions = HashSet::new();
        for property in extension_properties {
            supported_extensions.insert(property.extension_name);
        }

        Self::from_supported_extensions(&supported_extensions, request)
    }

    fn from_supported_extensions(
        supported_extensions: &HashSet<vk::ExtensionName>,
        request: ExtensionNameArray,
    ) -> Self {
        let mut result = Self {
            supported: vec![],
            missing: vec![],
        };

        for extension in request.into_iter() {
            if supported_extensions.contains(&extension) {
                result.supported.push(extension);
            } else {
                result.missing.push(extension);
            }
        }

        result
    }

    pub fn validate_required(&self, what: &str) -> Result<()> {
        if !self.missing.is_empty() {
            if cfg!(debug_assertions) {
                for extension in &self.missing {
                    eprintln!("DEBUG: required {} not supported: {}", what, extension);
                }
            }
            return Err(anyhow!("some required {} are not supported", what));
        }
        Ok(())
    }
}
