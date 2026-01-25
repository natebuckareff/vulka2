use std::env;

use anyhow::{Result, bail};
use slang::SlangCompilerBuilder;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <shader.slang> [-I <search_path>]...", args[0]);
        bail!("missing shader file argument");
    }

    let shader_path = &args[1];

    // Collect search paths from -I flags
    let mut search_paths = Vec::new();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "-I" {
            if i + 1 >= args.len() {
                bail!("-I requires a path argument");
            }
            search_paths.push(args[i + 1].clone());
            i += 2;
        } else {
            bail!("unknown argument: {}", args[i]);
        }
    }

    // Build the compiler
    let mut builder = SlangCompilerBuilder::new()?;
    for path in &search_paths {
        builder = builder.search_path(path);
    }
    let mut compiler = builder.build()?;

    println!(
        "Compiler options hash: {:02x?}",
        &compiler.options_hash().0[..8]
    );

    // Load the module
    let module = compiler.load_module(shader_path)?;
    println!("\nLoaded module: {}", module.name());
    println!("  file: {}", module.file_path());
    println!("  identity: {}", module.unique_identity());
    println!("  content hash: {:02x?}", &module.content_hash()[..8]);

    // List entrypoints
    println!("\nEntrypoints:");
    for ep in module.entrypoints() {
        println!(
            "  {:?} {} (module: {})",
            ep.stage(),
            ep.name(),
            ep.module_identity()
        );
    }

    // Link all entrypoints
    let program = compiler.linker().add_all_entrypoints(&module).link()?;

    println!("\nLinked program:");
    println!("  key: {:02x?}", &program.key().0[..8]);
    println!("  entrypoints: {}", program.entrypoints().len());

    for ep in program.entrypoints() {
        println!("    {:?} {}", ep.stage(), ep.name());
    }

    Ok(())
}
