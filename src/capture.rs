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

cfg_if::cfg_if! {
    if #[cfg(feature = "source-map")] {
        use std::cell::OnceCell as SourceMapCell;

        use crate::sourcemap::SourceMap;

        thread_local! {
            static SOURCE_MAP: SourceMapCell<Option<SourceMap>> = const { SourceMapCell::new() };
        }

        /// Initialize source location resolution from a source map JSON string.
        ///
        /// Parses the [source map v3][spec] and caches the result. Subsequent calls
        /// to [`capture`] will use it to fill `filename`, `lineno`, and `colno` on
        /// WASM frames that have a `wasm_byte_offset`.
        ///
        /// [spec]: https://sourcemaps.info/spec.html
        pub fn init_source_map(json: &str) {
            SOURCE_MAP.with(|cell| {
                cell.get_or_init(|| SourceMap::new(json));
            });
        }

        fn backfill_source_locations(frames: &mut [Frame]) {
            SOURCE_MAP.with(|cell| {
                let Some(Some(sm)) = cell.get() else { return };
                for frame in frames.iter_mut() {
                    let Some(offset) = frame.wasm_byte_offset else { continue };
                    if let Some((file, line, col)) = sm.resolve(offset) {
                        frame.filename = Some(file.to_string());
                        frame.lineno = Some(line);
                        frame.colno = Some(col);
                    }
                }
            });
        }
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "source-map-proxy")] {
        use std::cell::OnceCell as ProxyCell;

        thread_local! {
            static PROXY_FILENAME: ProxyCell<String> = const { ProxyCell::new() };
        }

        /// Configure JS-compatible frame rewriting for server-side source map
        /// resolution.
        ///
        /// WASM frames captured by [`capture`] will have their location set to
        /// `<filename>:1:<byte_offset>`, allowing JS-only source map consumers
        /// (PostHog, Datadog, etc.) to resolve them against an uploaded WASM
        /// source map.
        ///
        /// `filename` should match the artifact name used when uploading the
        /// source map (e.g. `"app.wasm.js"`).
        pub fn init_source_map_proxy(filename: &str) {
            PROXY_FILENAME.with(|cell| {
                cell.get_or_init(|| filename.to_string());
            });
        }

        fn rewrite_for_proxy(frames: &mut [Frame]) {
            PROXY_FILENAME.with(|cell| {
                let Some(filename) = cell.get() else { return };
                for frame in frames.iter_mut() {
                    let Some(offset) = frame.wasm_byte_offset else { continue };
                    frame.filename = Some(filename.clone());
                    frame.lineno = Some(1);
                    frame.colno = u32::try_from(offset).ok();
                }
            });
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
    #[allow(unused_mut)]
    let mut frames = Frame::parse(&stack);
    #[cfg(feature = "name-section")]
    backfill_names(&mut frames);
    #[cfg(feature = "source-map")]
    backfill_source_locations(&mut frames);
    #[cfg(feature = "source-map-proxy")]
    rewrite_for_proxy(&mut frames);
    frames
}
