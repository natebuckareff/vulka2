use std::env;

use anyhow::{Result, anyhow, bail};
use slang::SlangCompilerBuilder;

// Usage:
// slang [options] <shader>
// -I <dir>
// -o debug
// -o layout
// -o spirv

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    Ok(())
}
