//! Integration tests for the `sourcemap-gen` binary.
//!
//! Requires the e2e fixture: `mise run build:e2e`
//!
//! Run with: `cargo nextest run --features sourcemap-gen`

#![cfg(feature = "sourcemap-gen")]

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/e2e/static/pkg")
}

fn fixture_wasm() -> Option<PathBuf> {
    let wasm = fixture_dir().join("wacks_test_fixture_bg.wasm");
    if wasm.exists() {
        Some(wasm)
    } else {
        eprintln!("skip: run `mise run build:e2e` first");
        None
    }
}

fn run_sourcemap_gen(wasm: &Path, out: &Path) {
    let result = Command::new(env!("CARGO_BIN_EXE_sourcemap-gen"))
        .args([wasm.to_str().unwrap(), out.to_str().unwrap()])
        .output()
        .expect("failed to run sourcemap-gen");

    assert!(
        result.status.success(),
        "sourcemap-gen failed: {}",
        String::from_utf8_lossy(&result.stderr),
    );
}

#[test]
fn deterministic_output() {
    let Some(wasm) = fixture_wasm() else { return };
    let expected = fixture_dir().join("wacks_test_fixture_bg.wasm.map");
    let out = std::env::temp_dir().join("wacks-sourcemap-gen-deterministic.map");

    run_sourcemap_gen(&wasm, &out);

    let generated = std::fs::read_to_string(&out).unwrap();
    let expected = std::fs::read_to_string(&expected).unwrap();
    assert_eq!(generated, expected);
}

#[test]
fn sources_contain_no_absolute_paths_or_pii() {
    let Some(wasm) = fixture_wasm() else { return };
    let out = std::env::temp_dir().join("wacks-sourcemap-gen-relative.map");

    run_sourcemap_gen(&wasm, &out);

    let json = std::fs::read_to_string(&out).unwrap();
    let map: serde_json::Value = serde_json::from_str(&json).unwrap();

    for source in map["sources"].as_array().unwrap() {
        let path = source.as_str().unwrap();
        assert!(!path.starts_with('/'), "absolute path: {path}");
    }

    let home = std::env::var("HOME").unwrap();
    assert!(
        !json.contains(&home),
        "source map leaks home directory: {home}",
    );
}
