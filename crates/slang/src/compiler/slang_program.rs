use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, bail};
use blake3::{Hash, Hasher};

use crate::{LayoutIr, SlangEntrypoint, SlangLayoutDirect, SlangShaderStage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlangProgramKey(pub Hash);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpirvCodeKey(pub Hash);

impl SpirvCodeKey {
    pub fn new(program_key: SlangProgramKey, entrypoint: &SlangEntrypoint) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(program_key.0.as_bytes());
        hasher.update(&[entrypoint.stage() as u8]);
        hasher.update(entrypoint.name().as_bytes());
        hasher.update(&[0]);
        hasher.update(entrypoint.module_id().as_str().as_bytes());
        SpirvCodeKey(hasher.finalize())
    }
}

/// Owned blob of SPIR-V bytecode; 4-byte aligned.
#[derive(Clone)]
pub struct SpirvCode {
    key: SpirvCodeKey,
    data: Vec<u32>,
}

impl SpirvCode {
    pub(crate) fn new(key: SpirvCodeKey, bytes: &[u8]) -> Self {
        assert!(bytes.len() % 4 == 0, "SPIR-V must be 4-byte aligned");
        let words: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        Self { key, data: words }
    }

    pub fn key(&self) -> SpirvCodeKey {
        self.key
    }

    pub fn len_words(&self) -> usize {
        self.data.len()
    }

    pub fn len_bytes(&self) -> usize {
        self.data.len() * 4
    }
}

impl AsRef<[u32]> for SpirvCode {
    fn as_ref(&self) -> &[u32] {
        &self.data
    }
}

/// A compiled shader program containing SPIR-V code for all linked entrypoints.
pub struct SlangProgram {
    key: SlangProgramKey,
    layout: SlangLayoutDirect,
    entrypoints: Vec<SlangEntrypoint>,
    code: HashMap<SlangEntrypoint, SpirvCode>,
}

impl SlangProgram {
    pub(crate) fn new(
        key: SlangProgramKey,
        layout: SlangLayoutDirect,
        entrypoints: Vec<SlangEntrypoint>,
        code: HashMap<SlangEntrypoint, SpirvCode>,
    ) -> Arc<Self> {
        Arc::new(Self {
            key,
            layout,
            entrypoints,
            code,
        })
    }

    pub fn key(&self) -> SlangProgramKey {
        self.key
    }

    pub fn layout(&self) -> &SlangLayoutDirect {
        &self.layout
    }

    pub fn entrypoints(&self) -> &[SlangEntrypoint] {
        &self.entrypoints
    }

    pub fn code(&self, entrypoint: &SlangEntrypoint) -> Option<&SpirvCode> {
        self.code.get(entrypoint)
    }

    /// Select all entrypoints, ensuring at most one per stage.
    pub fn select_all(self: &Arc<Self>) -> Result<SlangPipelineProgram> {
        let mut selection = SlangPipelineSelection::default();
        for ep in &self.entrypoints {
            selection.set(ep.clone())?;
        }
        if selection.is_empty() {
            bail!("no entrypoints found");
        }
        self.select_each(selection)
    }

    /// Select only graphics (vertex/fragment) entrypoints.
    pub fn select_graphics(self: &Arc<Self>) -> Result<SlangPipelineProgram> {
        let mut selection = SlangPipelineSelection::default();
        for ep in &self.entrypoints {
            match ep.stage() {
                SlangShaderStage::Vertex | SlangShaderStage::Fragment => {
                    selection.set(ep.clone())?;
                }
                SlangShaderStage::Compute => {}
            }
        }
        if selection.is_empty() {
            bail!("no graphics entrypoints found");
        }
        self.select_each(selection)
    }

    /// Select only the compute entrypoint.
    pub fn select_compute(self: &Arc<Self>) -> Result<SlangPipelineProgram> {
        let mut selection = SlangPipelineSelection::default();
        for ep in &self.entrypoints {
            if ep.stage() == SlangShaderStage::Compute {
                selection.set(ep.clone())?;
            }
        }
        if selection.is_empty() {
            bail!("no compute entrypoints found");
        }
        self.select_each(selection)
    }

    /// Select a single specific entrypoint.
    pub fn select_one(
        self: &Arc<Self>,
        entrypoint: SlangEntrypoint,
    ) -> Result<SlangPipelineProgram> {
        if !self.entrypoints.contains(&entrypoint) {
            bail!("entrypoint '{}' not found in program", entrypoint.name());
        }
        let mut selection = SlangPipelineSelection::default();
        selection.set(entrypoint)?;
        self.select_each(selection)
    }

    /// Select entrypoints according to the given selection.
    pub fn select_each(
        self: &Arc<Self>,
        selection: SlangPipelineSelection,
    ) -> Result<SlangPipelineProgram> {
        for ep in selection.iter() {
            if !self.entrypoints.contains(ep) {
                bail!("entrypoint '{}' not found in program", ep.name());
            }
        }

        Ok(SlangPipelineProgram {
            program: Arc::clone(self),
            selection,
        })
    }
}

/// Selection of at most one entrypoint per shader stage.
#[derive(Clone, Default)]
pub struct SlangPipelineSelection {
    pub vertex: Option<SlangEntrypoint>,
    pub fragment: Option<SlangEntrypoint>,
    pub compute: Option<SlangEntrypoint>,
}

impl SlangPipelineSelection {
    pub fn set(&mut self, ep: SlangEntrypoint) -> Result<()> {
        let slot = match ep.stage() {
            SlangShaderStage::Vertex => &mut self.vertex,
            SlangShaderStage::Fragment => &mut self.fragment,
            SlangShaderStage::Compute => &mut self.compute,
        };
        if slot.is_some() {
            bail!(
                "multiple {:?} entrypoints: already have one, cannot add '{}'",
                ep.stage(),
                ep.name()
            );
        }
        *slot = Some(ep);
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.vertex.is_none() && self.fragment.is_none() && self.compute.is_none()
    }

    pub fn iter(&self) -> impl Iterator<Item = &SlangEntrypoint> {
        [&self.vertex, &self.fragment, &self.compute]
            .into_iter()
            .flatten()
    }
}

/// A program view with at most one entrypoint per stage, suitable for
/// Vulkan pipeline creation.
pub struct SlangPipelineProgram {
    program: Arc<SlangProgram>,
    selection: SlangPipelineSelection,
}

impl SlangPipelineProgram {
    pub fn key(&self) -> SlangProgramKey {
        self.program.key()
    }

    pub fn layout(&self) -> &SlangLayoutDirect {
        self.program.layout()
    }

    pub fn entrypoint(&self, stage: SlangShaderStage) -> Option<&SlangEntrypoint> {
        match stage {
            SlangShaderStage::Vertex => self.selection.vertex.as_ref(),
            SlangShaderStage::Fragment => self.selection.fragment.as_ref(),
            SlangShaderStage::Compute => self.selection.compute.as_ref(),
        }
    }

    pub fn entrypoints(&self) -> impl Iterator<Item = &SlangEntrypoint> {
        self.selection.iter()
    }

    pub fn code(&self, stage: SlangShaderStage) -> Option<&SpirvCode> {
        self.entrypoint(stage).and_then(|ep| self.program.code(ep))
    }
}
