use std::ffi::CString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use shader_slang::{
    CapabilityID, CompileTarget, CompilerOptions, GlobalSession, Module, OptimizationLevel,
    Session, SessionDesc, TargetDesc,
};

pub const SLANG_CACHE_KEY_VERSION: u8 = 1;

pub struct SlangCompilerBuilder {
    global_session: GlobalSession,
    capabilities: Vec<CapabilityID>,
    bindless_space_index: Option<i32>,
    optimization: OptimizationLevel,
    matrix_layout_row: bool,
    cache_path: Option<PathBuf>,
    search_paths: Vec<PathBuf>,
}

impl SlangCompilerBuilder {
    pub fn new() -> Result<Self> {
        let global_session =
            GlobalSession::new().context("failed to create slang global session")?;

        Ok(Self {
            global_session,
            capabilities: Vec::new(),
            bindless_space_index: None,
            optimization: OptimizationLevel::Default,
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
        self.capabilities.push(cap);
        Ok(self)
    }

    pub fn bindless_space_index(mut self, index: i32) -> Self {
        self.bindless_space_index = Some(index);
        self
    }

    pub fn optimization(mut self, level: OptimizationLevel) -> Self {
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

        let mut options = CompilerOptions::default()
            .vulkan_use_entry_point_name(true)
            .emit_spirv_directly(true)
            .optimization(self.optimization)
            .matrix_layout_row(self.matrix_layout_row);

        for cap in &self.capabilities {
            options = options.capability(*cap);
        }

        if let Some(index) = self.bindless_space_index {
            options = options.bindless_space_index(index);
        }

        let target = TargetDesc::default()
            .format(CompileTarget::Spirv)
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
        let session_desc = SessionDesc::default()
            .targets(&targets)
            .search_paths(&search_path_ptrs)
            .options(&options);

        let session = self
            .global_session
            .create_session(&session_desc)
            .context("failed to create slang session")?;

        Ok(SlangCompiler {
            global_session: self.global_session,
            session,
            cache_path: self.cache_path,
            search_paths: self.search_paths,
            // _search_path_cstrings: search_path_cstrings,
        })
    }
}

pub struct SlangCompiler {
    global_session: GlobalSession,
    session: Session,
    cache_path: Option<PathBuf>,
    search_paths: Vec<PathBuf>,
}

impl SlangCompiler {
    pub fn load_module<P: AsRef<Path>>(&mut self, path: P) -> Result<SlangModule> {
        let path = path.as_ref();

        let module_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("invalid module path: {}", path.display()))?;

        let module = self
            .session
            .load_module(module_name)
            .map_err(|e| anyhow!("failed to load module '{}': {}", module_name, e))?;

        Ok(SlangModule { module })
    }

    pub fn linker(&mut self) -> SlangLinker<'_> {
        SlangLinker { _compiler: self }
    }
}

pub struct SlangModule {
    module: Module,
}

impl SlangModule {
    pub fn name(&self) -> &str {
        self.module.name()
    }

    pub fn file_path(&self) -> &str {
        self.module.file_path()
    }

    pub fn unique_identity(&self) -> &str {
        self.module.unique_identity()
    }
}

pub struct SlangLinker<'a> {
    _compiler: &'a mut SlangCompiler,
}
