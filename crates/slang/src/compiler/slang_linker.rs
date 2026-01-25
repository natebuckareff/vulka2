use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::fs;

use anyhow::{Result, bail};
use blake3::{Hash, Hasher};
use compact_str::CompactString;
use shader_slang as slang;

use crate::SlangCompiler;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(CompactString);

impl ModuleId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ModuleId {
    fn from(s: &str) -> Self {
        ModuleId(s.into())
    }
}

impl std::fmt::Display for ModuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlangShaderStage {
    Vertex,
    Fragment,
    Compute,
}

impl SlangShaderStage {
    pub(crate) fn from_slang(stage: slang::Stage) -> Option<Self> {
        match stage {
            slang::Stage::Vertex => Some(SlangShaderStage::Vertex),
            slang::Stage::Fragment => Some(SlangShaderStage::Fragment),
            slang::Stage::Compute => Some(SlangShaderStage::Compute),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SlangEntrypoint {
    module_id: ModuleId,
    name: CompactString,
    stage: SlangShaderStage,
}

impl SlangEntrypoint {
    pub fn module_id(&self) -> &ModuleId {
        &self.module_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn stage(&self) -> SlangShaderStage {
        self.stage
    }
}

impl PartialOrd for SlangEntrypoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SlangEntrypoint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.stage, &self.name, &self.module_id).cmp(&(
            &other.stage,
            &other.name,
            &other.module_id,
        ))
    }
}

pub struct SlangModule {
    id: ModuleId,
    module: slang::Module,
    content_hash: Hash,
    entrypoints: OnceCell<Vec<SlangEntrypoint>>,
}

impl SlangModule {
    pub(crate) fn new(module: slang::Module) -> Self {
        let id = ModuleId::from(module.unique_identity());
        let content_hash = Self::compute_content_hash(&module);
        Self {
            id,
            module,
            content_hash,
            entrypoints: OnceCell::new(),
        }
    }

    pub fn id(&self) -> &ModuleId {
        &self.id
    }

    pub fn name(&self) -> &str {
        self.module.name()
    }

    pub fn file_path(&self) -> &str {
        self.module.file_path()
    }

    pub fn content_hash(&self) -> Hash {
        self.content_hash
    }

    pub fn entrypoints(&self) -> &[SlangEntrypoint] {
        self.entrypoints.get_or_init(|| self.compute_entrypoints())
    }

    pub fn entrypoint(&self, stage: SlangShaderStage, name: &str) -> Result<SlangEntrypoint> {
        self.entrypoints()
            .iter()
            .find(|ep| ep.stage == stage && ep.name == name)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "entrypoint '{}' with stage {:?} not found in module '{}'",
                    name,
                    stage,
                    self.name()
                )
            })
    }

    fn compute_entrypoints(&self) -> Vec<SlangEntrypoint> {
        let mut result = Vec::new();

        for slang_ep in self.module.entry_points() {
            let func = slang_ep.function_reflection();
            if let Some(name) = func.name() {
                let component: slang::ComponentType = slang_ep.clone().into();
                if let Ok(layout) = component.layout(0) {
                    if let Some(ep_layout) = layout.entry_point_by_index(0) {
                        if let Some(stage) = SlangShaderStage::from_slang(ep_layout.stage()) {
                            result.push(SlangEntrypoint {
                                module_id: self.id.clone(),
                                name: name.into(),
                                stage,
                            });
                        }
                    }
                }
            }
        }

        result
    }

    fn compute_content_hash(module: &slang::Module) -> Hash {
        let mut hasher = Hasher::new();

        let main_path = module.file_path();
        if let Ok(contents) = fs::read(main_path) {
            hasher.update(&contents);
        } else {
            hasher.update(main_path.as_bytes());
        }

        let mut deps: Vec<&str> = module.dependency_file_paths().collect();
        deps.sort();

        hasher.update(&(deps.len() as u32).to_le_bytes());
        for dep_path in deps {
            if let Ok(contents) = fs::read(dep_path) {
                hasher.update(&contents);
            } else {
                hasher.update(dep_path.as_bytes());
            }
        }

        hasher.finalize()
    }
}

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
            .filter(|ep| ep.stage == stage)
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
            hasher.update(&[ep.stage as u8]);
            hasher.update(ep.name.as_bytes());
            hasher.update(&[0]);
            hasher.update(ep.module_id.as_str().as_bytes());
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
