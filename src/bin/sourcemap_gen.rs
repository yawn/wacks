//! CLI wrapper for [`wacks::sourcemap_gen::generate`].

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

/// Generate a source map from a WASM binary.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Input WASM file
    input: PathBuf,
    /// Output source map file
    output: PathBuf,
}

fn main() -> Result<()> {
    let cli = Args::parse();

    let data = fs::read(&cli.input).context(format!("opening {}", cli.input.display()))?;
    let map = wacks::sourcemap_gen::generate(&data)?;

    let sources_len = map.json["sources"].as_array().map(|a| a.len()).unwrap_or(0);

    fs::write(&cli.output, serde_json::to_string(&map.json)?).context("writing output")?;

    eprintln!(
        "sourcemap-gen: {sources_len} sources, {} mappings → {}",
        map.num_mappings,
        cli.output.display(),
    );

    Ok(())
}
