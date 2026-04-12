use std::collections::HashSet;
use std::ffi::{CStr, CString, c_void};

use anyhow::{Result, anyhow};
use vulkanalia::Version;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;
use vulkanalia::vk::ExtDebugUtilsExtensionInstanceCommands;
use winit::window::Window;

use crate::gal::{
    engine::{Engine, EngineCreateInfo},
    engine_builder::ValidationFeatures,
};

const MIN_API_VERSION: Version = Version::V1_3_0;
const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);
const VALIDATION_LAYER: vk::ExtensionName =
    vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");

pub(crate) fn build_engine(create_info: EngineCreateInfo) -> Result<Engine> {
    let entry = load_entry()?;
    let vk = build_vk_instance(&entry, &create_info)?;

    Ok(Engine::from_vk_parts(
        entry,
        vk.instance,
        vk.debug_messenger,
        create_info.window,
    ))
}

pub(crate) fn load_entry() -> Result<vulkanalia::Entry> {
    use vulkanalia::prelude::v1_1::*;

    let loader = unsafe { LibloadingLoader::new(LIBRARY)? };
    let entry = unsafe { vulkanalia::Entry::new(loader).map_err(|err| anyhow!("{err}"))? };
    let version = unsafe { entry.enumerate_instance_version()? };

    if !is_version_compatible(version) {
        let major = vk::version_major(version);
        let minor = vk::version_minor(version);
        let patch = vk::version_patch(version);
        return Err(anyhow!(
            "vulkan 1.3 or newer is required, found {major}.{minor}.{patch}"
        ));
    }

    Ok(entry)
}

struct VkInstanceParts {
    instance: vulkanalia::Instance,
    debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
}

fn build_vk_instance(
    entry: &vulkanalia::Entry,
    create_info: &EngineCreateInfo,
) -> Result<VkInstanceParts> {
    let mut validation = ValidationConfig::new(entry);
    let mut required_extensions = required_instance_extensions(create_info.window.as_deref());

    if let Some(features) = create_info.validation_features {
        validation.enable_features(features)?;
        validation.enable_extensions(&[vk::EXT_DEBUG_UTILS_EXTENSION])?;
    }

    let flags = portability_flags(entry, &mut required_extensions)?;
    ensure_instance_extensions_supported(entry, &required_extensions)?;

    let extension_names = collect_extension_names(&required_extensions, &validation)?;
    let application_name = create_application_name(create_info.application_name.as_deref())?;
    let application_info = vk::ApplicationInfo::builder()
        .api_version(vk::make_version(1, 3, 0))
        .application_name(application_name.as_bytes_with_nul())
        .application_version(create_info.application_version.unwrap_or(0))
        .engine_name(b"voxels2\0")
        .engine_version(vk::make_version(0, 0, 1));

    let mut validation_features = validation.validation_features();
    let mut instance_info = vk::InstanceCreateInfo::builder()
        .application_info(&application_info)
        .enabled_extension_names(&extension_names)
        .enabled_layer_names(validation.layer_names())
        .flags(flags)
        .push_next(&mut validation_features);

    let debug_message_types = create_info
        .debug_message_types
        .unwrap_or_else(default_debug_message_types);
    let mut debug_info = debug_messenger_info(debug_message_types);

    if create_info.validation_features.is_some() {
        instance_info = instance_info.push_next(&mut debug_info);
    }

    let instance = unsafe { entry.create_instance(&instance_info, None)? };

    let debug_messenger = if create_info.validation_features.is_some() {
        let debug_info = debug_messenger_info(debug_message_types);
        Some(unsafe { instance.create_debug_utils_messenger_ext(&debug_info, None)? })
    } else {
        None
    };

    Ok(VkInstanceParts {
        instance,
        debug_messenger,
    })
}

fn required_instance_extensions(window: Option<&Window>) -> Vec<vk::ExtensionName> {
    let Some(window) = window else {
        return Vec::new();
    };

    vulkanalia::window::get_required_instance_extensions(window)
        .iter()
        .map(|extension| **extension)
        .collect()
}

fn portability_flags(
    entry: &vulkanalia::Entry,
    required_extensions: &mut Vec<vk::ExtensionName>,
) -> Result<vk::InstanceCreateFlags> {
    use vulkanalia::prelude::v1_1::*;

    if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
        required_extensions.push(vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION.name);
        required_extensions.push(vk::KHR_PORTABILITY_ENUMERATION_EXTENSION.name);
        Ok(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR)
    } else {
        Ok(vk::InstanceCreateFlags::empty())
    }
}

fn ensure_instance_extensions_supported(
    entry: &vulkanalia::Entry,
    required_extensions: &[vk::ExtensionName],
) -> Result<()> {
    let supported_extensions = unsafe { entry.enumerate_instance_extension_properties(None)? }
        .into_iter()
        .map(|extension| extension.extension_name)
        .collect::<HashSet<_>>();

    let missing_extensions = required_extensions
        .iter()
        .copied()
        .filter(|extension| !supported_extensions.contains(extension))
        .collect::<Vec<_>>();

    if missing_extensions.is_empty() {
        return Ok(());
    }

    let missing = missing_extensions
        .into_iter()
        .map(|extension| extension.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    Err(anyhow!("required instance extensions are not supported: {missing}"))
}

fn collect_extension_names(
    required_extensions: &[vk::ExtensionName],
    validation: &ValidationConfig<'_>,
) -> Result<Vec<*const i8>> {
    let mut extension_names = required_extensions
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();
    extension_names.extend(validation.extension_names()?);
    extension_names.sort();
    extension_names.dedup();
    Ok(extension_names)
}

fn create_application_name(application_name: Option<&str>) -> Result<CString> {
    application_name
        .map(CString::new)
        .transpose()?
        .map_or_else(|| Ok(CString::default()), Ok)
}

fn default_debug_message_types() -> vk::DebugUtilsMessageTypeFlagsEXT {
    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
}

fn debug_messenger_info(
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
) -> vk::DebugUtilsMessengerCreateInfoEXTBuilder<'static> {
    vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
        .message_type(message_types)
        .user_callback(Some(debug_callback))
}

fn is_version_compatible(version: u32) -> bool {
    let major = vk::version_major(version);
    let minor = vk::version_minor(version);
    let patch = vk::version_patch(version);

    if MIN_API_VERSION.major != major {
        return MIN_API_VERSION.major <= major;
    }

    if MIN_API_VERSION.minor != minor {
        return MIN_API_VERSION.minor <= minor;
    }

    MIN_API_VERSION.patch <= patch
}

struct ValidationConfig<'a> {
    entry: &'a vulkanalia::Entry,
    layer_names: Vec<*const i8>,
    layer_extensions: Vec<vk::ExtensionName>,
    validation_features: Vec<vk::ValidationFeatureEnableEXT>,
}

impl<'a> ValidationConfig<'a> {
    fn new(entry: &'a vulkanalia::Entry) -> Self {
        Self {
            entry,
            layer_names: Vec::new(),
            layer_extensions: Vec::new(),
            validation_features: Vec::new(),
        }
    }

    fn enable_extensions(&mut self, extensions: &[vk::Extension]) -> Result<()> {
        self.enable_validation_layer()?;

        for extension in extensions {
            if !self.layer_extensions.contains(&extension.name) {
                self.layer_extensions.push(extension.name);
            }
        }

        Ok(())
    }

    fn enable_features(&mut self, features: ValidationFeatures) -> Result<()> {
        self.enable_validation_layer()?;

        if features.debug_printf && features.gpu_assisted {
            return Err(anyhow!(
                "debug printf and gpu assisted validation cannot be enabled together"
            ));
        }

        if features.best_practices {
            self.push_validation_feature(vk::ValidationFeatureEnableEXT::BEST_PRACTICES);
        }

        if features.debug_printf {
            self.push_validation_feature(vk::ValidationFeatureEnableEXT::DEBUG_PRINTF);
        }

        if features.gpu_assisted {
            self.push_validation_feature(vk::ValidationFeatureEnableEXT::GPU_ASSISTED);
            self.push_validation_feature(
                vk::ValidationFeatureEnableEXT::GPU_ASSISTED_RESERVE_BINDING_SLOT,
            );
        }

        if features.synchronization_validation {
            self.push_validation_feature(
                vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION,
            );
        }

        Ok(())
    }

    fn enable_validation_layer(&mut self) -> Result<()> {
        if !self.layer_names.is_empty() {
            return Ok(());
        }

        let available_layers = unsafe { self.entry.enumerate_instance_layer_properties()? }
            .into_iter()
            .map(|layer| layer.layer_name)
            .collect::<HashSet<_>>();

        if !available_layers.contains(&VALIDATION_LAYER) {
            return Err(anyhow!("validation layers are not supported"));
        }

        self.layer_names.push(VALIDATION_LAYER.as_ptr());
        Ok(())
    }

    fn push_validation_feature(&mut self, feature: vk::ValidationFeatureEnableEXT) {
        if !self.validation_features.contains(&feature) {
            self.validation_features.push(feature);
        }
    }

    fn layer_names(&self) -> &[*const i8] {
        &self.layer_names
    }

    fn extension_names(&self) -> Result<Vec<*const i8>> {
        if self.layer_names.is_empty() {
            return Ok(Vec::new());
        }

        let supported_extensions = unsafe {
            self.entry
                .enumerate_instance_extension_properties(Some(VALIDATION_LAYER.as_bytes()))?
        }
        .into_iter()
        .map(|extension| extension.extension_name)
        .collect::<HashSet<_>>();

        let missing_extensions = self
            .layer_extensions
            .iter()
            .copied()
            .filter(|extension| !supported_extensions.contains(extension))
            .collect::<Vec<_>>();

        if !missing_extensions.is_empty() {
            let missing = missing_extensions
                .into_iter()
                .map(|extension| extension.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(anyhow!(
                "required validation layer extensions are not supported: {missing}"
            ));
        }

        Ok(self
            .layer_extensions
            .iter()
            .map(|extension| extension.as_ptr())
            .collect())
    }

    fn validation_features(&self) -> vk::ValidationFeaturesEXTBuilder<'_> {
        if self.layer_names.is_empty() {
            vk::ValidationFeaturesEXT::builder()
        } else {
            vk::ValidationFeaturesEXT::builder()
                .enabled_validation_features(&self.validation_features)
        }
    }
}

extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    let data = unsafe { *data };
    let message = unsafe { CStr::from_ptr(data.message) }.to_string_lossy();

    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        eprintln!("ERROR ({message_type:?})\n  {message}\n");
    } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        eprintln!("WARNING ({message_type:?})\n  {message}\n");
    }

    vk::FALSE
}
