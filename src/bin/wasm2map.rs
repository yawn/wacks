//! CLI wrapper for [`wacks::sourcemap_gen::generate`].
//!
//! Usage: `wasm2map <input.wasm> <output.wasm.map>`

use std::{env, fs, process};

use anyhow::{Context, Result, anyhow};

fn main() {
    let run = || -> Result<()> {
        let args: Vec<String> = env::args().collect();
        if args.len() != 3 {
            return Err(anyhow!("usage: wasm2map <input.wasm> <output.wasm.map>"));
        }

        let data = fs::read(&args[1]).context(format!("opening {}", &args[1]))?;
        let (map, num_mappings) = wacks::sourcemap_gen::generate(&data)?;

        let sources_len = map["sources"].as_array().map(|a| a.len()).unwrap_or(0);
        fs::write(&args[2], serde_json::to_string(&map)?).context("writing output")?;

        eprintln!(
            "wasm2map: {sources_len} sources, {num_mappings} mappings → {}",
            &args[2]
        );

        Ok(())
    };

    if let Err(err) = run() {
        eprintln!("wasm2map: {err:#}");
        process::exit(1);
    }
}
