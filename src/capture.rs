//! JS-side `Error.stack` capture via `wasm-bindgen`.

use wasm_bindgen::prelude::*;

use crate::Frame;

#[wasm_bindgen(inline_js = "export function capture_stack() { return new Error().stack || ''; }")]
extern "C" {
    fn capture_stack() -> String;
}

/// Capture the current JS call stack and parse it into structured frames.
///
/// Creates a `new Error()` on the JS side, reads its `.stack` property,
/// and parses the result. Call this inside a panic hook to get the full
/// WASM call stack at the panic site.
pub fn capture() -> Vec<Frame> {
    let stack = capture_stack();
    Frame::parse(&stack)
}
