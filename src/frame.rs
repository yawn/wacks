//! Frame type representing a single entry in a parsed stack trace.

use std::fmt;

/// A single frame from a WASM stack trace.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Frame {
    pub colno: Option<u32>,
    /// Source filename from panic location or source map
    pub filename: Option<String>,
    /// Demangled function name (e.g. `my_crate::handler`)
    pub function: Option<String>,
    /// `false` for browser internals, wasm-bindgen glue, std
    pub in_app: bool,
    pub lineno: Option<u32>,
    /// Raw symbol as it appeared in `Error.stack` (e.g. `my_crate::handler::h86f485cc`)
    pub raw_function: Option<String>,
    /// Byte offset within the WASM module
    pub wasm_byte_offset: Option<u64>,
    /// WASM function table index
    pub wasm_function_index: Option<u32>,
}

impl Frame {
    /// A WASM frame whose function name was not resolved — typically
    /// because the binary's name section was stripped.
    pub fn is_anonymous(&self) -> bool {
        self.wasm_function_index.is_some() && self.function.is_none()
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.function {
            Some(name) => write!(f, "{name}")?,
            None => write!(f, "<unknown>")?,
        }
        if let (Some(idx), Some(offset)) = (self.wasm_function_index, self.wasm_byte_offset) {
            write!(f, " at wasm-function[{idx}]:0x{offset:x}")?;
        } else if let Some(filename) = &self.filename {
            write!(f, " at {filename}")?;
            if let Some(line) = self.lineno {
                write!(f, ":{line}")?;
                if let Some(col) = self.colno {
                    write!(f, ":{col}")?;
                }
            }
        }
        Ok(())
    }
}
