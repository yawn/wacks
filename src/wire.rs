//! Serialized framemap types shared between build-time generation and
//! runtime resolution.

use serde::{Deserialize, Serialize};

/// Call site entry: a direct `call` instruction within a WASM function.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CallSite {
    pub(crate) caller: u32,
    pub(crate) callee: u32,
    pub(crate) offset: u32,
}

/// DWARF line entry: byte offset → source location.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct LineEntry {
    pub(crate) addr: u32,
    pub(crate) source_idx: u32,
    pub(crate) line: u32,
    pub(crate) col: u32,
}

/// Serialized framemap format used by both `framemap-gen` (writer) and
/// the runtime resolver (reader).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Framemap {
    pub(crate) num_imports: u32,
    pub(crate) function_starts: Vec<u32>,
    pub(crate) call_sites: Vec<CallSite>,
    pub(crate) sources: Vec<String>,
    pub(crate) line_entries: Vec<LineEntry>,
}
