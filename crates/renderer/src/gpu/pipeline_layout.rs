use std::{cell::OnceCell, sync::Arc};

use anyhow::{Result, anyhow};
use vulkanalia::vk;

use crate::gpu::{
    DescriptorSetLayout, Device, OwnedDescriptorSetLayout, OwnedPipelineLayout, PushConstant,
    PushConstantData, VulkanResource,
};

pub struct PipelineLayout {
    sets: Box<[Option<Arc<DescriptorSetLayout>>]>,
    empty_set: OnceCell<OwnedDescriptorSetLayout>,
    constants: Box<[Arc<PushConstant>]>,
    handle: OwnedPipelineLayout,
}

impl PipelineLayout {
    pub fn new(
        device: Arc<Device>,
        parameter_blocks: &[slang::LayoutCursor],
        constants: Box<[Arc<PushConstant>]>,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let sets = Self::derive_sets(&device, parameter_blocks)?;
        let empty_set = OnceCell::new();
        let set_layouts = get_vk_set_layouts(&device, &sets, &empty_set)?;

        let push_constant_ranges = constants
            .iter()
            .map(|pc| pc.vk_range())
            .collect::<Result<Vec<_>>>()?;

        let info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_constant_ranges);

        let handle = OwnedPipelineLayout::new(device.handle().clone(), &info)?;

        Ok(Self {
            sets,
            empty_set,
            constants,
            handle,
        })
    }

    fn derive_sets(
        device: &Arc<Device>,
        parameter_blocks: &[slang::LayoutCursor],
    ) -> Result<Box<[Option<Arc<DescriptorSetLayout>>]>> {
        let mut highest_set: Option<usize> = None;
        let mut indexed_sets = Vec::with_capacity(parameter_blocks.len());

        for layout in parameter_blocks {
            let set_index = get_set_index(layout)?;
            let set_layout = Arc::new(DescriptorSetLayout::new(device.clone(), layout.clone())?);

            highest_set = Some(highest_set.map_or(set_index, |highest| highest.max(set_index)));
            indexed_sets.push((set_index, set_layout));
        }

        let Some(highest_set) = highest_set else {
            return Ok(Box::new([]));
        };

        let mut sets = vec![None; highest_set + 1];
        for (set_index, set_layout) in indexed_sets {
            if sets[set_index].is_some() {
                return Err(anyhow!(
                    "duplicate descriptor set layout for set {}",
                    set_index
                ));
            }
            sets[set_index] = Some(set_layout);
        }

        Ok(sets.into_boxed_slice())
    }

    pub fn set(&self, index: usize) -> Result<&DescriptorSetLayout> {
        let Some(set) = self.sets.get(index) else {
            return Err(anyhow!("descriptor set slot {} is out-of-bounds", index));
        };
        let Some(set) = set.as_ref() else {
            return Err(anyhow!("descriptor set slot {} is empty", index));
        };
        Ok(set)
    }

    pub fn constants(&self) -> &[Arc<PushConstant>] {
        &self.constants
    }

    pub(crate) fn validate_push_constant_data(&self, data: &PushConstantData) -> Result<()> {
        let range = data.range();
        let is_valid = self
            .constants
            .iter()
            .map(|constant| constant.matches_range(range))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .any(|matches| matches);

        if !is_valid {
            return Err(anyhow!(
                "push constant range is not declared on this pipeline layout"
            ));
        }

        Ok(())
    }

    pub(crate) fn owned(&self) -> &OwnedPipelineLayout {
        &self.handle
    }
}

impl PartialEq for PipelineLayout {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.owned().raw() == other.owned().raw() }
    }
}

fn get_set_index(layout: &slang::LayoutCursor) -> Result<usize> {
    let parameter_block_layout = layout.parameter_block_layout()?;
    let Some(set_index) = parameter_block_layout.set else {
        return Err(anyhow!("parameter block does not contain a descriptor set"));
    };
    if set_index < 0 {
        return Err(anyhow!("descriptor set index must be non-negative"));
    }
    Ok(set_index as usize) // TODO: should be u32
}

fn get_vk_set_layouts(
    device: &Arc<Device>,
    sets: &[Option<Arc<DescriptorSetLayout>>],
    empty_set: &OnceCell<OwnedDescriptorSetLayout>,
) -> Result<Vec<vk::DescriptorSetLayout>> {
    let mut set_layouts = Vec::with_capacity(sets.len());

    for set in sets {
        let set_layout = match set {
            Some(set) => unsafe { *set.owned().raw() },
            None => unsafe { *create_empty_set_layout(device, empty_set)?.raw() },
        };
        set_layouts.push(set_layout);
    }

    Ok(set_layouts)
}

fn create_empty_set_layout<'a>(
    device: &Arc<Device>,
    empty_set: &'a OnceCell<OwnedDescriptorSetLayout>,
) -> Result<&'a OwnedDescriptorSetLayout> {
    use vulkanalia::prelude::v1_0::*;
    empty_set.get_or_try_init(|| {
        let bindings: [vk::DescriptorSetLayoutBinding; 0] = [];
        let info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        OwnedDescriptorSetLayout::new(device.handle().clone(), &info)
    })
}
