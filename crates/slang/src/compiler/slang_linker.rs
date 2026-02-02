use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use anyhow::{Result, bail};
use blake3::Hasher;
use shader_slang as slang;

use crate::{
    LayoutBuilder, ModuleId, SlangCompiler, SlangEntrypoint, SlangModule, SlangProgram,
    SlangProgramKey, SlangShaderStage, SpirvCode, SpirvCodeKey, walk_program,
};

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

    pub fn link(self) -> Result<Arc<SlangProgram>> {
        if self.entrypoints.is_empty() {
            bail!("no entrypoints to link");
        }

        let program_key = self.compute_program_key();
        // TODO: check cache

        let entrypoints: Vec<_> = self.entrypoints.iter().cloned().collect();
        let mut components: Vec<slang::ComponentType> = Vec::new();

        for module_id in &self.modules {
            let module = self.compiler.module(module_id).unwrap();
            components.push(module.slang_module().clone().into());
        }

        for ep in &entrypoints {
            let module = self.compiler.module(ep.module_id()).unwrap();
            let slang_ep = module.slang_entrypoint(ep)?;
            components.push(slang_ep.clone().into());
        }

        let composite = self
            .compiler
            .session()
            .create_composite_component_type(&components)
            .map_err(|e| anyhow::anyhow!("failed to create composite: {}", e))?;

        let linked = composite
            .link()
            .map_err(|e| anyhow::anyhow!("failed to link: {}", e))?;

        let mut code: HashMap<SlangEntrypoint, SpirvCode> = HashMap::new();

        for (i, ep) in entrypoints.iter().enumerate() {
            let spirv_blob = linked
                .entry_point_code(i as i64, 0)
                .map_err(|e| anyhow::anyhow!("failed to get code for '{}': {}", ep.name(), e))?;

            let spirv_bytes = spirv_blob.as_slice();
            let code_key = SpirvCodeKey::new(program_key, ep);
            let spirv_code = SpirvCode::new(code_key, spirv_bytes);

            code.insert(ep.clone(), spirv_code);
        }

        let mut builder = LayoutBuilder::new();
        let program_layout = linked.layout(0)?;
        let shader_layout = builder.build_shader(program_layout)?;
        let layout = serde_json::to_string_pretty(&shader_layout)?;

        Ok(SlangProgram::new(program_key, layout, entrypoints, code))
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
