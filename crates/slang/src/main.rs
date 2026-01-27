use std::env;

use anyhow::{Result, anyhow, bail};
use slang::SlangCompilerBuilder;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!(
            "Usage: {} <shader.slang> [--json-only] [-I <search_path>]...",
            args[0]
        );
        bail!("missing shader file argument");
    }

    let mut shader_path: Option<String> = None;
    let mut json_only = false;

    // Collect search paths from -I flags
    let mut search_paths = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-I" => {
                if i + 1 >= args.len() {
                    bail!("-I requires a path argument");
                }
                search_paths.push(args[i + 1].clone());
                i += 2;
            }
            "--json-only" => {
                json_only = true;
                i += 1;
            }
            _ => {
                if shader_path.is_none() {
                    shader_path = Some(args[i].clone());
                    i += 1;
                } else {
                    bail!("unknown argument: {}", args[i]);
                }
            }
        }
    }

    let shader_path = shader_path.ok_or_else(|| anyhow!("missing shader file argument"))?;

    // Build the compiler
    let mut builder = SlangCompilerBuilder::new()?;
    for path in &search_paths {
        builder = builder.search_path(path);
    }
    let mut compiler = builder.build()?;

    if !json_only {
        println!(
            "Compiler options hash: {:02x?}",
            &compiler.options_hash().0.as_bytes()[..8]
        );
    }

    // Load the module (mutable borrow)
    let module = compiler.load_module(shader_path)?;
    let module_id = module.id().clone();
    let name = module.name().to_owned();
    let file_path = module.file_path().to_owned();
    let content_hash = module.content_hash();
    let entrypoints: Vec<_> = module.entrypoints().to_vec();
    // mutable borrow ends here

    if !json_only {
        println!("\nLoaded module: {}", name);
        println!("  file: {}", file_path);
        println!("  id: {}", module_id);
        println!("  content hash: {:02x?}", &content_hash.as_bytes()[..8]);
    }

    // List entrypoints
    if !json_only {
        println!("\nEntrypoints:");
        for ep in &entrypoints {
            println!(
                "  {:?} {} (module: {})",
                ep.stage(),
                ep.name(),
                ep.module_id()
            );
        }
    }

    // Link all entrypoints
    let program = compiler.linker().add_all_entrypoints(&module_id)?.link()?;

    if !json_only {
        println!("\nLinked program:");
        println!("  key: {:02x?}", &program.key().0.as_bytes()[..8]);
        println!("  entrypoints: {}", program.entrypoints().len());
    }

    if !json_only {
        for ep in program.entrypoints() {
            let code = program.code(ep).unwrap();
            println!(
                "    {:?} {} - {} bytes ({} words)",
                ep.stage(),
                ep.name(),
                code.len_bytes(),
                code.len_words()
            );
        }
    }

    if !json_only {
        // Test select_graphics
        let pipeline = program.select_graphics()?;
        println!("\nGraphics pipeline:");
        for ep in pipeline.entrypoints() {
            if let Some(code) = pipeline.code(ep.stage()) {
                println!(
                    "  {:?} {} - {} bytes",
                    ep.stage(),
                    ep.name(),
                    code.len_bytes()
                );
            }
        }
    }

    // Print layout as JSON
    let layout_json = serde_json::to_string_pretty(program.layout())?;
    if !json_only {
        println!("\nLayout JSON:");
    }
    println!("{}", layout_json);

    Ok(())
}
