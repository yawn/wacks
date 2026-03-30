//! Structured WASM panic stack traces for browsers.
//!
//! `wacks` captures `Error.stack` from inside a WASM panic hook,
//! parses it into structured [`Frame`]s across Chrome, Firefox, and Safari,
//! and demangles Rust symbols — giving you data suitable for error
//! reporting services like PostHog, Sentry, or Datadog.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use wacks::{capture, Frame};
//!
//! std::panic::set_hook(Box::new(|info| {
//!     let frames: Vec<Frame> = capture();
//!     // forward to your error reporter …
//! }));
//! ```
//!
//! # Parsing existing stack strings
//!
//! [`Frame::parse`] works on any target (not just WASM), so you can
//! use it server-side to process stack traces sent from browsers.
//!
//! ```
//! use wacks::Frame;
//!
//! let stack = "Error\n    at my_crate::handler::h86f485cc (wasm://wasm/abc:wasm-function[58]:0x9065)\n";
//! let frames = Frame::parse(stack);
//! assert_eq!(frames[0].function.as_deref(), Some("my_crate::handler"));
//! ```

mod demangle;
mod format;
mod frame;
mod parse;

pub use frame::Frame;

#[cfg(feature = "name-section")]
mod namesec;

#[cfg(feature = "source-map")]
mod sourcemap;

#[cfg(feature = "source-map-gen")]
pub mod sourcemap_gen;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod capture;

        use std::panic::{set_hook, PanicHookInfo};

        pub use capture::capture;

        #[cfg(feature = "source-map")]
        pub use capture::init_source_map;

        #[cfg(feature = "source-map-proxy")]
        pub use capture::init_source_map_proxy;

        /// Install a panic hook that passes structured frames to `callback`.
        ///
        /// This is a convenience wrapper around [`std::panic::set_hook`] +
        /// [`capture`]. For more control, call [`capture`] directly inside
        /// your own hook.
        pub fn set_panic_hook(callback: fn(Vec<Frame>, &PanicHookInfo<'_>)) {
            set_hook(Box::new(move |info| {
                let frames = capture::capture();
                callback(frames, info);
            }));
        }
    }
}
