use std::collections::BTreeSet;

use anyhow::{Result, bail};
use blake3::{Hash, Hasher};

use crate::{ModuleId, SlangCompiler, SlangEntrypoint, SlangModule, SlangShaderStage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlangProgramKey(pub Hash);

pub struct SlangLinker<'a> {
    compiler: &'a SlangCompiler,
    modules: BTreeSet<ModuleId>,
    entrypoints: BTreeSet<SlangEntrypoint>,
}

impl<'a> SlangLinker<'a> {
    pub(crate) fn new(compiler: &'a SlangCompiler) -> Self {
        Self {
            compiler,
            modules: BTreeSet::new(),
            entrypoints: BTreeSet::new(),
        }
    }

    fn require_module(&self, id: &ModuleId) -> Result<&SlangModule> {
        self.compiler
            .module(id)
            .ok_or_else(|| anyhow::anyhow!("module '{}' not loaded in compiler", id))
    }

    pub fn add_module(mut self, id: &ModuleId) -> Result<Self> {
        self.require_module(id)?;
        self.modules.insert(id.clone());
        Ok(self)
    }

    pub fn add_stage(mut self, id: &ModuleId, stage: SlangShaderStage) -> Result<Self> {
        let module = self.require_module(id)?;
        let stage_eps: Vec<_> = module
            .entrypoints()
            .iter()
            .filter(|ep| ep.stage() == stage)
            .cloned()
            .collect();

        self.modules.insert(id.clone());
        for ep in stage_eps {
            self.entrypoints.insert(ep);
        }
        Ok(self)
    }

    pub fn add_all_entrypoints(mut self, id: &ModuleId) -> Result<Self> {
        let module = self.require_module(id)?;
        let all_eps: Vec<_> = module.entrypoints().to_vec();

        self.modules.insert(id.clone());
        for ep in all_eps {
            self.entrypoints.insert(ep);
        }
        Ok(self)
    }

    pub fn add_entrypoint(mut self, entrypoint: SlangEntrypoint) -> Result<Self> {
        self.require_module(entrypoint.module_id())?;
        self.modules.insert(entrypoint.module_id().clone());
        self.entrypoints.insert(entrypoint);
        Ok(self)
    }

    pub fn link(self) -> Result<SlangProgram> {
        if self.entrypoints.is_empty() {
            bail!("no entrypoints to link");
        }

        let program_key = self.compute_program_key();

        // TODO: Check cache for existing program
        // TODO: Perform actual linking via shader_slang

        Ok(SlangProgram {
            key: program_key,
            entrypoints: self.entrypoints.into_iter().collect(),
        })
    }

    fn compute_program_key(&self) -> SlangProgramKey {
        let mut hasher = Hasher::new();

        hasher.update(self.compiler.options_hash().0.as_bytes());
        hasher.update(&(self.modules.len() as u32).to_le_bytes());

        for module_id in &self.modules {
            hasher.update(module_id.as_str().as_bytes());
            hasher.update(&[0]);
            if let Some(module) = self.compiler.module(module_id) {
                hasher.update(module.content_hash().as_bytes());
            }
        }

        hasher.update(&(self.entrypoints.len() as u32).to_le_bytes());
        for ep in &self.entrypoints {
            hasher.update(&[ep.stage() as u8]);
            hasher.update(ep.name().as_bytes());
            hasher.update(&[0]);
            hasher.update(ep.module_id().as_str().as_bytes());
            hasher.update(&[0]);
        }

        SlangProgramKey(hasher.finalize())
    }
}

pub struct SlangProgram {
    key: SlangProgramKey,
    entrypoints: Vec<SlangEntrypoint>,
}

impl SlangProgram {
    pub fn key(&self) -> SlangProgramKey {
        self.key
    }

    pub fn entrypoints(&self) -> &[SlangEntrypoint] {
        &self.entrypoints
    }
}
