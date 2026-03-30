//! CLI wrapper for [`wacks::framemap_gen::generate`].

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

/// Generate a framemap from a WASM binary for WebKit byte offset resolution.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Input WASM file
    input: PathBuf,
    /// Output framemap file
    output: PathBuf,
}

fn main() -> Result<()> {
    let cli = Args::parse();

    let data = fs::read(&cli.input).context(format!("opening {}", cli.input.display()))?;
    let framemap = wacks::framemap_gen::generate(&data)?;

    fs::write(&cli.output, &framemap).context("writing output")?;

    eprintln!(
        "framemap-gen: {} bytes → {}",
        framemap.len(),
        cli.output.display(),
    );

    Ok(())
}
