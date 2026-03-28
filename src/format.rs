//! Stack format detection and line-by-line parsing for V8 and SpiderMonkey.

use std::borrow::Cow;

use crate::demangle::demangle_symbol;
use crate::Frame;

/// Parsed JS source location (`filename:line:col`).
pub(crate) struct JsLocation {
    pub(crate) colno: Option<u32>,
    pub(crate) filename: Option<String>,
    pub(crate) lineno: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackFormat {
    SpiderMonkey,
    Unknown,
    V8,
}

/// Parsed WASM location (`wasm-function[index]:0xoffset`).
pub(crate) struct WasmLocation {
    pub(crate) byte_offset: Option<u64>,
    pub(crate) function_index: Option<u32>,
}

impl Frame {
    /// Build a [`Frame`] from a raw function name and location string.
    fn build(raw_name: Option<&str>, location: &str) -> Self {
        let raw_name = raw_name.map(strip_wasm_module_prefix);
        let wasm = WasmLocation::parse(location);

        let raw_name = match (raw_name, wasm.function_index) {
            (Some(name), Some(idx)) if name.parse::<u32>().ok() == Some(idx) => None,
            (name, _) => name,
        };

        let js = if wasm.function_index.is_none() {
            JsLocation::parse(location)
        } else {
            JsLocation { colno: None, filename: None, lineno: None }
        };

        let demangled = raw_name.map(demangle_symbol);
        let in_app = demangled
            .as_deref()
            .map(is_in_app)
            .unwrap_or(true);

        Self {
            function: demangled.map(Cow::into_owned),
            raw_function: raw_name.map(str::to_string),
            filename: js.filename,
            lineno: js.lineno,
            colno: js.colno,
            wasm_function_index: wasm.function_index,
            wasm_byte_offset: wasm.byte_offset,
            in_app,
        }
    }
}

impl JsLocation {
    /// Parse `filename:line:col` from a JS location string.
    ///
    /// Parses from the right to avoid tripping on colons in URLs
    /// (e.g. `http://localhost:3030/index.js:187:13`).
    pub(crate) fn parse(location: &str) -> Self {
        if let Some((rest, col_str)) = location.rsplit_once(':') {
            if let Ok(col) = col_str.parse::<u32>() {
                if let Some((url, line_str)) = rest.rsplit_once(':') {
                    if let Ok(line) = line_str.parse::<u32>() {
                        return Self {
                            colno: Some(col),
                            filename: Some(url.to_string()),
                            lineno: Some(line),
                        };
                    }
                }
                return Self {
                    colno: None,
                    filename: Some(rest.to_string()),
                    lineno: Some(col),
                };
            }
        }

        if location.is_empty() {
            Self { colno: None, filename: None, lineno: None }
        } else {
            Self { colno: None, filename: Some(location.to_string()), lineno: None }
        }
    }
}

impl StackFormat {
    /// Detect the stack format from the first meaningful line.
    pub(crate) fn detect(stack: &str) -> Self {
        for line in stack.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("Error") {
                continue;
            }
            if trimmed.starts_with("at ") {
                return Self::V8;
            }
            if trimmed.contains('@') {
                return Self::SpiderMonkey;
            }
        }
        Self::Unknown
    }

    /// Parse a single stack line into a [`Frame`], if possible.
    pub(crate) fn parse_line(self, line: &str) -> Option<Frame> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Error") {
            return None;
        }
        match self {
            Self::V8 => Self::parse_v8(trimmed),
            Self::SpiderMonkey => Self::parse_spidermonkey(trimmed),
            Self::Unknown => None,
        }
    }

    /// ```text
    /// <function>@<url>:wasm-function[<idx>]:0x<offset>
    /// @<url>:line:col              → anonymous
    /// ```
    fn parse_spidermonkey(line: &str) -> Option<Frame> {
        let at_pos = line.find('@')?;
        let raw_name = &line[..at_pos];
        let location = &line[at_pos + 1..];

        if location == "[native code]" {
            return None;
        }

        let name = (!raw_name.is_empty()).then_some(raw_name);

        Some(Frame::build(name, location))
    }

    /// ```text
    /// at <function> (<location>)     → named frame
    /// at <location>                  → anonymous frame
    /// ```
    fn parse_v8(line: &str) -> Option<Frame> {
        let rest = line.strip_prefix("at ")?;

        let (raw_name, location) = if rest.ends_with(')') {
            let paren_open = rest.rfind(" (")?;
            let name = &rest[..paren_open];
            let loc = &rest[paren_open + 2..rest.len() - 1];
            (Some(name), loc)
        } else {
            (None, rest)
        };

        Some(Frame::build(raw_name, location))
    }
}

impl WasmLocation {
    /// Parse `wasm-function[index]` and `0xoffset` from a location string.
    ///
    /// Works on V8 (`wasm://wasm/<hash>:wasm-function[N]:0xOFF`),
    /// SpiderMonkey (`http://…/app.wasm:wasm-function[N]:0xOFF`), and
    /// WebKit (`wasm-function[N]`) locations.
    pub(crate) fn parse(location: &str) -> Self {
        let after_marker = if let Some(pos) = location.find(":wasm-function[") {
            &location[pos + ":wasm-function[".len()..]
        } else if location.starts_with("wasm-function[") {
            &location["wasm-function[".len()..]
        } else {
            return Self { byte_offset: None, function_index: None };
        };

        let Some(bracket_end) = after_marker.find(']') else {
            return Self { byte_offset: None, function_index: None };
        };

        let Ok(fn_index) = after_marker[..bracket_end].parse::<u32>() else {
            return Self { byte_offset: None, function_index: None };
        };

        let byte_offset = after_marker[bracket_end + 1..]
            .strip_prefix(":0x")
            .and_then(|hex| u64::from_str_radix(hex, 16).ok());

        Self { byte_offset, function_index: Some(fn_index) }
    }
}

/// Heuristic: returns `false` for standard library, wasm-bindgen glue,
/// and panic infrastructure frames.
pub(crate) fn is_in_app(function: &str) -> bool {
    const NOT_IN_APP_PREFIXES: &[&str] = &[
        "std::", "core::", "alloc::", "wasm_bindgen::",
        "console_error_panic_hook::", "<alloc::", "<core::", "<std::",
    ];
    const NOT_IN_APP_CONTAINS: &[&str] = &[
        "__wbg_", "__wbindgen_", "__rust_start_panic",
        "rust_begin_unwind", "rust_panic",
    ];

    !NOT_IN_APP_PREFIXES.iter().any(|p| function.starts_with(p))
        && !NOT_IN_APP_CONTAINS.iter().any(|n| function.contains(n))
}

/// Strip browser-added WASM module name prefix.
///
/// V8 and SpiderMonkey prefix WASM function names with the module name:
/// `module.wasm.crate::func::hash` → `crate::func::hash`
fn strip_wasm_module_prefix(name: &str) -> &str {
    match name.find(".wasm.") {
        Some(pos) => &name[pos + 6..],
        None => name,
    }
}
