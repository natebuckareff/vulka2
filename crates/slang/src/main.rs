use std::io::{self, Write};
use std::{env, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use slang::{SlangCompilerBuilder, SlangShaderStage};

#[derive(Debug)]
struct Cli {
    args: Vec<Arg>,
    inputs: Vec<PathBuf>,
}

#[derive(Debug)]
enum Arg {
    Include(PathBuf),
    AllEntrypoints,
    AllStageEntrypoints(SlangShaderStage),
    Entrypoint(SlangShaderStage, String),
    Output(OutputArg),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OutputArg {
    Debug,
    Layout,
    Spirv,
}

fn main() -> Result<()> {
    let cli = parse_args()?;
    let mut builder = SlangCompilerBuilder::new()?.optimization(slang::OptimizationLevel::None);
    let mut output = None;

    for input in &cli.inputs {
        let Some(base_path) = input.parent() else {
            continue;
        };
        builder = builder.search_path(base_path);
    }

    for arg in &cli.args {
        match arg {
            Arg::Include(path_buf) => {
                builder = builder.search_path(path_buf);
            }
            Arg::Output(arg) => {
                if output.is_some() {
                    return Err(anyhow!("multiple output arguments provided"));
                }
                output = Some(arg.clone());
            }
            _ => {}
        }
    }

    let mut compiler = builder.build()?;
    let mut modules = vec![];

    for input in cli.inputs {
        let module = compiler.load_module(input)?;
        modules.push(module.id().clone());
    }

    let Some(first_id) = modules.first().cloned() else {
        return Err(anyhow!("no modules to link"));
    };

    let first_module = compiler.module(&first_id).context("module not found")?;
    let mut target = None;

    let mut linker = compiler.linker();
    for id in modules {
        linker = linker.add_module(&id)?;
    }

    for arg in cli.args {
        match arg {
            Arg::AllEntrypoints => {
                linker = linker.add_all_entrypoints(&first_id)?;
            }
            Arg::Entrypoint(stage, name) => {
                let entrypoint = first_module.entrypoint(stage, &name)?;
                linker = linker.add_entrypoint(entrypoint.clone())?;
                target = Some(entrypoint);
            }
            Arg::AllStageEntrypoints(stage) => {
                linker = linker.add_stage(&first_id, stage)?;
            }
            _ => {}
        }
    }

    let program = linker.link()?;

    match output.unwrap_or(OutputArg::Debug) {
        OutputArg::Debug => {
            let json = serde_json::to_string_pretty(&program.layout())?;
            println!("{}", json);
        }
        OutputArg::Layout => {
            todo!();
        }
        OutputArg::Spirv => {
            let Some(target) = target else {
                return Err(anyhow!("no target entrypoint"));
            };
            let mut out = io::stdout().lock();

            let code = program.code(&target).context("code not linked")?;
            let words = code.as_ref();
            for &w in words {
                out.write_all(&w.to_le_bytes())?;
            }
        }
    }

    Ok(())
}

fn parse_args() -> Result<Cli> {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let mut args = vec![];
    let mut inputs = vec![];
    let mut option: Option<String> = None;

    for raw_arg in raw_args {
        if let Some(option) = option.take() {
            if raw_arg.starts_with("-") {
                return Err(anyhow!("expected argument for option: {}", option));
            }

            match option.as_str() {
                "-I" => {
                    let path = PathBuf::from(raw_arg);
                    args.push(Arg::Include(path));
                    continue;
                }
                "-a" => {
                    args.push(Arg::AllEntrypoints);
                    continue;
                }
                "-s" => {
                    let stage = match raw_arg.as_str() {
                        "vertex" => SlangShaderStage::Vertex,
                        "fragment" => SlangShaderStage::Fragment,
                        "compute" => SlangShaderStage::Compute,
                        _ => return Err(anyhow!("unknown stage: {}", raw_arg)),
                    };
                    args.push(Arg::AllStageEntrypoints(stage));
                    continue;
                }
                "-v" => {
                    let entrypoint = raw_arg;
                    args.push(Arg::Entrypoint(SlangShaderStage::Vertex, entrypoint));
                    continue;
                }
                "-f" => {
                    let entrypoint = raw_arg;
                    args.push(Arg::Entrypoint(SlangShaderStage::Fragment, entrypoint));
                    continue;
                }
                "-c" => {
                    let entrypoint = raw_arg;
                    args.push(Arg::Entrypoint(SlangShaderStage::Compute, entrypoint));
                    continue;
                }
                "-o" => {
                    let arg = match raw_arg.as_str() {
                        "debug" => OutputArg::Debug,
                        "layout" => OutputArg::Layout,
                        "spirv" => OutputArg::Spirv,
                        _ => return Err(anyhow!("unknown output: {}", raw_arg)),
                    };
                    args.push(Arg::Output(arg));
                    continue;
                }
                _ => return Err(anyhow!("unknown option: {}", option)),
            }
        }

        if raw_arg.starts_with("-") {
            option = Some(raw_arg);
        } else {
            let path = PathBuf::from(raw_arg);
            inputs.push(path);
        }
    }

    if inputs.is_empty() {
        return Err(anyhow!("no input files"));
    };

    Ok(Cli { args, inputs })
}
