use std::{cell::OnceCell, collections::HashSet};

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu::ValidationFeatures;

const VALIDATION_LAYER: vk::ExtensionName =
    vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");

pub(crate) struct ValidationLayers<'a> {
    entry: &'a vulkanalia::Entry,
    layer_names: Vec<*const i8>,
    extensions: Vec<vk::ExtensionName>,
    extension_names: OnceCell<Vec<*const i8>>,
    features: Vec<vk::ValidationFeatureEnableEXT>,
}

impl<'a> ValidationLayers<'a> {
    pub fn new(entry: &'a vulkanalia::Entry) -> Self {
        Self {
            entry,
            layer_names: vec![],
            extensions: vec![],
            extension_names: OnceCell::new(),
            features: vec![],
        }
    }

    fn enable(&mut self) -> Result<()> {
        use vulkanalia::prelude::v1_0::*;

        if !self.layer_names.is_empty() {
            return Ok(());
        }

        let available_layers = unsafe { self.entry.enumerate_instance_layer_properties()? }
            .iter()
            .map(|layer| layer.layer_name)
            .collect::<HashSet<_>>();

        if !available_layers.contains(&VALIDATION_LAYER) {
            return Err(anyhow!("validation layers not supported"));
        }

        self.layer_names.push(VALIDATION_LAYER.as_ptr());

        // invalidate in case already called
        self.extension_names.take();

        Ok(())
    }

    pub fn enable_extensions(&mut self, extensions: &[vk::Extension]) -> Result<()> {
        self.enable()?;
        for extension in extensions {
            if !self.extensions.contains(&extension.name) {
                self.extensions.push(extension.name);
            }
        }
        Ok(())
    }

    pub fn enable_features(&mut self, validation_features: ValidationFeatures) -> Result<()> {
        self.enable()?;

        if validation_features.best_practices {
            self.enable_feature(vk::ValidationFeatureEnableEXT::BEST_PRACTICES)?;
        }

        if validation_features.debug_printf {
            if validation_features.gpu_assisted {
                return Err(anyhow!(
                    "debug printf and gpu assisted cannot be enabled together"
                ));
            }
            self.enable_feature(vk::ValidationFeatureEnableEXT::DEBUG_PRINTF)?;
        }

        if validation_features.gpu_assisted {
            if validation_features.debug_printf {
                return Err(anyhow!(
                    "debug printf and gpu assisted cannot be enabled together"
                ));
            }
            self.enable_feature(vk::ValidationFeatureEnableEXT::GPU_ASSISTED)?;
            self.enable_feature(vk::ValidationFeatureEnableEXT::GPU_ASSISTED_RESERVE_BINDING_SLOT)?;
        }

        if validation_features.synchronization_validation {
            self.enable_feature(vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION)?;
        }

        Ok(())
    }

    fn enable_feature(&mut self, feature: vk::ValidationFeatureEnableEXT) -> Result<()> {
        if !self.features.contains(&feature) {
            self.features.push(feature);
        }
        Ok(())
    }

    pub fn layer_names(&self) -> &[*const i8] {
        &self.layer_names
    }

    pub fn get_layer_extensions(&self) -> Result<&Vec<*const i8>> {
        use vulkanalia::prelude::v1_0::*;

        if self.layer_names.is_empty() {
            return Ok(self.extension_names.get_or_init(|| vec![]));
        }

        let available_layers = unsafe { self.entry.enumerate_instance_layer_properties()? }
            .iter()
            .map(|layer| layer.layer_name)
            .collect::<HashSet<_>>();

        if !available_layers.contains(&VALIDATION_LAYER) {
            return Err(anyhow!("validation layers not supported"));
        }

        let layer_name = Some(VALIDATION_LAYER.as_bytes());
        let supported_extensions = unsafe {
            self.entry
                .enumerate_instance_extension_properties(layer_name)?
        }
        .into_iter()
        .map(|ext| ext.extension_name)
        .collect::<HashSet<_>>();

        let missing_extensions: Vec<_> = self
            .extensions
            .iter()
            .copied()
            .filter(|ext| !supported_extensions.contains(ext))
            .collect();

        if !missing_extensions.is_empty() {
            for ext in missing_extensions {
                eprintln!("not supported: {}", ext);
            }
            return Err(anyhow!("some required layer extensions are not supported",));
        }

        let ext = self.extension_names.get_or_init(|| {
            self.extensions
                .iter()
                .map(|ext| ext.as_ptr())
                .collect::<Vec<_>>()
        });

        Ok(ext)
    }

    pub fn get_validation_features(&'_ self) -> vk::ValidationFeaturesEXTBuilder<'_> {
        use vulkanalia::prelude::v1_0::*;

        if self.layer_names.is_empty() {
            return vk::ValidationFeaturesEXT::builder();
        } else {
            vk::ValidationFeaturesEXT::builder().enabled_validation_features(&self.features)
        }
    }
}
