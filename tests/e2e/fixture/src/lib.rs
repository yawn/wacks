//! E2E test fixture — triggers a known panic chain for cross-browser assertions.

use std::panic::set_hook;

use serde_json::to_string;
use wacks::{capture, Frame};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "
export function capture_stack_string() { return new Error().stack || ''; }

export function store_raw_frames(json) {
    window.__raw_frames = JSON.parse(json);
}

export function store_captured_frames(json) {
    window.__captured_frames = JSON.parse(json);
}
")]
extern "C" {
    fn capture_stack_string() -> String;
    fn store_raw_frames(json: &str);
    fn store_captured_frames(json: &str);
}

#[wasm_bindgen(start)]
pub fn init() {
    set_hook(Box::new(|_info| {
        // Raw parse only — no name section backfill
        let raw = Frame::parse(&capture_stack_string());
        if let Ok(json) = to_string(&raw) {
            store_raw_frames(&json);
        }

        // Full capture — parse + name section backfill
        let captured = capture();
        if let Ok(json) = to_string(&captured) {
            store_captured_frames(&json);
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
pub fn setup_source_map(json: &str) {
    wacks::init_source_map(json);
}

#[wasm_bindgen]
pub fn setup_source_map_proxy(filename: &str) {
    wacks::init_source_map_proxy(filename);
}

#[wasm_bindgen]
pub fn trigger_panic() {
    level_1();
}
