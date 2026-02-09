use vulkanalia::prelude::v1_3::*;
use vulkanalia::vk;

#[derive(Clone, Copy, PartialEq)]
pub enum GpuDeviceFeature {
    Vulkan12(GpuDeviceFeatureV12),
    Vulkan13(GpuDeviceFeatureV13),
    Ext(GpuDeviceFeatureExt),
}

#[derive(Clone, Copy, PartialEq)]
pub enum GpuDeviceFeatureV12 {
    BufferDeviceAddress,
    TimelineSemaphore,
    DescriptorBindingPartiallyBound,
    DescriptorBindingVariableDescriptorCount,
    DescriptorIndexing,
    RuntimeDescriptorArray,
}

#[derive(Clone, Copy, PartialEq)]
pub enum GpuDeviceFeatureV13 {
    DynamicRendering,
    Synchronization2,
}

#[derive(Clone, Copy, PartialEq)]
pub enum GpuDeviceFeatureExt {
    MutableDescriptorType,
}

#[derive(Default)]
pub(crate) struct DeviceFeatureArray {
    features: Vec<GpuDeviceFeature>,
    vulkan12: Option<Box<vk::PhysicalDeviceVulkan12Features>>,
    vulkan13: Option<Box<vk::PhysicalDeviceVulkan13Features>>,
    mutable_descriptor_type: Option<Box<vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT>>,
    features2: Option<Box<vk::PhysicalDeviceFeatures2>>,
}

impl From<Vec<GpuDeviceFeature>> for DeviceFeatureArray {
    fn from(features: Vec<GpuDeviceFeature>) -> Self {
        Self {
            features,
            vulkan12: None,
            vulkan13: None,
            mutable_descriptor_type: None,
            features2: None,
        }
    }
}

impl DeviceFeatureArray {
    pub(crate) fn contains(&self, feature: &GpuDeviceFeature) -> bool {
        self.features.contains(feature)
    }

    pub(crate) fn len(&self) -> usize {
        self.features.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    pub(crate) fn push(&mut self, feature: GpuDeviceFeature) {
        self.features.push(feature);
    }

    fn get_vulkan12(features: &Vec<GpuDeviceFeature>) -> vk::PhysicalDeviceVulkan12Features {
        let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::default();
        for feature in features.iter() {
            use GpuDeviceFeature::*;
            match feature {
                Vulkan12(feature) => {
                    *set_v12(&mut vulkan12, *feature) = vk::TRUE;
                }
                _ => {}
            }
        }
        vulkan12
    }

    fn get_vulkan13(features: &Vec<GpuDeviceFeature>) -> vk::PhysicalDeviceVulkan13Features {
        let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::default();
        for feature in features.iter() {
            use GpuDeviceFeature::*;
            match feature {
                Vulkan13(feature) => {
                    *set_v13(&mut vulkan13, *feature) = vk::TRUE;
                }
                _ => {}
            }
        }
        vulkan13
    }

    fn get_mutable_descriptor_type(
        features: &Vec<GpuDeviceFeature>,
    ) -> vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT {
        let mut mutable = vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT::default();
        for feature in features.iter() {
            use GpuDeviceFeature::*;
            match feature {
                Ext(feature) => {
                    *set_ext_mutable_descriptor_type(&mut mutable, *feature) = vk::TRUE;
                }
                _ => {}
            }
        }
        mutable
    }

    pub(crate) fn get_features2(&mut self) -> &mut vk::PhysicalDeviceFeatures2 {
        if self.features2.is_none() {
            let features = &self.features;

            let vulkan12 = self
                .vulkan12
                .get_or_insert_with(|| Box::new(Self::get_vulkan12(features)));

            let vulkan13 = self
                .vulkan13
                .get_or_insert_with(|| Box::new(Self::get_vulkan13(features)));

            let mutable = self.mutable_descriptor_type.get_or_insert_with(|| {
                Box::new(Self::get_mutable_descriptor_type(features))
            });

            let features2 = vk::PhysicalDeviceFeatures2::builder()
                .push_next(vulkan12.as_mut())
                .push_next(vulkan13.as_mut())
                .push_next(mutable.as_mut())
                .build();

            self.features2 = Some(Box::new(features2));
        }
        self.features2.as_mut().unwrap()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &GpuDeviceFeature> {
        self.features.iter()
    }

    pub(crate) fn into_iter(self) -> impl Iterator<Item = GpuDeviceFeature> {
        self.features.into_iter()
    }
}

pub(crate) struct FeatureSupport {
    pub(crate) supported: DeviceFeatureArray,
    pub(crate) missing: DeviceFeatureArray,
}

impl FeatureSupport {
    pub(crate) fn from_device_features(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
        request: DeviceFeatureArray,
    ) -> Self {
        let mut vulkan12 = vk::PhysicalDeviceVulkan12Features::default();
        let mut vulkan13 = vk::PhysicalDeviceVulkan13Features::default();
        let mut mutable = vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT::default();

        let mut features2 = vk::PhysicalDeviceFeatures2::builder()
            .push_next(&mut vulkan12)
            .push_next(&mut vulkan13)
            .push_next(&mut mutable)
            .build();

        unsafe { instance.get_physical_device_features2(physical_device, &mut features2) };

        let mut result = Self {
            supported: DeviceFeatureArray::default(),
            missing: DeviceFeatureArray::default(),
        };

        for feature in request.into_iter() {
            use GpuDeviceFeature::*;
            let supported = match feature {
                Vulkan12(feature) => *set_v12(&mut vulkan12, feature) == vk::TRUE,
                Vulkan13(feature) => *set_v13(&mut vulkan13, feature) == vk::TRUE,
                Ext(feature) => *set_ext_mutable_descriptor_type(&mut mutable, feature) == vk::TRUE,
            };
            if supported {
                result.supported.push(feature);
            } else {
                result.missing.push(feature);
            }
        }

        result
    }
}

fn set_v12(
    vulkan12: &mut vk::PhysicalDeviceVulkan12Features,
    feature: GpuDeviceFeatureV12,
) -> &mut u32 {
    use GpuDeviceFeatureV12::*;
    match feature {
        BufferDeviceAddress => &mut vulkan12.buffer_device_address,
        TimelineSemaphore => &mut vulkan12.timeline_semaphore,
        DescriptorBindingPartiallyBound => &mut vulkan12.descriptor_binding_partially_bound,
        DescriptorBindingVariableDescriptorCount => {
            &mut vulkan12.descriptor_binding_variable_descriptor_count
        }
        DescriptorIndexing => &mut vulkan12.descriptor_indexing,
        RuntimeDescriptorArray => &mut vulkan12.runtime_descriptor_array,
    }
}

fn set_v13(
    vulkan13: &mut vk::PhysicalDeviceVulkan13Features,
    feature: GpuDeviceFeatureV13,
) -> &mut u32 {
    use GpuDeviceFeatureV13::*;
    match feature {
        DynamicRendering => &mut vulkan13.dynamic_rendering,
        Synchronization2 => &mut vulkan13.synchronization2,
    }
}

fn set_ext_mutable_descriptor_type(
    mutable: &mut vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT,
    feature: GpuDeviceFeatureExt,
) -> &mut u32 {
    use GpuDeviceFeatureExt::*;
    match feature {
        MutableDescriptorType => &mut mutable.mutable_descriptor_type,
    }
}
