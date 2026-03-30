//! JS-side `Error.stack` capture via `wasm-bindgen`.

use std::panic::{self, PanicHookInfo};

use wasm_bindgen::prelude::*;

use crate::Frame;
use crate::namesec::BackfillNames;
use crate::sourcemap::{RewriteForSourcemap, SOURCEMAP_FILENAME};

#[wasm_bindgen(inline_js = r#"
export function __wacks_capture_stack() { return new Error().stack || ''; }
"#)]
extern "C" {
    #[wasm_bindgen(js_name = "__wacks_capture_stack")]
    fn capture_stack() -> String;
}

/// Configures and installs a WASM panic hook.
///
/// ```rust,ignore
/// wacks::Builder::new()
///     .sourcemap("app.wasm.js")
///     .install(|frames, info| {
///         // send to PostHog, Sentry, etc.
///     });
/// ```
pub struct Builder {
    sourcemap_filename: Option<String>,
}

impl Builder {
    /// Install the panic hook with the given callback.
    ///
    /// Consumes the builder, configures source map rewriting (if enabled),
    /// and installs a [`std::panic::set_hook`] that captures structured
    /// frames and passes them to `callback`.
    pub fn install(self, callback: fn(Vec<Frame>, &PanicHookInfo<'_>)) {
        if let Some(filename) = self.sourcemap_filename {
            SOURCEMAP_FILENAME.with(|cell| {
                cell.get_or_init(|| filename);
            });
        }

        panic::set_hook(Box::new(move |info| {
            let stack = capture_stack();

            let mut frames = Frame::parse(&stack);

            frames.backfill_names();
            frames.rewrite_for_sourcemap();

            callback(frames, info);
        }));
    }

    pub fn new() -> Self {
        Self {
            sourcemap_filename: None,
        }
    }

    /// Enable source map rewriting for server-side resolution.
    ///
    /// WASM frames will have their location set to `<filename>:1:<byte_offset>`,
    /// allowing JS-only source map consumers (PostHog, Datadog, etc.) to resolve
    /// them against an uploaded source map.
    ///
    /// `filename` should match the artifact name used when uploading the source
    /// map (e.g. `"app.wasm.js"`).
    pub fn sourcemap(mut self, filename: &str) -> Self {
        self.sourcemap_filename = Some(filename.to_string());
        self
    }
}
