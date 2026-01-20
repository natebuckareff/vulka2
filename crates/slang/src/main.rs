use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use shader_slang as slang;
use slang::Downcast;

fn main() -> Result<()> {
    let (module_name, search_path) = resolve_inputs();

    let global_session =
        slang::GlobalSession::new().context("failed to create slang global session")?;

    let compiler_options = slang::CompilerOptions::default()
        .optimization(slang::OptimizationLevel::High)
        .matrix_layout_row(true)
        .vulkan_use_entry_point_name(true);

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
        std::ffi::CString::new(search_path.as_os_str().to_string_lossy().as_ref())?;
    let search_paths = [search_path_cstr.as_ptr()];

    let session_desc = slang::SessionDesc::default()
        .targets(&targets)
        .search_paths(&search_paths);

    let session = global_session
        .create_session(&session_desc)
        .context("failed to create slang session")?;

    let module = session
        .load_module(&module_name)
        .map_err(|err| anyhow!("failed to load module {module_name}: {err:?}"))?;

    println!("module: {}", module.name());
    println!("file_path: {}", module.file_path());
    println!("dependencies: {}", module.dependency_file_count());
    for (index, dep) in module.dependency_file_paths().enumerate() {
        println!("  dep[{index}]: {dep}");
    }

    let entry_points: Vec<_> = module.entry_points().collect();
    if entry_points.is_empty() {
        return Err(anyhow!("module {module_name} has no entry points"));
    }

    let mut components = Vec::with_capacity(1 + entry_points.len());
    components.push(module.downcast().clone());
    for entry in &entry_points {
        components.push(entry.downcast().clone());
    }

    let linked_program = session
        .create_composite_component_type(&components)?
        .link()?;

    let reflection = linked_program.layout(0)?;

    println!("entry_points: {}", reflection.entry_point_count());
    for (index, entry) in reflection.entry_points().enumerate() {
        println!(
            "  entry[{index}]: name={} stage={:?}",
            entry.name(),
            entry.stage()
        );
    }

    println!("parameters: {}", reflection.parameter_count());
    for (index, param) in reflection.parameters().enumerate() {
        println!("  param[{index}]:");
        dump_variable_layout(param, 4);
    }

    let global_layout = reflection.global_params_type_layout();
    dump_descriptor_sets(global_layout);
    dump_binding_ranges(global_layout);

    Ok(())
}

fn resolve_inputs() -> (String, PathBuf) {
    let mut args = std::env::args().skip(1);
    let module_arg = args.next();
    let search_arg = args.next();

    match (module_arg, search_arg) {
        (None, _) => {
            let module_path = PathBuf::from("crates/renderer/shaders/cube.slang");
            let search_path = module_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            println!(
                "no inputs provided; defaulting to {} (search path {})",
                module_path.display(),
                search_path.display()
            );
            (
                module_path
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("cube.slang"))
                    .to_string_lossy()
                    .to_string(),
                search_path,
            )
        }
        (Some(module), search_path) => {
            let module_path = PathBuf::from(&module);
            let module_name = module_path
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(&module))
                .to_string_lossy()
                .to_string();
            let search_path = search_path
                .map(PathBuf::from)
                .or_else(|| module_path.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."));
            (module_name, search_path)
        }
    }
}

fn dump_variable_layout(layout: &slang::reflection::VariableLayout, indent: usize) {
    let indent = " ".repeat(indent);
    let name = layout.name().unwrap_or("<unnamed>");
    let category = layout.category();
    let binding_index = layout.binding_index();
    let binding_space = layout.binding_space();
    let stage = layout.stage();
    let ty = layout.ty();
    let ty_name = ty.map(|t| t.name()).unwrap_or("<anonymous>");
    let ty_kind = ty.map(|t| t.kind());
    let array_count = ty.map(|t| t.total_array_element_count()).unwrap_or(0);

    println!(
        "{indent}name={name} category={category:?} binding={binding_space}:{binding_index} stage={stage:?}"
    );
    println!(
        "{indent}type=name={ty_name} kind={:?} array_total={array_count}",
        ty_kind
    );

    if let Some(ty) = ty {
        if ty_kind == Some(slang::TypeKind::Resource) {
            println!(
                "{indent}resource_shape={:?} resource_access={:?}",
                ty.resource_shape(),
                ty.resource_access()
            );
        }
    }
}

fn dump_descriptor_sets(layout: &slang::reflection::TypeLayout) {
    let set_count = layout.descriptor_set_count();
    println!("descriptor_sets: {set_count}");
    for set_index in 0..set_count {
        let range_count = layout.descriptor_set_descriptor_range_count(set_index);
        println!("  set[{set_index}]: descriptor_ranges={range_count}");
        for range_index in 0..range_count {
            let range_type = layout.descriptor_set_descriptor_range_type(set_index, range_index);
            let range_category =
                layout.descriptor_set_descriptor_range_category(set_index, range_index);
            let descriptor_count =
                layout.descriptor_set_descriptor_range_descriptor_count(set_index, range_index);
            println!(
                "    range[{range_index}]: type={range_type:?} category={range_category:?} count={descriptor_count}"
            );
        }
    }
}

fn dump_binding_ranges(layout: &slang::reflection::TypeLayout) {
    let binding_range_count = layout.binding_range_count();
    println!("binding_ranges: {binding_range_count}");
    for range_index in 0..binding_range_count {
        let range_type = layout.binding_range_type(range_index);
        let binding_count = layout.binding_range_binding_count(range_index);
        let set_index = layout.binding_range_descriptor_set_index(range_index);
        let first_descriptor_range = layout.binding_range_first_descriptor_range_index(range_index);
        let descriptor_range_count = layout.binding_range_descriptor_range_count(range_index);
        println!(
            "  range[{range_index}]: type={range_type:?} set={set_index} bindings={binding_count} descriptor_ranges={first_descriptor_range}+{descriptor_range_count}"
        );
    }
}
