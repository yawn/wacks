//! Build-time framemap generator.
//!
//! Parses WASM instructions to build a call-site index and, when DWARF
//! debug info is present, a byte-offset → source location table. The
//! resulting framemap enables both WebKit byte offset resolution and
//! runtime source location resolution without external source maps.
//!
//! ```rust,ignore
//! let wasm = std::fs::read("app.wasm")?;
//! let framemap = wacks::framemap_gen::generate(&wasm)?;
//! std::fs::write("app.framemap", framemap)?;
//! ```

use std::collections::BTreeMap;
use std::env;

use anyhow::{Context, Result, anyhow};
use gimli::{ColumnType, Dwarf, EndianSlice, LittleEndian};
use leb128::read::unsigned as read_leb128;
use object::{File, Object, ObjectSection};
use serde::{Deserialize, Serialize};
use wasmparser::{Operator, Parser, Payload};

/// Call site entry: a direct `call` instruction within a WASM function.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CallSite {
    pub caller: u32,
    pub callee: u32,
    pub offset: u64,
}

/// Framemap: call-site index, function starts, and optional DWARF line info.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Framemap {
    pub num_imports: u32,
    pub function_starts: Vec<u64>,
    pub call_sites: Vec<CallSite>,
    pub sources: Vec<String>,
    pub line_entries: Vec<LineEntry>,
}

/// DWARF line entry: byte offset → source location.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LineEntry {
    pub addr: u64,
    pub source_idx: u32,
    pub line: u32,
    pub col: u32,
}

struct WasmReader<'a> {
    data: &'a [u8],
    num_imports: u32,
}

impl<'a> WasmReader<'a> {
    /// Use `wasmparser` to find all direct `call` instructions and their byte offsets.
    fn collect_call_sites(&self) -> Result<Vec<CallSite>> {
        let mut call_sites = Vec::new();
        let mut function_index = self.num_imports;

        for payload in Parser::new(0).parse_all(self.data) {
            let payload = payload.context("parsing WASM")?;

            if let Payload::CodeSectionEntry(body) = payload {
                let mut reader = body
                    .get_operators_reader()
                    .context("reading operators")?;

                while !reader.eof() {
                    let offset = reader.original_position() as u64;
                    let op = reader.read().context("reading operator")?;

                    if let Operator::Call { function_index: callee } = op {
                        call_sites.push(CallSite {
                            caller: function_index,
                            callee,
                            offset,
                        });
                    }
                }

                function_index += 1;
            }
        }

        Ok(call_sites)
    }

    /// Walk the code section to find each function's first-instruction byte offset.
    fn collect_function_starts(&self) -> Result<Vec<u64>> {
        let code_start = Self::find_section_start(self.data, 10)?;
        let mut pos = code_start;
        let mut rest = &self.data[pos..];
        let func_count = read_leb128(&mut rest).context("function count")? as usize;
        pos = self.data.len() - rest.len();

        let mut starts = Vec::with_capacity(func_count);

        for _ in 0..func_count {
            let mut rest = &self.data[pos..];
            let body_size = read_leb128(&mut rest).context("body size")? as usize;
            let body_start = self.data.len() - rest.len();

            let mut body = &self.data[body_start..body_start + body_size];
            let local_count = read_leb128(&mut body).context("local count")? as usize;
            for _ in 0..local_count {
                read_leb128(&mut body).context("local entry count")?;
                body = body.get(1..).context("valtype")?;
            }

            starts.push((body_start + body_size - body.len()) as u64);
            pos = body_start + body_size;
        }

        Ok(starts)
    }

    /// Count function imports in the WASM import section (section ID 2).
    fn count_imported_functions(data: &[u8]) -> Result<u32> {
        let pos = match Self::find_section_start(data, 2) {
            Ok(pos) => pos,
            Err(_) => return Ok(0),
        };

        let mut section = &data[pos..];
        let count = read_leb128(&mut section).context("import count")? as u32;
        let mut func_imports = 0u32;

        for _ in 0..count {
            let len = read_leb128(&mut section).context("module name len")? as usize;
            section = section.get(len..).context("module name")?;

            let len = read_leb128(&mut section).context("field name len")? as usize;
            section = section.get(len..).context("field name")?;

            let kind = *section.first().context("import kind")?;
            section = &section[1..];

            match kind {
                0x00 => {
                    func_imports += 1;
                    read_leb128(&mut section).context("type index")?;
                }
                0x01 => {
                    section = &section[1..];
                    Self::skip_limits(&mut section)?;
                }
                0x02 => {
                    Self::skip_limits(&mut section)?;
                }
                0x03 => {
                    section = &section[2..];
                }
                other => return Err(anyhow!("unknown import kind: {other}")),
            }
        }

        Ok(func_imports)
    }

    /// Find the byte offset where a WASM section's content begins.
    fn find_section_start(data: &[u8], target_id: u8) -> Result<usize> {
        let mut pos = 8; // skip magic + version
        while pos < data.len() {
            let section_id = data[pos];
            pos += 1;

            let mut rest = &data[pos..];
            let size = read_leb128(&mut rest).context("section size")? as usize;
            pos = data.len() - rest.len();

            if section_id == target_id {
                return Ok(pos);
            }

            pos += size;
        }

        Err(anyhow!("WASM section {target_id} not found"))
    }

    fn new(data: &'a [u8]) -> Result<Self> {
        let num_imports = Self::count_imported_functions(data)?;
        Ok(Self { data, num_imports })
    }

    fn skip_limits(data: &mut &[u8]) -> Result<()> {
        let flags = *data.first().context("limits flags")?;
        *data = &data[1..];
        read_leb128(data).context("limits min")?;
        if flags & 1 != 0 {
            read_leb128(data).context("limits max")?;
        }
        Ok(())
    }
}

type Reader<'a> = EndianSlice<'a, LittleEndian>;

/// Collect DWARF line entries from a WASM binary, if debug info is present.
fn collect_dwarf_lines(wasm: &[u8]) -> Result<(Vec<String>, Vec<LineEntry>)> {
    let code_offset = WasmReader::find_section_start(wasm, 10)
        .map(|pos| pos as u64)
        .unwrap_or(0);

    let obj = match File::parse(wasm) {
        Ok(obj) => obj,
        Err(_) => return Ok((Vec::new(), Vec::new())),
    };

    let dwarf = match Dwarf::load(|id| {
        let data = obj
            .section_by_name(id.name())
            .and_then(|s| s.data().ok())
            .unwrap_or_default();
        Ok::<_, gimli::Error>(Reader::new(data, LittleEndian))
    }) {
        Ok(d) => d,
        Err(_) => return Ok((Vec::new(), Vec::new())),
    };

    let home_prefix = env::var("HOME")
        .map(|h| format!("{}/", h.replace('\\', "/")))
        .unwrap_or_default();

    let mut sources: Vec<String> = Vec::new();
    let mut source_idx: BTreeMap<String, usize> = BTreeMap::new();
    let mut entries: Vec<LineEntry> = Vec::new();
    let mut units = dwarf.units();

    while let Some(header) = units.next()? {
        let unit = dwarf.unit(header)?;
        let Some(prog) = unit.line_program.clone() else {
            continue;
        };

        let mut rows = prog.rows();

        while let Some((header, row)) = rows.next_row()? {
            if row.end_sequence() {
                continue;
            }

            let file = row.file(header).context("missing file entry")?;
            let dir = file
                .directory(header)
                .map(|d| {
                    dwarf
                        .attr_string(&unit, d)
                        .map(|s| s.to_string_lossy().into_owned())
                })
                .transpose()?
                .unwrap_or_default();
            let name = dwarf
                .attr_string(&unit, file.path_name())?
                .to_string_lossy()
                .into_owned();
            let path = if dir.is_empty() {
                name
            } else {
                format!("{dir}/{name}")
            };

            let display_path = make_relative(&path.replace('\\', "/"), &home_prefix);

            let idx = *source_idx.entry(display_path.clone()).or_insert_with(|| {
                sources.push(display_path);
                sources.len() - 1
            });

            let line = row.line().map(|l| l.get() as u32).unwrap_or(0);
            let col = match row.column() {
                ColumnType::LeftEdge => 0,
                ColumnType::Column(c) => c.get() as u32,
            };

            entries.push(LineEntry {
                addr: row.address() + code_offset,
                source_idx: idx as u32,
                line,
                col,
            });
        }
    }

    entries.sort_by_key(|e| e.addr);
    Ok((sources, entries))
}

/// Strip a DWARF source path to a PII-free relative form.
fn make_relative(path: &str, home_prefix: &str) -> String {
    if !home_prefix.is_empty()
        && let Some(rest) = path.strip_prefix(home_prefix)
    {
        return rest.to_string();
    }
    path.strip_prefix('/').unwrap_or(path).to_string()
}

/// Generate a serialized framemap from a WASM binary.
///
/// Includes DWARF line info when debug info is present, enabling runtime
/// source location resolution without external source maps.
pub fn generate(wasm: &[u8]) -> Result<Vec<u8>> {
    let reader = WasmReader::new(wasm)?;
    let (sources, line_entries) = collect_dwarf_lines(wasm).unwrap_or_default();

    let framemap = Framemap {
        num_imports: reader.num_imports,
        function_starts: reader.collect_function_starts()?,
        call_sites: reader.collect_call_sites()?,
        sources,
        line_entries,
    };

    postcard::to_allocvec(&framemap).context("serializing framemap")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_import_section_yields_zero() {
        let wasm = b"\x00asm\x01\x00\x00\x00";
        assert_eq!(WasmReader::count_imported_functions(wasm).unwrap(), 0);
    }

    #[test]
    fn roundtrip_framemap() {
        let framemap = Framemap {
            num_imports: 3,
            function_starts: vec![100, 200, 300],
            call_sites: vec![
                CallSite { caller: 3, callee: 4, offset: 120 },
                CallSite { caller: 4, callee: 5, offset: 220 },
            ],
            sources: vec!["src/lib.rs".into(), "src/main.rs".into()],
            line_entries: vec![
                LineEntry { addr: 100, source_idx: 0, line: 10, col: 5 },
                LineEntry { addr: 200, source_idx: 1, line: 42, col: 9 },
            ],
        };

        let bytes = postcard::to_allocvec(&framemap).unwrap();
        let parsed: Framemap = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.num_imports, 3);
        assert_eq!(parsed.function_starts, vec![100, 200, 300]);
        assert_eq!(parsed.call_sites.len(), 2);
        assert_eq!(parsed.call_sites[0], CallSite { caller: 3, callee: 4, offset: 120 });
        assert_eq!(parsed.sources, vec!["src/lib.rs", "src/main.rs"]);
        assert_eq!(parsed.line_entries.len(), 2);
        assert_eq!(parsed.line_entries[0], LineEntry { addr: 100, source_idx: 0, line: 10, col: 5 });
    }

    #[test]
    fn roundtrip_framemap_without_dwarf() {
        let framemap = Framemap {
            num_imports: 1,
            function_starts: vec![50],
            call_sites: vec![],
            sources: vec![],
            line_entries: vec![],
        };

        let bytes = postcard::to_allocvec(&framemap).unwrap();
        let parsed: Framemap = postcard::from_bytes(&bytes).unwrap();

        assert!(parsed.sources.is_empty());
        assert!(parsed.line_entries.is_empty());
    }

    #[test]
    fn make_relative_strips_home() {
        assert_eq!(
            make_relative("/home/user/project/src/main.rs", "/home/user/"),
            "project/src/main.rs",
        );
    }

    #[test]
    fn make_relative_strips_leading_slash() {
        assert_eq!(
            make_relative("/rustc/abc/library/core/src/ptr.rs", "/home/user/"),
            "rustc/abc/library/core/src/ptr.rs",
        );
    }

    #[test]
    fn make_relative_preserves_relative() {
        assert_eq!(make_relative("src/lib.rs", "/home/user/"), "src/lib.rs");
    }
}
