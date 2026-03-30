//! DWARF-to-source-map v3 converter for WASM binaries.
//!
//! Reads DWARF debug info embedded in a `.wasm` file and produces a
//! [source map v3](https://sourcemaps.info/spec.html) JSON value.
//!
//! ```rust,ignore
//! let wasm = std::fs::read("app.wasm")?;
//! let (map, num_mappings) = wacks::sourcemap_gen::generate(&wasm)?;
//! ```

use std::collections::BTreeMap;
use std::{env, fs};

use anyhow::{Context, Result, anyhow};
use gimli::{ColumnType, Dwarf, EndianSlice, LittleEndian};
use leb128::read::unsigned as read_leb128;
use object::{File, Object, ObjectSection};
use serde_json::json;

struct DwarfReader<'a> {
    code_section_offset: u64,
    dwarf: Dwarf<Reader<'a>>,
}

struct Entry {
    addr: u64,
    col: u32,
    line: u32,
    src_idx: usize,
}

type Reader<'a> = EndianSlice<'a, LittleEndian>;

/// Strip a DWARF source path to a PII-free relative form.
///
/// Strips `$HOME/` if it matches, otherwise strips the leading `/`.
fn make_relative(path: &str, home_prefix: &str) -> String {
    if !home_prefix.is_empty() {
        if let Some(rest) = path.strip_prefix(home_prefix) {
            return rest.to_string();
        }
    }
    path.strip_prefix('/').unwrap_or(path).to_string()
}

impl<'a> DwarfReader<'a> {
    fn new(obj: &'a File<'a>, code_section_offset: u64) -> Result<Self> {
        let dwarf = Dwarf::load(|id| {
            let data = obj
                .section_by_name(id.name())
                .and_then(|s| s.data().ok())
                .unwrap_or_default();
            Ok::<_, gimli::Error>(Reader::new(data, LittleEndian))
        })
        .context("loading DWARF sections")?;

        Ok(Self {
            code_section_offset,
            dwarf,
        })
    }

    /// Collect DWARF line entries, returning display paths, absolute paths
    /// (for reading source content), and mapping entries.
    fn collect_entries(&self) -> Result<(Vec<String>, Vec<String>, Vec<Entry>)> {
        let mut sources: Vec<String> = Vec::new();
        let mut abs_sources: Vec<String> = Vec::new();
        let mut source_idx: BTreeMap<String, usize> = BTreeMap::new();
        let mut entries: Vec<Entry> = Vec::new();
        let mut units = self.dwarf.units();

        while let Some(header) = units.next()? {
            let unit = self.dwarf.unit(header)?;
            let comp_dir = unit
                .comp_dir
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
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
                        self.dwarf
                            .attr_string(&unit, d)
                            .map(|s| s.to_string_lossy().into_owned())
                    })
                    .transpose()?
                    .unwrap_or_default();
                let name = self
                    .dwarf
                    .attr_string(&unit, file.path_name())?
                    .to_string_lossy()
                    .into_owned();
                let path = if dir.is_empty() {
                    name
                } else {
                    format!("{dir}/{name}")
                };

                let idx = *source_idx.entry(path.clone()).or_insert_with(|| {
                    let abs = if path.starts_with('/') || comp_dir.is_empty() {
                        path.clone()
                    } else {
                        format!("{comp_dir}/{path}")
                    };
                    sources.push(path);
                    abs_sources.push(abs);
                    sources.len() - 1
                });

                let line = row.line().map(|l| l.get() as u32).unwrap_or(0);
                let col = match row.column() {
                    ColumnType::LeftEdge => 0,
                    ColumnType::Column(c) => c.get() as u32,
                };

                entries.push(Entry {
                    addr: row.address() + self.code_section_offset,
                    src_idx: idx,
                    line,
                    col,
                });
            }
        }

        Ok((sources, abs_sources, entries))
    }

    fn encode_vlq_mappings(entries: &[Entry]) -> Result<String> {
        let mut buf: Vec<u8> = Vec::new();
        let mut prev_col: i64 = 0;
        let mut prev_line: i64 = 0;
        let mut prev_src: i64 = 0;
        let mut prev_src_col: i64 = 0;

        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                buf.push(b',');
            }

            let line_0 = if entry.line > 0 { entry.line - 1 } else { 0 } as i64;
            vlq::encode(entry.addr as i64 - prev_col, &mut buf)?;
            vlq::encode(entry.src_idx as i64 - prev_src, &mut buf)?;
            vlq::encode(line_0 - prev_line, &mut buf)?;
            vlq::encode(entry.col as i64 - prev_src_col, &mut buf)?;

            prev_col = entry.addr as i64;
            prev_src = entry.src_idx as i64;
            prev_line = line_0;
            prev_src_col = entry.col as i64;
        }

        String::from_utf8(buf).context("VLQ produced invalid UTF-8")
    }

    fn find_code_section_offset(data: &[u8]) -> Result<u64> {
        let mut pos = 8;
        while pos < data.len() {
            let section_id = data[pos];
            pos += 1;

            let mut rest = &data[pos..];
            let size = read_leb128(&mut rest).context("invalid LEB128 section size")?;
            pos = data.len() - rest.len();

            if section_id == 10 {
                return Ok(pos as u64);
            }

            pos += size as usize;
        }

        Err(anyhow!("WASM code section not found"))
    }
}

/// Generate a source map v3 JSON value from a WASM binary with DWARF debug info.
///
/// Source paths are made relative by stripping `$HOME/` (or just the
/// leading `/`) to avoid leaking PII like usernames or home directories.
///
/// Returns the JSON value and the number of mappings emitted.
pub fn generate(wasm: &[u8]) -> Result<(serde_json::Value, usize)> {
    let code_offset = DwarfReader::find_code_section_offset(wasm)?;
    let obj = File::parse(wasm).context("parsing wasm object")?;
    let reader = DwarfReader::new(&obj, code_offset)?;

    let (raw_sources, abs_sources, mut entries) = reader.collect_entries()?;

    entries.sort_by_key(|e| e.addr);

    let mappings = DwarfReader::encode_vlq_mappings(&entries)?;
    let num_mappings = entries.len();

    let sources_content: Vec<serde_json::Value> = abs_sources
        .iter()
        .map(|path| {
            fs::read_to_string(path)
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null)
        })
        .collect();

    let home_prefix = env::var("HOME")
        .map(|h| format!("{}/", h.replace('\\', "/")))
        .unwrap_or_default();

    let sources: Vec<String> = raw_sources
        .into_iter()
        .map(|s| s.replace('\\', "/"))
        .map(|s| make_relative(&s, &home_prefix))
        .collect();

    let map = json!({
        "version": 3,
        "sources": sources,
        "sourcesContent": sources_content,
        "names": [],
        "mappings": mappings,
    });

    Ok((map, num_mappings))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/user/";

    #[test]
    fn strips_home_prefix() {
        assert_eq!(
            make_relative("/home/user/project/src/main.rs", HOME),
            "project/src/main.rs",
        );
        assert_eq!(
            make_relative(
                "/home/user/.cargo/registry/src/idx/serde-1.0/src/lib.rs",
                HOME
            ),
            ".cargo/registry/src/idx/serde-1.0/src/lib.rs",
        );
    }

    #[test]
    fn strips_leading_slash_when_not_under_home() {
        assert_eq!(
            make_relative("/rustc/abc123/library/core/src/ptr.rs", HOME),
            "rustc/abc123/library/core/src/ptr.rs",
        );
        assert_eq!(
            make_relative("/rust/deps/dlmalloc-0.2.11/src/lib.rs", HOME),
            "rust/deps/dlmalloc-0.2.11/src/lib.rs",
        );
    }

    #[test]
    fn already_relative_unchanged() {
        assert_eq!(make_relative("src/lib.rs", HOME), "src/lib.rs");
    }

    #[test]
    fn empty_home_prefix_is_harmless() {
        assert_eq!(make_relative("/some/path/file.rs", ""), "some/path/file.rs");
    }

    #[test]
    fn no_home_directory_survives() {
        let cases = [
            "/home/user/project/src/main.rs",
            "/home/user/.cargo/registry/src/idx/serde-1.0/src/lib.rs",
            "/home/user/.cargo/git/checkouts/repo-abc/def/src/lib.rs",
            "/home/user/.rustup/toolchains/stable/lib/src/lib.rs",
        ];

        for path in cases {
            let result = make_relative(path, HOME);
            assert!(
                !result.contains("/home/user"),
                "home directory leaked in: {result} (from {path})",
            );
        }
    }
}
