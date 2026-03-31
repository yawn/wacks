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
//! wacks::Builder::new()
//!     .framemap(include_bytes!("app.framemap"))
//!     .install(|frames, info| {
//!         // forward to your error reporter …
//!     });
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

mod delta;
mod demangle;
mod frame;
mod parse;

pub use frame::Frame;

#[cfg(feature = "framemap-gen")]
pub mod framemap_gen;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod builder;
        mod framemap;
        mod namesec;

        pub use builder::Builder;
    } else if #[cfg(test)] {
        mod framemap;
        mod namesec;
    }
}
