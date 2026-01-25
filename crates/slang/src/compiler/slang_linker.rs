use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::fs;

use anyhow::{Result, bail};
use blake3::{Hash, Hasher};
use compact_str::CompactString;
use shader_slang as slang;

use crate::SlangCompiler;

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
    module_identity: CompactString,
    name: CompactString,
    stage: SlangShaderStage,
}

impl SlangEntrypoint {
    pub fn module_identity(&self) -> &str {
        &self.module_identity
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
        (&self.stage, &self.name, &self.module_identity).cmp(&(
            &other.stage,
            &other.name,
            &other.module_identity,
        ))
    }
}

pub struct SlangModule {
    module: slang::Module,
    content_hash: Hash,
    entrypoints: OnceCell<Vec<SlangEntrypoint>>,
}

impl SlangModule {
    pub(crate) fn new(module: slang::Module) -> Self {
        let content_hash = Self::compute_content_hash(&module);
        Self {
            module,
            content_hash,
            entrypoints: OnceCell::new(),
        }
    }

    pub fn name(&self) -> &str {
        self.module.name()
    }

    pub fn file_path(&self) -> &str {
        self.module.file_path()
    }

    pub fn unique_identity(&self) -> &str {
        self.module.unique_identity()
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
        let module_identity: CompactString = self.unique_identity().into();
        let mut result = Vec::new();

        for slang_ep in self.module.entry_points() {
            let func = slang_ep.function_reflection();
            if let Some(name) = func.name() {
                let component: slang::ComponentType = slang_ep.clone().into();
                if let Ok(layout) = component.layout(0) {
                    if let Some(ep_layout) = layout.entry_point_by_index(0) {
                        if let Some(stage) = SlangShaderStage::from_slang(ep_layout.stage()) {
                            result.push(SlangEntrypoint {
                                module_identity: module_identity.clone(),
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
    compiler: &'a mut SlangCompiler,
    modules: BTreeSet<CompactString>,
    entrypoints: BTreeSet<SlangEntrypoint>,
    module_hashes: Vec<(CompactString, Hash)>,
}

impl<'a> SlangLinker<'a> {
    pub(crate) fn new(compiler: &'a mut SlangCompiler) -> Self {
        Self {
            compiler,
            modules: BTreeSet::new(),
            entrypoints: BTreeSet::new(),
            module_hashes: Vec::new(),
        }
    }

    fn add_module_if_missing(&mut self, module: &SlangModule) {
        let identity: CompactString = module.unique_identity().into();
        if self.modules.insert(identity.clone()) {
            self.module_hashes.push((identity, module.content_hash()));
        }
    }

    pub fn add_module(mut self, module: &SlangModule) -> Self {
        self.add_module_if_missing(module);
        self
    }

    pub fn add_stage(mut self, module: &SlangModule, stage: SlangShaderStage) -> Self {
        self.add_module_if_missing(module);
        for ep in module.entrypoints() {
            if ep.stage == stage {
                self.entrypoints.insert(ep.clone());
            }
        }
        self
    }

    pub fn add_all_entrypoints(mut self, module: &SlangModule) -> Self {
        self.add_module_if_missing(module);
        for ep in module.entrypoints() {
            self.entrypoints.insert(ep.clone());
        }
        self
    }

    pub fn add_entrypoint(mut self, entrypoint: SlangEntrypoint) -> Self {
        let identity: CompactString = entrypoint.module_identity().into();
        if !self.modules.contains(&identity) {
            if let Some(hash) = self.compiler.module_hash(&identity) {
                self.modules.insert(identity.clone());
                self.module_hashes.push((identity, hash));
            }
        }
        self.entrypoints.insert(entrypoint);
        self
    }

    pub fn link(mut self) -> Result<SlangProgram> {
        if self.entrypoints.is_empty() {
            bail!("no entrypoints to link");
        }

        // Compute the program key
        let program_key = self.compute_program_key();

        // TODO: Check cache for existing program
        // For now, always compile

        // Actually perform the linking via shader_slang
        // TODO: Implement actual linking
        // For now just return a stub

        Ok(SlangProgram {
            key: program_key,
            entrypoints: self.entrypoints.into_iter().collect(),
        })
    }

    fn compute_program_key(&mut self) -> SlangProgramKey {
        let mut hasher = Hasher::new();

        hasher.update(self.compiler.options_hash().0.as_bytes());

        self.module_hashes.sort_by(|a, b| a.0.cmp(&b.0));
        hasher.update(&(self.module_hashes.len() as u32).to_le_bytes());

        for (identity, hash) in &self.module_hashes {
            hasher.update(identity.as_bytes());
            hasher.update(&[0]);
            hasher.update(hash.as_bytes());
        }

        hasher.update(&(self.entrypoints.len() as u32).to_le_bytes());

        for ep in &self.entrypoints {
            hasher.update(&[ep.stage as u8]);
            hasher.update(ep.name.as_bytes());
            hasher.update(&[0]);
            hasher.update(ep.module_identity.as_bytes());
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
