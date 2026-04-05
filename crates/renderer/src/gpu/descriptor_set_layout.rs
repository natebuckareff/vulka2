// TODO: llm generated file; rewrite

use std::{cmp::Ordering, sync::Arc};

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{Device, OwnedDescriptorSetLayout, VulkanResource};

pub struct DescriptorSetLayout {
    layout: slang::LayoutCursor,
    handle: OwnedDescriptorSetLayout,
}

impl DescriptorSetLayout {
    pub fn new(device: Arc<Device>, layout: slang::LayoutCursor) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let parameter_block_layout = layout.parameter_block_layout()?;
        let bindings = Self::bindings(parameter_block_layout)?;

        let info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        let device = device.handle().clone();
        let handle = OwnedDescriptorSetLayout::new(device, &info)?;

        Ok(Self { layout, handle })
    }

    // TODO: feel like this should be a trait, then we can derive PartialEq etc
    pub(crate) fn owned(&self) -> &OwnedDescriptorSetLayout {
        &self.handle
    }

    pub fn layout(&self) -> &slang::LayoutCursor {
        &self.layout
    }

    fn bindings(
        layout: &slang::ParameterBlockLayout,
    ) -> Result<Vec<vk::DescriptorSetLayoutBinding>> {
        let mut bindings = Vec::new();

        if let Some(binding) = &layout.implicit_ubo {
            bindings.push(Self::binding(binding)?);
        }

        for range in &layout.binding_ranges {
            bindings.push(Self::binding(&range.descriptor)?);
        }

        bindings.sort_by(|a, b| {
            if a.binding == b.binding {
                Ordering::Equal
            } else if a.binding < b.binding {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });

        Ok(bindings)
    }

    fn binding(binding: &slang::DescriptorBindingLayout) -> Result<vk::DescriptorSetLayoutBinding> {
        use vulkanalia::prelude::v1_0::*;

        let descriptor_count = match binding.count {
            slang::ElementCount::Bounded(count) => count as u32,
            slang::ElementCount::Runtime => {
                return Err(anyhow!(
                    "runtime-sized descriptor bindings are not supported for transient descriptor sets"
                ));
            }
        };

        if binding.binding < 0 {
            return Err(anyhow!("descriptor binding index must be non-negative"));
        }

        Ok(vk::DescriptorSetLayoutBinding::builder()
            .binding(binding.binding as u32)
            .descriptor_type(binding.descriptor_type)
            .descriptor_count(descriptor_count)
            .stage_flags(binding.stages)
            .build())
    }
}

impl PartialEq for DescriptorSetLayout {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.owned().raw() == other.owned().raw() }
    }
}
