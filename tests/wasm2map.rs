//! Integration tests for the `wasm2map` binary.
//!
//! Requires the e2e fixture: `mise run build:e2e`
//!
//! Run with: `cargo nextest run --features source-map-gen`

#![cfg(feature = "source-map-gen")]

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

fn run_wasm2map(wasm: &Path, out: &Path) {
    let result = Command::new(env!("CARGO_BIN_EXE_wasm2map"))
        .args([wasm.to_str().unwrap(), out.to_str().unwrap()])
        .output()
        .expect("failed to run wasm2map");

    assert!(
        result.status.success(),
        "wasm2map failed: {}",
        String::from_utf8_lossy(&result.stderr),
    );
}

#[test]
fn deterministic_output() {
    let Some(wasm) = fixture_wasm() else { return };
    let expected = fixture_dir().join("wacks_test_fixture_bg.wasm.map");
    let out = std::env::temp_dir().join("wacks-wasm2map-deterministic.map");

    run_wasm2map(&wasm, &out);

    let generated = std::fs::read_to_string(&out).unwrap();
    let expected = std::fs::read_to_string(&expected).unwrap();
    assert_eq!(generated, expected);
}

#[test]
fn sources_contain_no_absolute_paths_or_pii() {
    let Some(wasm) = fixture_wasm() else { return };
    let out = std::env::temp_dir().join("wacks-wasm2map-relative.map");

    run_wasm2map(&wasm, &out);

    let json = std::fs::read_to_string(&out).unwrap();
    let map: serde_json::Value = serde_json::from_str(&json).unwrap();

    for source in map["sources"].as_array().unwrap() {
        let path = source.as_str().unwrap();
        assert!(!path.starts_with('/'), "absolute path: {path}");
    }

    // The full JSON must not contain any home-directory fragment that could
    // leak a username. This catches both `sources` values and any future
    // field that might embed paths.
    let home = std::env::var("HOME").unwrap();
    assert!(
        !json.contains(&home),
        "source map leaks home directory: {home}",
    );
}

#[test]
fn rejects_wrong_arg_count() {
    let result = Command::new(env!("CARGO_BIN_EXE_wasm2map"))
        .output()
        .expect("failed to run wasm2map");

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("usage:"));
}
