use std::ffi::{CStr, CString};
use std::sync::Arc;

use anyhow::{Context, Result};
use slang::{SlangPipelineProgram, SlangShaderStage};

use crate::gpu::{Device, OwnedShaderModule};

pub struct ShaderModule {
    owned: OwnedShaderModule,
    stage: SlangShaderStage,
    entrypoint: CString,
}

impl ShaderModule {
    pub fn new(
        device: Arc<Device>,
        program: &SlangPipelineProgram,
        stage: SlangShaderStage,
    ) -> Result<Self> {
        use vulkanalia::prelude::v1_0::*;

        let entrypoint = program.entrypoint(stage).context("entrypoint not found")?;
        let code = program.code(stage).context("entrypoint code not found")?;

        let info = vk::ShaderModuleCreateInfo::builder()
            .code_size(code.len_bytes())
            .code(code.as_ref());

        let device = device.handle().clone();
        let owned = OwnedShaderModule::new(device, &info)?;

        let stage = entrypoint.stage();
        let entrypoint = CString::new(entrypoint.name())?;

        Ok(Self {
            owned,
            stage,
            entrypoint,
        })
    }

    pub(crate) fn owned(&self) -> &OwnedShaderModule {
        &self.owned
    }

    pub fn stage(&self) -> SlangShaderStage {
        self.stage
    }

    pub fn entrypoint(&self) -> &CStr {
        &self.entrypoint
    }
}
