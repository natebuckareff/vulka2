// TODO: llm generated file; rewrite

use std::{cmp::Ordering, collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result, anyhow};
use vulkanalia::vk::{self, HasBuilder};

use crate::gpu::{Device, OwnedDescriptorSetLayout};

pub struct DescriptorSetLayout {
    layout: slang::LayoutCursor,
    handle: OwnedDescriptorSetLayout,
    sizing: DescriptorPoolSizing,
}

impl DescriptorSetLayout {
    pub fn new(device: Arc<Device>, layout: slang::LayoutCursor, max_sets: u32) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let parameter_block_layout = layout.parameter_block_layout()?;
        let bindings = Self::bindings(parameter_block_layout)?;
        let sizing = DescriptorPoolSizing::new(parameter_block_layout, max_sets)?;

        let info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        let device = device.handle().clone();
        let handle = OwnedDescriptorSetLayout::new(device, &info)?;

        Ok(Self {
            layout,
            handle,
            sizing,
        })
    }

    pub(crate) fn owned(&self) -> &OwnedDescriptorSetLayout {
        &self.handle
    }

    pub(crate) fn sizing(&self) -> &DescriptorPoolSizing {
        &self.sizing
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

pub(crate) struct DescriptorPoolSizing {
    max_sets: u32,
    sizes: Vec<vk::DescriptorPoolSize>,
}

impl DescriptorPoolSizing {
    fn new(layout: &slang::ParameterBlockLayout, max_sets: u32) -> Result<Self> {
        let mut counts = BTreeMap::new();

        if let Some(binding) = &layout.implicit_ubo {
            Self::accumulate(&mut counts, binding, max_sets)?;
        }

        for range in &layout.binding_ranges {
            Self::accumulate(&mut counts, &range.descriptor, max_sets)?;
        }

        let sizes = counts
            .into_iter()
            .map(|(ty, descriptor_count)| {
                vk::DescriptorPoolSize::builder()
                    .type_(ty)
                    .descriptor_count(descriptor_count)
                    .build()
            })
            .collect();

        Ok(Self { max_sets, sizes })
    }

    pub(crate) fn max_sets(&self) -> u32 {
        self.max_sets
    }

    pub(crate) fn sizes(&self) -> &[vk::DescriptorPoolSize] {
        &self.sizes
    }

    fn accumulate(
        counts: &mut BTreeMap<vk::DescriptorType, u32>,
        binding: &slang::DescriptorBindingLayout,
        max_sets: u32,
    ) -> Result<()> {
        let per_set = match binding.count {
            slang::ElementCount::Bounded(count) => count as u32,
            slang::ElementCount::Runtime => {
                return Err(anyhow!(
                    "runtime-sized descriptor bindings are not supported for transient descriptor sets"
                ));
            }
        };

        let total = per_set
            .checked_mul(max_sets)
            .context("descriptor pool size overflow")?;

        counts
            .entry(binding.descriptor_type)
            .and_modify(|count| *count += total)
            .or_insert(total);

        Ok(())
    }
}
