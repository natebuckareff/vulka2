use std::{cell::OnceCell, fs};

use anyhow::Result;
use blake3::{Hash, Hasher};
use compact_str::CompactString;
use shader_slang as slang;

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

    pub(crate) fn slang_module(&self) -> &slang::Module {
        &self.module
    }

    pub(crate) fn slang_entrypoint(&self, ep: &SlangEntrypoint) -> Result<slang::EntryPoint> {
        if ep.module_id != self.id {
            anyhow::bail!(
                "entrypoint '{}' belongs to module '{}', not '{}'",
                ep.name,
                ep.module_id,
                self.id
            );
        }

        for slang_ep in self.module.entry_points() {
            let func = slang_ep.function_reflection();
            if let Some(name) = func.name() {
                if name == ep.name.as_str() {
                    // Verify stage matches
                    let component: slang::ComponentType = slang_ep.clone().into();
                    if let Ok(layout) = component.layout(0) {
                        if let Some(ep_layout) = layout.entry_point_by_index(0) {
                            if SlangShaderStage::from_slang(ep_layout.stage()) == Some(ep.stage) {
                                return Ok(slang_ep);
                            }
                        }
                    }
                }
            }
        }

        anyhow::bail!(
            "entrypoint '{}' ({:?}) not found in module '{}'",
            ep.name,
            ep.stage,
            self.name()
        )
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
