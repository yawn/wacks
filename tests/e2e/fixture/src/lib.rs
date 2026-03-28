//! E2E test fixture — triggers a known panic chain for cross-browser assertions.

use std::panic::set_hook;

use serde_json::to_string;
use wacks::capture;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function store_frames(json) {
    window.__captured_frames = JSON.parse(json);
}
")]
extern "C" {
    fn store_frames(json: &str);
}

#[wasm_bindgen(start)]
pub fn init() {
    set_hook(Box::new(|_info| {
        let frames = capture();

        if let Ok(json) = to_string(&frames) {
            store_frames(&json);
        }
    }));
}

#[inline(never)]
fn level_1() {
    level_2();
}

#[inline(never)]
fn level_2() {
    level_3();
}

#[inline(never)]
fn level_3() {
    panic!("test panic");
}

#[wasm_bindgen]
pub fn trigger_panic() {
    level_1();
}
