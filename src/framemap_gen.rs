//! Build-time framemap generator for WebKit byte offset resolution.
//!
//! Parses WASM instructions to build a call-site index mapping
//! `(caller, callee) → byte_offset`, enabling exact source map resolution
//! for WebKit frames that only provide function indices.
//!
//! ```rust,ignore
//! let wasm = std::fs::read("app.wasm")?;
//! let framemap = wacks::framemap_gen::generate(&wasm)?;
//! std::fs::write("app.framemap", framemap)?;
//! ```

use anyhow::{Context, Result, anyhow};
use leb128::read::unsigned as read_leb128;
use serde::{Deserialize, Serialize};
use wasmparser::{Operator, Parser, Payload};

/// Call site entry: a direct `call` instruction within a WASM function.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CallSite {
    pub caller: u32,
    pub callee: u32,
    pub offset: u64,
}

/// Framemap: function-start offsets + call-site index for byte offset resolution.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Framemap {
    pub num_imports: u32,
    pub function_starts: Vec<u64>,
    pub call_sites: Vec<CallSite>,
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

/// Generate a serialized framemap from a WASM binary.
pub fn generate(wasm: &[u8]) -> Result<Vec<u8>> {
    let reader = WasmReader::new(wasm)?;

    let framemap = Framemap {
        num_imports: reader.num_imports,
        function_starts: reader.collect_function_starts()?,
        call_sites: reader.collect_call_sites()?,
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
        };

        let bytes = postcard::to_allocvec(&framemap).unwrap();
        let parsed: Framemap = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.num_imports, 3);
        assert_eq!(parsed.function_starts, vec![100, 200, 300]);
        assert_eq!(parsed.call_sites.len(), 2);
        assert_eq!(parsed.call_sites[0], CallSite { caller: 3, callee: 4, offset: 120 });
    }
}
