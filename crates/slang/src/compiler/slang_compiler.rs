use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ffi::CString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use blake3::{Hash, Hasher};
use serde::{Deserialize, Serialize};
use shader_slang as slang;

use crate::{ModuleId, SlangLinker, SlangModule};

pub const SLANG_CACHE_KEY_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompilerOptionsHash(pub Hash);

#[derive(Clone, Copy)]
pub struct BindlessConfig {
    pub space_index: i32,
    pub policy: BindlessPolicy,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum BindlessPolicy {
    Indexable,
    Mutable,
}

pub struct SlangCompilerBuilder {
    global_session: slang::GlobalSession,
    capabilities: Vec<(String, slang::CapabilityID)>,
    bindless_space_index: Option<i32>,
    bindless_policy: Option<BindlessPolicy>,
    optimization: slang::OptimizationLevel,
    matrix_layout_row: bool,
    cache_path: Option<PathBuf>,
    search_paths: Vec<PathBuf>,
}

impl SlangCompilerBuilder {
    pub fn new() -> Result<Self> {
        let global_session =
            slang::GlobalSession::new().context("failed to create slang global session")?;

        Ok(Self {
            global_session,
            capabilities: Vec::new(),
            bindless_space_index: None,
            bindless_policy: None,
            optimization: slang::OptimizationLevel::Default,
            matrix_layout_row: true,
            cache_path: None,
            search_paths: Vec::new(),
        })
    }

    pub fn capability(mut self, name: &str) -> Result<Self> {
        let cap = self.global_session.find_capability(name);
        if cap.is_unknown() {
            bail!("unknown capability: {name}");
        }
        self.capabilities.push((name.to_string(), cap));
        Ok(self)
    }

    pub fn bindless_space_index(mut self, index: i32) -> Self {
        self.bindless_space_index = Some(index);
        self
    }

    pub fn bindless_policy(mut self, policy: BindlessPolicy) -> Self {
        self.bindless_policy = Some(policy);
        self
    }

    pub fn optimization(mut self, level: slang::OptimizationLevel) -> Self {
        self.optimization = level;
        self
    }

    pub fn matrix_layout_row(mut self, enable: bool) -> Self {
        self.matrix_layout_row = enable;
        self
    }

    pub fn cache_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.cache_path = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn search_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.search_paths.push(path.as_ref().to_path_buf());
        self
    }

    pub fn build(self) -> Result<SlangCompiler> {
        let profile = self.global_session.find_profile("spirv_1_6");
        if profile.is_unknown() {
            bail!("spirv_1_6 profile not found");
        }

        let options_hash = self.compute_options_hash();

        let mut options = slang::CompilerOptions::default()
            .vulkan_use_entry_point_name(true)
            .emit_spirv_directly(true)
            .optimization(self.optimization)
            .matrix_layout_row(self.matrix_layout_row);

        for (_, cap) in self.capabilities {
            options = options.capability(cap);
        }

        if let Some(index) = self.bindless_space_index {
            options = options.bindless_space_index(index);
        }

        let target = slang::TargetDesc::default()
            .format(slang::CompileTarget::Spirv)
            .profile(profile)
            .options(&options);

        let search_path_cstrings: Vec<CString> = self
            .search_paths
            .iter()
            .map(|p| CString::new(p.to_string_lossy().as_bytes()).unwrap())
            .collect();

        let search_path_ptrs: Vec<*const i8> =
            search_path_cstrings.iter().map(|s| s.as_ptr()).collect();

        let targets = [target];
        let session_desc = slang::SessionDesc::default()
            .targets(&targets)
            .search_paths(&search_path_ptrs)
            .options(&options);

        let session = self
            .global_session
            .create_session(&session_desc)
            .context("failed to create slang session")?;

        let bindless_config = self.bindless_space_index.map(|space_index| BindlessConfig {
            space_index,
            policy: self.bindless_policy.unwrap_or(BindlessPolicy::Indexable),
        });

        Ok(SlangCompiler {
            bindless_config,
            global_session: self.global_session,
            session,
            options_hash,
            cache_path: self.cache_path,
            search_paths: self.search_paths,
            modules: HashMap::new(),
        })
    }

    fn compute_options_hash(&self) -> CompilerOptionsHash {
        let mut hasher = Hasher::new();

        hasher.update(&[SLANG_CACHE_KEY_VERSION]);
        hasher.update(self.global_session.build_tag_string().as_bytes());

        let mut cap_names: Vec<&str> = self
            .capabilities
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        cap_names.sort();
        hasher.update(&(cap_names.len() as u32).to_le_bytes());

        for name in cap_names {
            hasher.update(name.as_bytes());
            hasher.update(&[0]);
        }

        match self.bindless_space_index {
            Some(index) => {
                hasher.update(&[1]);
                hasher.update(&index.to_le_bytes());
            }
            None => {
                hasher.update(&[0]);
            }
        }

        hasher.update(&(self.optimization as u32).to_le_bytes());
        hasher.update(&[self.matrix_layout_row as u8]);

        CompilerOptionsHash(hasher.finalize())
    }
}

pub struct SlangCompiler {
    bindless_config: Option<BindlessConfig>,
    global_session: slang::GlobalSession,
    session: slang::Session,
    options_hash: CompilerOptionsHash,
    cache_path: Option<PathBuf>,
    search_paths: Vec<PathBuf>,
    modules: HashMap<ModuleId, SlangModule>,
}

impl SlangCompiler {
    pub fn options_hash(&self) -> CompilerOptionsHash {
        self.options_hash
    }

    pub fn bindless_config(&self) -> &Option<BindlessConfig> {
        &self.bindless_config
    }

    pub fn load_module<P: AsRef<Path>>(&mut self, path: P) -> Result<&SlangModule> {
        let path = path.as_ref();

        let module_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("invalid module path: {}", path.display()))?;

        let module = self
            .session
            .load_module(module_name)
            .map_err(|e| anyhow!("failed to load module '{}': {}", module_name, e))?;

        let slang_module = SlangModule::new(module);
        let id = slang_module.id().clone();

        Ok(match self.modules.entry(id) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(slang_module),
        })
    }

    pub fn module(&self, id: &ModuleId) -> Option<&SlangModule> {
        self.modules.get(id)
    }

    /// Access the underlying slang session for linking operations.
    pub(crate) fn session(&self) -> &slang::Session {
        &self.session
    }

    pub fn linker(&self) -> SlangLinker<'_> {
        SlangLinker::new(self)
    }
}
