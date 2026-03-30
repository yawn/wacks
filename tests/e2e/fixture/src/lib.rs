//! E2E test fixture — triggers a known panic chain for cross-browser assertions.

use serde_json::to_string;
use wacks::Builder;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function store_captured_frames(json) {
    window.__captured_frames = JSON.parse(json);
}
")]
extern "C" {
    fn store_captured_frames(json: &str);
}

#[wasm_bindgen]
pub fn install_hook() {
    Builder::new().install(|frames, _info| {
        if let Ok(json) = to_string(&frames) {
            store_captured_frames(&json);
        }
    });
}

#[wasm_bindgen]
pub fn install_hook_with_sourcemap(filename: &str) {
    Builder::new()
        .sourcemap(filename)
        .install(|frames, _info| {
            if let Ok(json) = to_string(&frames) {
                store_captured_frames(&json);
            }
        });
}

mod pipeline {
    #[inline(never)]
    pub fn ingest(data: &[u8]) {
        validate(data);
    }

    #[inline(never)]
    fn validate(data: &[u8]) {
        decode(data);
    }

    #[inline(never)]
    fn decode(data: &[u8]) {
        transform(data);
    }

    #[inline(never)]
    fn transform(data: &[u8]) {
        normalize(data);
    }

    #[inline(never)]
    fn normalize(data: &[u8]) {
        enrich(data);
    }

    #[inline(never)]
    fn enrich(data: &[u8]) {
        compress(data);
    }

    #[inline(never)]
    fn compress(data: &[u8]) {
        encrypt(data);
    }

    #[inline(never)]
    fn encrypt(data: &[u8]) {
        flush(data);
    }

    #[inline(never)]
    fn flush(data: &[u8]) {
        commit(data);
    }

    #[inline(never)]
    fn commit(_data: &[u8]) {
        panic!("pipeline commit failed");
    }
}

#[wasm_bindgen]
pub fn trigger_panic() {
    pipeline::ingest(b"test payload");
}
