use std::path::PathBuf;

use anyhow::Result;
use slang::SlangCompilerBuilder;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[test]
fn official_slang_pb_example() -> Result<()> {
    let mut compiler = SlangCompilerBuilder::new()?
        .search_path(fixtures_dir())
        .build()?;

    let module = compiler.load_module("shader.slang")?;
    let module_id = module.id().clone();

    let program = compiler.linker().add_all_entrypoints(&module_id)?.link()?;

    // let pipeline_layout = slang::slang_layout_direct_from_shader_layout(program.layout())?;
    let pipeline_layout = program.layout();
    let _layout_json = serde_json::to_string_pretty(&pipeline_layout)?;
    // insta::assert_snapshot!(layout_json);

    Ok(())
}
