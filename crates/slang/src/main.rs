use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use shader_slang as slang;

fn main() -> Result<()> {
    let (module_name, search_path) = resolve_inputs();

    let global_session =
        slang::GlobalSession::new().context("failed to create slang global session")?;

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
        .optimization(slang::OptimizationLevel::High)
        .matrix_layout_row(true)
        .vulkan_use_entry_point_name(true)
        .capability(physical_storage)
        .capability(descriptor_indexing);
    if let Some(bindless_space_index) = bindless_space_index() {
        println!("bindless_space_index: {bindless_space_index}");
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
        std::ffi::CString::new(search_path.as_os_str().to_string_lossy().as_ref())?;
    let search_paths = [search_path_cstr.as_ptr()];

    let session_desc = slang::SessionDesc::default()
        .targets(&targets)
        .search_paths(&search_paths)
        .options(&compiler_options);

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
    components.push(module.clone().into());
    for entry in &entry_points {
        components.push(entry.clone().into());
    }

    let linked_program = session
        .create_composite_component_type(&components)?
        .link()?;

    let reflection = linked_program.layout(0)?;

    if env_flag("SLANG_DUMP_SPIRV") {
        dump_spirv(&linked_program, reflection)?;
    }

    println!("entry_points: {}", reflection.entry_point_count());
    for (index, entry) in reflection.entry_points().enumerate() {
        println!(
            "  entry[{index}]: name={} stage={:?}",
            entry.name().unwrap_or("<unnamed>"),
            entry.stage()
        );
    }

    println!("parameters: {}", reflection.parameter_count());
    for (index, param) in reflection.parameters().enumerate() {
        let name = param.name().unwrap_or("<unnamed>");
        println!(
            "  param[{index}]: name={name} category={:?} binding={}:{} stage={:?}",
            param.category(),
            param.binding_space(),
            param.binding_index(),
            param.stage()
        );
    }

    println!(
        "global_constant_buffer: binding={} size={}",
        reflection.global_constant_buffer_binding(),
        reflection.global_constant_buffer_size()
    );

    if let Some(global_layout) = reflection.global_params_type_layout() {
        dump_descriptor_sets(global_layout, "global");
    }

    for (index, entry) in reflection.entry_points().enumerate() {
        let name = entry.name().unwrap_or("<unnamed>");
        let stage = entry.stage();
        let Some(entry_layout) = entry.type_layout() else {
            continue;
        };
        println!("entry_layout[{index}]: name={name} stage={stage:?}");
        dump_descriptor_sets(entry_layout, "entry");
    }

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

fn bindless_space_index() -> Option<i32> {
    std::env::var("SLANG_BINDLESS_SPACE_INDEX")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
}

fn env_flag(name: &str) -> bool {
    matches!(std::env::var(name).as_deref(), Ok("1") | Ok("true") | Ok("TRUE"))
}

fn dump_spirv(
    program: &slang::ComponentType,
    reflection: &slang::reflection::Shader,
) -> Result<()> {
    let out_dir = std::env::var("SLANG_SPIRV_OUT_DIR").unwrap_or_else(|_| "target/slang".into());
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {out_dir}"))?;

    for index in 0..reflection.entry_point_count() {
        let name = reflection
            .entry_point_by_index(index)
            .and_then(|entry| entry.name())
            .unwrap_or("<unknown>");
        let blob = program
            .entry_point_code(index as i64, 0)
            .with_context(|| format!("failed to get SPIR-V for entry {name}"))?;
        let path = format!("{out_dir}/{name}.spv");
        std::fs::write(&path, blob.as_slice())
            .with_context(|| format!("failed to write {path}"))?;
        println!("wrote_spirv: {path}");
    }

    Ok(())
}

fn dump_descriptor_sets(layout: &slang::reflection::TypeLayout, label: &str) {
    let set_count = layout.descriptor_set_count();
    println!("{label}_descriptor_sets: {set_count}");
    for set_index in 0..set_count {
        let range_count = layout.descriptor_set_descriptor_range_count(set_index);
        let space_offset = layout.descriptor_set_space_offset(set_index);
        println!(
            "  set[{set_index}]: space_offset={space_offset} descriptor_ranges={range_count}"
        );
        for range_index in 0..range_count {
            let range_type = layout.descriptor_set_descriptor_range_type(set_index, range_index);
            let range_category =
                layout.descriptor_set_descriptor_range_category(set_index, range_index);
            let descriptor_count =
                layout.descriptor_set_descriptor_range_descriptor_count(set_index, range_index);
            let range_offset =
                layout.descriptor_set_descriptor_range_index_offset(set_index, range_index);
            println!(
                "    range[{range_index}]: type={range_type:?} category={range_category:?} count={descriptor_count} range_offset={range_offset}"
            );
        }
    }
}
