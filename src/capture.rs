//! JS-side `Error.stack` capture and name section backfill via `wasm-bindgen`.

use wasm_bindgen::prelude::*;

use crate::Frame;

cfg_if::cfg_if! {
    if #[cfg(feature = "name-section")] {
        use std::cell::OnceCell;

        use crate::demangle::demangle_symbol;
        use crate::format::is_in_app;
        use crate::namesec::NameSection;

        #[wasm_bindgen(inline_js = r#"
export function __wacks_capture_stack() { return new Error().stack || ''; }

export function __wacks_get_name_section_bytes(module) {
    try {
        const sections = WebAssembly.Module.customSections(module, "name");
        if (sections.length === 0) return new Uint8Array();
        return new Uint8Array(sections[0]);
    } catch (e) {
        return new Uint8Array();
    }
}
"#)]
        extern "C" {
            #[wasm_bindgen(js_name = "__wacks_capture_stack")]
            fn capture_stack() -> String;
            #[wasm_bindgen(js_name = "__wacks_get_name_section_bytes")]
            fn get_name_section_bytes(module: &JsValue) -> Vec<u8>;
        }

        thread_local! {
            static NAME_MAP: OnceCell<NameSection> = const { OnceCell::new() };
        }

        fn backfill_names(frames: &mut [Frame]) {
            NAME_MAP.with(|cell| {
                let ns = cell.get_or_init(|| {
                    let bytes = get_name_section_bytes(&wasm_bindgen::module());
                    NameSection::new(&bytes)
                });

                for frame in frames.iter_mut() {
                    if frame.function.is_some() || frame.wasm_function_index.is_none() {
                        continue;
                    }
                    let idx = frame.wasm_function_index.unwrap();
                    if let Some(raw_name) = ns.get(&idx) {
                        let demangled = demangle_symbol(raw_name);
                        frame.in_app = is_in_app(&demangled);
                        frame.raw_function = Some(raw_name.clone());
                        frame.function = Some(demangled.into_owned());
                    }
                }
            });
        }
    } else {
        #[wasm_bindgen(inline_js = r#"
export function __wacks_capture_stack() { return new Error().stack || ''; }
"#)]
        extern "C" {
            #[wasm_bindgen(js_name = "__wacks_capture_stack")]
            fn capture_stack() -> String;
        }
    }
}

/// Capture the current JS call stack and parse it into structured frames.
///
/// Creates a `new Error()` on the JS side, reads its `.stack` property,
/// and parses the result. With the `name-section` feature (enabled by
/// default), frames where the browser omitted function names are resolved
/// from the WASM name section automatically.
pub fn capture() -> Vec<Frame> {
    let stack = capture_stack();
    let mut frames = Frame::parse(&stack);
    #[cfg(feature = "name-section")]
    backfill_names(&mut frames);
    frames
}
