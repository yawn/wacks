//! Converts WASM DWARF debug info to a source map v3 JSON file.
//!
//! Usage: `wasm2map <input.wasm> <output.wasm.map>`

use std::collections::BTreeMap;
use std::{env, fs, process};

use anyhow::{Context, Result, anyhow};
use gimli::{ColumnType, Dwarf, EndianSlice, LittleEndian};
use leb128::read::unsigned as read_leb128;
use object::{File, Object, ObjectSection};
use serde_json::json;

/// DWARF-to-source-map converter for a parsed WASM object.
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

/// Strip a DWARF source path to a leak-free relative form.
///
/// Priority: workspace-relative via `base_prefix`, then cargo registry
/// (`…/.cargo/registry/src/<index>/<crate>/…` → `<crate>/…`), then rustc
/// stdlib (`/rustc/<hash>/…` → `rustc/<hash>/…`). Falls back to stripping
/// any leading `/`.
fn make_relative(path: &str, base_prefix: &str) -> String {
    if let Some(rest) = path.strip_prefix(base_prefix) {
        return rest.to_string();
    }

    if let Some(idx) = path.find("/.cargo/registry/src/") {
        let after = &path[idx + "/.cargo/registry/src/".len()..];
        if let Some(slash) = after.find('/') {
            return after[slash + 1..].to_string();
        }
    }

    if let Some(idx) = path.find("/rust/deps/") {
        return path[idx + 1..].to_string();
    }

    if let Some(idx) = path.find("/rustc/") {
        return path[idx + 1..].to_string();
    }

    path.trim_start_matches('/').to_string()
}

impl<'a> DwarfReader<'a> {
    fn collect_entries(&self) -> Result<(Vec<String>, Vec<Entry>)> {
        let mut sources: Vec<String> = Vec::new();
        let mut source_idx: BTreeMap<String, usize> = BTreeMap::new();
        let mut entries: Vec<Entry> = Vec::new();
        let mut units = self.dwarf.units();

        while let Some(header) = units.next()? {
            let unit = self.dwarf.unit(header)?;
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
                    sources.push(path);
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

        Ok((sources, entries))
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

    /// Scan WASM sections for the Code section (ID 10) payload offset.
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

    /// Detect the workspace root from DWARF `comp_dir` entries.
    ///
    /// Returns the longest common directory prefix of all compilation
    /// directories that belong to workspace crates (i.e. not rustc stdlib
    /// or cargo registry dependencies).
    fn detect_base(&self) -> Result<String> {
        let mut base = String::new();
        let mut units = self.dwarf.units();

        while let Some(header) = units.next()? {
            let unit = self.dwarf.unit(header)?;
            let Some(ref comp_dir) = unit.comp_dir else { continue };
            let dir = comp_dir.to_string_lossy().replace('\\', "/");
            if !dir.starts_with('/')
                || dir.starts_with("/rustc/")
                || dir.starts_with("/rust/deps/")
                || dir.contains("/.cargo/")
            {
                continue;
            }

            if base.is_empty() {
                base = dir;
            } else {
                while !dir.starts_with(&base) {
                    match base.rfind('/') {
                        Some(i) => base.truncate(i),
                        None => {
                            base.clear();
                            break;
                        }
                    }
                }
            }
        }

        Ok(base)
    }

    /// Generate source map v3 JSON from DWARF line programs.
    ///
    /// Paths in `sources` are made relative to avoid leaking local filesystem
    /// details (home directory, username). The workspace root is auto-detected
    /// from DWARF compilation directories; cargo registry paths are trimmed to
    /// `<crate>-<ver>/…`, rustc stdlib paths to `rustc/<hash>/…`.
    fn source_map(&self) -> Result<(serde_json::Value, usize)> {
        let base = self.detect_base()?;
        let (sources, mut entries) = self.collect_entries()?;

        entries.sort_by_key(|e| e.addr);

        let mappings = Self::encode_vlq_mappings(&entries)?;
        let num_mappings = entries.len();
        let prefix = format!("{base}/");

        let sources: Vec<String> = sources
            .into_iter()
            .map(|s| s.replace('\\', "/"))
            .map(|s| make_relative(&s, &prefix))
            .collect();

        let map = json!({
            "version": 3,
            "sources": sources,
            "names": [],
            "mappings": mappings,
        });

        Ok((map, num_mappings))
    }
}

fn main() {
    let run = || -> Result<()> {
        let args: Vec<String> = env::args().collect();
        if args.len() != 3 {
            return Err(anyhow!("usage: wasm2map <input.wasm> <output.wasm.map>"));
        }

        let data = fs::read(&args[1]).context(format!("opening {}", &args[1]))?;
        let code_offset = DwarfReader::find_code_section_offset(&data)?;
        let obj = File::parse(&*data).context("parsing wasm object")?;
        let (map, num_mappings) = DwarfReader::new(&obj, code_offset)?.source_map()?;

        let sources_len = map["sources"].as_array().map(|a| a.len()).unwrap_or(0);
        fs::write(&args[2], serde_json::to_string(&map)?).context("writing output")?;

        eprintln!(
            "wasm2map: {sources_len} sources, {num_mappings} mappings → {}",
            &args[2]
        );

        Ok(())
    };

    if let Err(err) = run() {
        eprintln!("wasm2map: {err:#}");
        process::exit(1);
    }
}
