use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use shader_slang as slang;

pub use shader_slang::{BindingType, ParameterCategory, Stage, TypeKind};

#[derive(Clone, Debug)]
pub struct SlangShader {
    layout: SlangLayout,
    entry_points: Vec<SlangEntryPoint>,
}

impl SlangShader {
    pub fn layout(&self) -> &SlangLayout {
        &self.layout
    }

    pub fn entry_points(&self) -> &[SlangEntryPoint] {
        &self.entry_points
    }

    pub fn entry_point_by_name(&self, name: &str) -> Option<&SlangEntryPoint> {
        self.entry_points.iter().find(|entry| entry.name == name)
    }
}

#[derive(Clone, Debug)]
pub struct SlangEntryPoint {
    pub name: String,
    pub stage: slang::Stage,
    pub spirv: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SlangLayout {
    pub global_params: Vec<SlangParameterLayout>,
    pub entry_points: Vec<SlangEntryPointLayout>,
    pub descriptor_sets: Vec<SlangDescriptorSetLayout>,
    pub bindless_heap: Option<SlangBindlessHeap>,
}

impl SlangLayout {
    pub fn push_constant_params(&self) -> impl Iterator<Item = &SlangParameterLayout> {
        self.global_params
            .iter()
            .filter(|param| param.category == Some(slang::ParameterCategory::PushConstantBuffer))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangEntryPointLayout {
    pub name: String,
    pub stage: slang::Stage,
    pub parameters: Vec<SlangParameterLayout>,
    pub result: Option<SlangParameterLayout>,
    pub descriptor_sets: Vec<SlangDescriptorSetLayout>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangParameterLayout {
    pub name: Option<String>,
    pub category: Option<slang::ParameterCategory>,
    pub binding: SlangBinding,
    pub stage: SlangStageMask,
    pub layouts: Vec<SlangLayoutMetric>,
    pub type_layout: SlangTypeLayout,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangBinding {
    pub space: u32,
    pub index: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SlangStageMask {
    bits: u32,
}

impl SlangStageMask {
    pub fn insert(&mut self, stage: slang::Stage) {
        let bit = 1u32 << (stage as u32);
        self.bits |= bit;
    }

    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    pub fn bits(&self) -> u32 {
        self.bits
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangDescriptorSetLayout {
    pub set: u32,
    pub bindings: Vec<SlangDescriptorBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangDescriptorBinding {
    pub binding: u32,
    pub binding_type: slang::BindingType,
    pub category: slang::ParameterCategory,
    pub descriptor_count: DescriptorCount,
    pub stage: SlangStageMask,
    pub range_offset: LayoutUnit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DescriptorCount {
    Exact(u32),
    Unbounded,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangBindlessHeap {
    pub set: u32,
    pub bindings: Vec<SlangDescriptorBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangTypeLayout {
    pub name: Option<String>,
    pub kind: slang::TypeKind,
    pub parameter_category: slang::ParameterCategory,
    pub layouts: Vec<SlangLayoutMetric>,
    pub container_offsets: Vec<SlangLayoutMetric>,
    pub element_offsets: Vec<SlangLayoutMetric>,
    pub fields: Vec<SlangFieldLayout>,
    pub element_type: Option<Box<SlangTypeLayout>>,
    pub array: Option<SlangArrayLayout>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangFieldLayout {
    pub name: Option<String>,
    pub layouts: Vec<SlangLayoutMetric>,
    pub type_layout: SlangTypeLayout,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangArrayLayout {
    pub element_count: u32,
    pub total_elements: u32,
    pub element_stride: Option<LayoutUnit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlangLayoutMetric {
    pub offset: Option<LayoutUnit>,
    pub size: Option<LayoutUnit>,
    pub stride: Option<LayoutUnit>,
    pub align: Option<LayoutUnit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LayoutUnit {
    None(u32),
    Mixed(u32),
    ConstantBuffer(u32),
    ShaderResource(u32),
    UnorderedAccess(u32),
    VaryingInput(u32),
    VaryingOutput(u32),
    SamplerState(u32),
    Uniform(u32),
    DescriptorTableSlot(u32),
    SpecializationConstant(u32),
    PushConstantBuffer(u32),
    RegisterSpace(u32),
    Generic(u32),
    RayPayload(u32),
    HitAttributes(u32),
    CallablePayload(u32),
    ShaderRecord(u32),
    Other {
        category: slang::ParameterCategory,
        units: u32,
    },
}

impl LayoutUnit {
    pub fn units(&self) -> u32 {
        match self {
            LayoutUnit::None(value)
            | LayoutUnit::Mixed(value)
            | LayoutUnit::ConstantBuffer(value)
            | LayoutUnit::ShaderResource(value)
            | LayoutUnit::UnorderedAccess(value)
            | LayoutUnit::VaryingInput(value)
            | LayoutUnit::VaryingOutput(value)
            | LayoutUnit::SamplerState(value)
            | LayoutUnit::Uniform(value)
            | LayoutUnit::DescriptorTableSlot(value)
            | LayoutUnit::SpecializationConstant(value)
            | LayoutUnit::PushConstantBuffer(value)
            | LayoutUnit::RegisterSpace(value)
            | LayoutUnit::Generic(value)
            | LayoutUnit::RayPayload(value)
            | LayoutUnit::HitAttributes(value)
            | LayoutUnit::CallablePayload(value)
            | LayoutUnit::ShaderRecord(value) => *value,
            LayoutUnit::Other { units, .. } => *units,
        }
    }

    pub fn to_bytes(&self) -> Option<u32> {
        match self {
            LayoutUnit::Uniform(value) => Some(*value),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SlangShaderBuilder {
    module_name: String,
    search_path: PathBuf,
    optimization: slang::OptimizationLevel,
    bindless_space_index: Option<i32>,
}

impl SlangShaderBuilder {
    pub fn new(module_name: impl Into<String>) -> Self {
        Self {
            module_name: module_name.into(),
            search_path: PathBuf::from("."),
            optimization: slang::OptimizationLevel::High,
            bindless_space_index: None,
        }
    }

    pub fn search_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.search_path = path.into();
        self
    }

    pub fn optimization(mut self, level: slang::OptimizationLevel) -> Self {
        self.optimization = level;
        self
    }

    pub fn bindless_space_index(mut self, index: i32) -> Self {
        self.bindless_space_index = Some(index);
        self
    }

    pub fn build(self) -> Result<SlangShader> {
        let global_session =
            slang::GlobalSession::new().context("failed to create Slang session")?;

        let physical_storage = global_session.find_capability("SPV_EXT_physical_storage_buffer");
        if physical_storage.is_unknown() {
            return Err(anyhow!(
                "Slang capability SPV_EXT_physical_storage_buffer is unavailable"
            ));
        }
        let descriptor_indexing = global_session.find_capability("SPV_EXT_descriptor_indexing");
        if descriptor_indexing.is_unknown() {
            return Err(anyhow!(
                "Slang capability SPV_EXT_descriptor_indexing is unavailable"
            ));
        }

        let mut compiler_options = slang::CompilerOptions::default()
            .optimization(self.optimization)
            .matrix_layout_row(true)
            .vulkan_use_entry_point_name(true)
            .capability(physical_storage)
            .capability(descriptor_indexing);
        if let Some(bindless_space_index) = self.bindless_space_index {
            compiler_options = compiler_options.bindless_space_index(bindless_space_index);
        }

        let profile = global_session.find_profile("glsl_450");
        if profile.is_unknown() {
            return Err(anyhow!("Slang profile glsl_450 is unavailable"));
        }
        let target_desc = slang::TargetDesc::default()
            .format(slang::CompileTarget::Spirv)
            .profile(profile)
            .options(&compiler_options);
        let targets = [target_desc];

        let search_path_cstr =
            std::ffi::CString::new(self.search_path.as_os_str().to_string_lossy().as_ref())?;
        let search_paths = [search_path_cstr.as_ptr()];

        let session_desc = slang::SessionDesc::default()
            .targets(&targets)
            .search_paths(&search_paths)
            .options(&compiler_options);

        let session = global_session
            .create_session(&session_desc)
            .context("failed to create Slang session")?;

        let module = session
            .load_module(&self.module_name)
            .map_err(|err| anyhow!("failed to load module {}: {err:?}", self.module_name))?;

        let entry_points: Vec<_> = module.entry_points().collect();
        if entry_points.is_empty() {
            return Err(anyhow!("module {} has no entry points", self.module_name));
        }

        let mut components = Vec::with_capacity(1 + entry_points.len());
        components.push(module.clone().into());
        for entry in &entry_points {
            components.push(entry.clone().into());
        }

        let linked_program = session
            .create_composite_component_type(&components)?
            .link()?;

        let reflection = linked_program.layout(0)?;

        let entry_points = reflection
            .entry_points()
            .enumerate()
            .map(|(index, entry)| {
                let name = entry.name().unwrap_or("<unnamed>").to_string();
                let stage = entry.stage();
                let blob = linked_program.entry_point_code(index as i64, 0)?;
                let spirv = blob_to_words(&blob)?;
                Ok(SlangEntryPoint { name, stage, spirv })
            })
            .collect::<Result<Vec<_>>>()?;

        let layout = build_layout(&reflection, &entry_points, self.bindless_space_index)?;

        Ok(SlangShader {
            layout,
            entry_points,
        })
    }
}

fn build_layout(
    reflection: &slang::reflection::Shader,
    entry_points: &[SlangEntryPoint],
    bindless_space_index: Option<i32>,
) -> Result<SlangLayout> {
    let mut all_stages = SlangStageMask::default();
    for entry in entry_points {
        all_stages.insert(entry.stage);
    }

    let global_var_layout = reflection
        .global_params_var_layout()
        .context("missing global params layout")?;
    let global_type_layout = global_var_layout
        .type_layout()
        .context("missing global params type layout")?;

    let global_params = build_params_from_scope(global_var_layout, all_stages)?;

    let descriptor_sets = collect_descriptor_sets(global_type_layout, all_stages)?;

    let entry_points_layouts = reflection
        .entry_points()
        .map(|entry| {
            let name = entry.name().unwrap_or("<unnamed>").to_string();
            let stage = entry.stage();
            let mut stage_mask = SlangStageMask::default();
            stage_mask.insert(stage);

            let var_layout = entry
                .var_layout()
                .context("missing entry-point var layout")?;
            let type_layout = var_layout
                .type_layout()
                .context("missing entry-point type layout")?;
            let parameters = build_params_from_scope(var_layout, stage_mask)?;

            let result = entry
                .result_var_layout()
                .and_then(|layout| build_param_layout(layout, stage_mask).ok());

            let entry_descriptor_sets = collect_descriptor_sets(type_layout, stage_mask)?;

            Ok(SlangEntryPointLayout {
                name,
                stage,
                parameters,
                result,
                descriptor_sets: entry_descriptor_sets,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let bindless_heap = bindless_space_index
        .map(|space_index| {
            if space_index < 0 {
                return Err(anyhow!("bindless space index must be non-negative"));
            }
            let binding = SlangDescriptorBinding {
                binding: vkmutable_binding_index(BindlessKind::SampledImage),
                binding_type: slang::BindingType::Texture,
                category: slang::ParameterCategory::DescriptorTableSlot,
                descriptor_count: DescriptorCount::Unbounded,
                stage: all_stages,
                range_offset: LayoutUnit::DescriptorTableSlot(vkmutable_binding_index(
                    BindlessKind::SampledImage,
                )),
            };
            Ok(SlangBindlessHeap {
                set: space_index as u32,
                bindings: vec![binding],
            })
        })
        .transpose()?;

    Ok(SlangLayout {
        global_params,
        entry_points: entry_points_layouts,
        descriptor_sets,
        bindless_heap,
    })
}

fn build_params_from_scope(
    scope_layout: &slang::reflection::VariableLayout,
    stage: SlangStageMask,
) -> Result<Vec<SlangParameterLayout>> {
    let Some(type_layout) = scope_layout.type_layout() else {
        return Ok(Vec::new());
    };

    let mut params = Vec::new();
    for index in 0..type_layout.field_count() {
        let Some(field) = type_layout.field_by_index(index) else {
            continue;
        };
        let layout = build_param_layout(field, stage)?;
        params.push(layout);
    }
    Ok(params)
}

fn build_param_layout(
    param: &slang::reflection::VariableLayout,
    stage: SlangStageMask,
) -> Result<SlangParameterLayout> {
    let name = param.name().map(|name| name.to_string());
    let category = param.category();
    let binding = SlangBinding {
        space: param.binding_space(),
        index: param.binding_index(),
    };
    let layouts = layout_metrics_from_var_layout(param);
    let type_layout = param
        .type_layout()
        .context("missing parameter type layout")?;
    let type_layout = build_type_layout(type_layout)?;

    Ok(SlangParameterLayout {
        name,
        category,
        binding,
        stage,
        layouts,
        type_layout,
    })
}

fn build_type_layout(layout: &slang::reflection::TypeLayout) -> Result<SlangTypeLayout> {
    let name = layout
        .ty()
        .and_then(|ty| ty.name())
        .map(|name| name.to_string());
    let kind = layout.kind();
    let parameter_category = layout.parameter_category();

    let layouts = layout_metrics_from_type_layout(layout);
    let container_offsets = layout
        .container_var_layout()
        .map(layout_metrics_from_var_layout)
        .unwrap_or_default();
    let element_offsets = layout
        .element_var_layout()
        .map(layout_metrics_from_var_layout)
        .unwrap_or_default();

    let fields = (0..layout.field_count())
        .filter_map(|index| layout.field_by_index(index))
        .map(|field| build_field_layout(field))
        .collect::<Result<Vec<_>>>()?;

    let element_type = layout
        .element_type_layout()
        .map(build_type_layout)
        .transpose()?
        .map(Box::new);

    let array = layout.element_count().map(|element_count| {
        let total_elements = layout.total_array_element_count() as u32;
        let element_stride = Some(LayoutUnit::DescriptorTableSlot(
            layout.element_stride(slang::ParameterCategory::DescriptorTableSlot) as u32,
        ));
        SlangArrayLayout {
            element_count: element_count as u32,
            total_elements,
            element_stride,
        }
    });

    Ok(SlangTypeLayout {
        name,
        kind,
        parameter_category,
        layouts,
        container_offsets,
        element_offsets,
        fields,
        element_type,
        array,
    })
}

fn build_field_layout(field: &slang::reflection::VariableLayout) -> Result<SlangFieldLayout> {
    let name = field.name().map(|name| name.to_string());
    let layouts = layout_metrics_from_var_layout(field);
    let type_layout = field.type_layout().context("missing field type layout")?;
    let type_layout = build_type_layout(type_layout)?;
    Ok(SlangFieldLayout {
        name,
        layouts,
        type_layout,
    })
}

fn collect_descriptor_sets(
    layout: &slang::reflection::TypeLayout,
    stage: SlangStageMask,
) -> Result<Vec<SlangDescriptorSetLayout>> {
    let mut sets: BTreeMap<u32, BTreeMap<u32, SlangDescriptorBinding>> = BTreeMap::new();
    let set_count = layout.descriptor_set_count();
    for set_index in 0..set_count {
        let set = layout.descriptor_set_space_offset(set_index) as u32;
        let range_count = layout.descriptor_set_descriptor_range_count(set_index);
        for range_index in 0..range_count {
            let binding =
                layout.descriptor_set_descriptor_range_index_offset(set_index, range_index) as u32;
            let descriptor_count_raw =
                layout.descriptor_set_descriptor_range_descriptor_count(set_index, range_index);
            let descriptor_count = if descriptor_count_raw <= 0 {
                DescriptorCount::Unbounded
            } else {
                DescriptorCount::Exact(descriptor_count_raw as u32)
            };
            let binding_type = layout.descriptor_set_descriptor_range_type(set_index, range_index);
            if binding_type == slang::BindingType::PushConstant {
                continue;
            }
            let category = layout.descriptor_set_descriptor_range_category(set_index, range_index);

            let set_map = sets.entry(set).or_default();
            let entry = set_map.entry(binding).or_insert(SlangDescriptorBinding {
                binding,
                binding_type,
                category,
                descriptor_count: descriptor_count.clone(),
                stage,
                range_offset: LayoutUnit::DescriptorTableSlot(
                    layout.descriptor_set_descriptor_range_index_offset(set_index, range_index)
                        as u32,
                ),
            });
            if entry.binding_type != binding_type {
                return Err(anyhow!(
                    "descriptor type mismatch for set {set} binding {binding}: {:?} vs {:?}",
                    entry.binding_type,
                    binding_type
                ));
            }
            if entry.descriptor_count != descriptor_count {
                return Err(anyhow!(
                    "descriptor count mismatch for set {set} binding {binding}"
                ));
            }
            if entry.category != category {
                return Err(anyhow!(
                    "descriptor category mismatch for set {set} binding {binding}: {:?} vs {:?}",
                    entry.category,
                    category
                ));
            }
        }
    }

    let sets = sets
        .into_iter()
        .map(|(set, bindings)| {
            let mut bindings: Vec<_> = bindings.into_values().collect();
            bindings.sort_by_key(|binding| binding.binding);
            SlangDescriptorSetLayout { set, bindings }
        })
        .collect::<Vec<_>>();

    Ok(sets)
}

fn layout_metrics_from_var_layout(
    layout: &slang::reflection::VariableLayout,
) -> Vec<SlangLayoutMetric> {
    let categories: Vec<_> = layout.categories().collect();
    if categories.is_empty() {
        return Vec::new();
    }
    categories
        .into_iter()
        .map(|category| SlangLayoutMetric {
            offset: Some(layout_unit(category, layout.offset(category) as u32)),
            size: None,
            stride: None,
            align: None,
        })
        .collect()
}

fn layout_metrics_from_type_layout(
    layout: &slang::reflection::TypeLayout,
) -> Vec<SlangLayoutMetric> {
    let categories: Vec<_> = layout.categories().collect();
    if categories.is_empty() {
        return Vec::new();
    }
    categories
        .into_iter()
        .map(|category| SlangLayoutMetric {
            offset: None,
            size: Some(layout_unit(category, layout.size(category) as u32)),
            stride: Some(layout_unit(category, layout.stride(category) as u32)),
            align: Some(layout_unit(category, layout.alignment(category) as u32)),
        })
        .collect()
}

fn layout_unit(category: slang::ParameterCategory, units: u32) -> LayoutUnit {
    use slang::ParameterCategory::*;
    match category {
        None => LayoutUnit::None(units),
        Mixed => LayoutUnit::Mixed(units),
        ConstantBuffer => LayoutUnit::ConstantBuffer(units),
        ShaderResource => LayoutUnit::ShaderResource(units),
        UnorderedAccess => LayoutUnit::UnorderedAccess(units),
        VaryingInput => LayoutUnit::VaryingInput(units),
        VaryingOutput => LayoutUnit::VaryingOutput(units),
        SamplerState => LayoutUnit::SamplerState(units),
        Uniform => LayoutUnit::Uniform(units),
        DescriptorTableSlot => LayoutUnit::DescriptorTableSlot(units),
        SpecializationConstant => LayoutUnit::SpecializationConstant(units),
        PushConstantBuffer => LayoutUnit::PushConstantBuffer(units),
        RegisterSpace => LayoutUnit::RegisterSpace(units),
        Generic => LayoutUnit::Generic(units),
        RayPayload => LayoutUnit::RayPayload(units),
        HitAttributes => LayoutUnit::HitAttributes(units),
        CallablePayload => LayoutUnit::CallablePayload(units),
        ShaderRecord => LayoutUnit::ShaderRecord(units),
        _ => LayoutUnit::Other { category, units },
    }
}

#[derive(Clone, Copy, Debug)]
enum BindlessKind {
    SampledImage,
}

fn vkmutable_binding_index(kind: BindlessKind) -> u32 {
    match kind {
        BindlessKind::SampledImage => 2,
    }
}

fn blob_to_words(blob: &slang::Blob) -> Result<Vec<u32>> {
    let bytes = blob.as_slice();
    if bytes.len() % 4 != 0 {
        return Err(anyhow!("shader bytecode size is not 4-byte aligned"));
    }
    if bytes.len() < 4 {
        return Err(anyhow!("shader bytecode is empty"));
    }
    let mut words = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        words.push(word);
    }
    if words.first().copied() != Some(0x07230203) {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return Err(anyhow!(
                "shader bytecode is not SPIR-V; first bytes: {:?}",
                &text[..text.len().min(64)]
            ));
        }
        return Err(anyhow!("shader bytecode is not SPIR-V"));
    }
    Ok(words)
}
