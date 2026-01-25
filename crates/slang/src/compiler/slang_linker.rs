use shader_slang as slang;

use crate::SlangCompiler;

pub struct SlangModule {
    module: slang::Module,
}

impl SlangModule {
    pub(crate) fn new(module: slang::Module) -> Self {
        Self { module }
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
}

pub struct SlangLinker<'a> {
    compiler: &'a mut SlangCompiler,
}

impl<'a> SlangLinker<'a> {
    pub(crate) fn new(compiler: &'a mut SlangCompiler) -> Self {
        Self { compiler }
    }
}
