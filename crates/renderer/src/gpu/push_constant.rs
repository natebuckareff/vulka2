use anyhow::{Context, Result, anyhow, ensure};
use slang::{LayoutCursor, LayoutKind};
use vulkanalia::vk;

use crate::gpu::PushConstantData;

#[derive(Clone, Debug)]
pub struct PushConstant {
    layouts: Box<[LayoutCursor]>,
    stage_flags: vk::ShaderStageFlags,
}

impl PushConstant {
    pub fn new(layout: LayoutCursor) -> Result<Self> {
        let stage_flags = layout.push_constant_layout()?.stages;

        // TODO: For global push constants, these stage flags should eventually
        // come from the selected pipeline entrypoints rather than reflection's
        // coarse default. That context likely belongs in pipeline assembly.
        Self::with_stages(layout, stage_flags)
    }

    pub fn with_stages(layout: LayoutCursor, stage_flags: vk::ShaderStageFlags) -> Result<Self> {
        validate_push_constant_layout(&layout)?;

        Ok(Self {
            layouts: vec![layout].into_boxed_slice(),
            stage_flags,
        })
    }

    pub fn merge(layouts: &[LayoutCursor]) -> Result<Self> {
        let (first, rest) = layouts
            .split_first()
            .context("cannot merge an empty push constant layout list")?;

        validate_push_constant_layout(first)?;

        let first_range = first.push_constant_layout()?;

        let mut stage_flags = first_range.stages;
        let mut merged_layouts = Vec::with_capacity(layouts.len());
        merged_layouts.push(first.clone());

        for layout in rest {
            validate_push_constant_layout(layout)?;

            let range = layout.push_constant_layout()?;

            ensure!(
                range.offset == first_range.offset,
                "push constant offsets differ: {} vs {}",
                first_range.offset,
                range.offset
            );

            ensure!(
                range.size == first_range.size,
                "push constant sizes differ: {} vs {}",
                first_range.size,
                range.size
            );

            stage_flags |= range.stages;
            merged_layouts.push(layout.clone());
        }

        Ok(Self {
            layouts: merged_layouts.into_boxed_slice(),
            stage_flags,
        })
    }

    pub fn layouts(&self) -> &[LayoutCursor] {
        &self.layouts
    }

    pub fn stage_flags(&self) -> vk::ShaderStageFlags {
        self.stage_flags
    }

    pub(crate) fn vk_range(&self) -> Result<vk::PushConstantRange> {
        let layout = self
            .layouts
            .first()
            .context("push constant has no source layouts")?;
        let range = layout.push_constant_layout()?;
        Ok(vk::PushConstantRange {
            stage_flags: self.stage_flags,
            offset: range.offset,
            size: range.size,
        })
    }

    pub(crate) fn matches_range(&self, range: vk::PushConstantRange) -> Result<bool> {
        Ok(self.vk_range()? == range)
    }

    pub fn data(&self) -> Result<PushConstantData> {
        let layout = self
            .layouts()
            .first()
            .context("push constant has no source layouts")?
            .element_layout()?
            .rebase();
        let range = self.vk_range()?;
        PushConstantData::new(layout, range)
    }
}

fn validate_push_constant_layout(layout: &LayoutCursor) -> Result<()> {
    if layout.kind() != LayoutKind::PushConstantBuffer {
        return Err(anyhow!("layout cursor is not a push constant buffer"));
    }

    let payload = layout
        .element_layout()
        .context("push constant buffer is missing its payload layout")?;

    if payload.kind() != LayoutKind::Struct {
        return Err(anyhow!(
            "push constant payload is expected to be a struct, found {:?}",
            payload.kind()
        ));
    }

    Ok(())
}
