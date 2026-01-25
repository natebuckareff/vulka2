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
        &compiler.options_hash().0.as_bytes()[..8]
    );

    // Load the module (mutable borrow)
    let module = compiler.load_module(shader_path)?;
    let module_id = module.id().clone();
    let name = module.name().to_owned();
    let file_path = module.file_path().to_owned();
    let content_hash = module.content_hash();
    let entrypoints: Vec<_> = module.entrypoints().to_vec();
    // mutable borrow ends here

    println!("\nLoaded module: {}", name);
    println!("  file: {}", file_path);
    println!("  id: {}", module_id);
    println!("  content hash: {:02x?}", &content_hash.as_bytes()[..8]);

    // List entrypoints
    println!("\nEntrypoints:");
    for ep in &entrypoints {
        println!(
            "  {:?} {} (module: {})",
            ep.stage(),
            ep.name(),
            ep.module_id()
        );
    }

    // Link all entrypoints
    let program = compiler.linker().add_all_entrypoints(&module_id)?.link()?;

    println!("\nLinked program:");
    println!("  key: {:02x?}", &program.key().0.as_bytes()[..8]);
    println!("  entrypoints: {}", program.entrypoints().len());

    for ep in program.entrypoints() {
        println!("    {:?} {}", ep.stage(), ep.name());
    }

    Ok(())
}
