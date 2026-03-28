//! WASM name section parser for function name resolution.
//!
//! Parses the binary [name custom section] to extract function index → name
//! mappings. Used at runtime to backfill names that WebKit drops from
//! `Error.stack`.
//!
//! [name custom section]: https://webassembly.github.io/spec/core/appendix/custom.html#name-section

use std::collections::HashMap;

use leb128::read::unsigned as read_leb128;

/// Function index → name mapping extracted from a WASM name section.
pub(crate) struct NameSection {
    names: HashMap<u32, String>,
}

impl NameSection {
    /// Parse function names (subsection 1) from a raw name section.
    ///
    /// Input is the payload of the "name" custom section as returned by
    /// `WebAssembly.Module.customSections(module, "name")`. Returns an empty
    /// map on malformed input — never panics.
    pub(crate) fn new(data: &[u8]) -> Self {
        let mut data = data;
        let mut names = HashMap::new();

        while !data.is_empty() {
            let Some(id) = read_byte(&mut data) else {
                break;
            };
            let Some(size) = read_u32(&mut data) else {
                break;
            };
            let size = size as usize;
            if size > data.len() {
                break;
            }

            if id == 1 {
                parse_name_map(&data[..size], &mut names);
            }
            data = &data[size..];
        }

        Self { names }
    }

    pub(crate) fn get(&self, idx: &u32) -> Option<&String> {
        self.names.get(idx)
    }
}

fn parse_name_map(data: &[u8], names: &mut HashMap<u32, String>) {
    let mut data = data;
    let Some(count) = read_u32(&mut data) else {
        return;
    };

    for _ in 0..count {
        let Some(idx) = read_u32(&mut data) else {
            return;
        };
        let Some(len) = read_u32(&mut data) else {
            return;
        };
        let len = len as usize;
        if len > data.len() {
            return;
        }

        if let Ok(name) = std::str::from_utf8(&data[..len]) {
            names.insert(idx, name.to_string());
        }
        data = &data[len..];
    }
}

fn read_byte(data: &mut &[u8]) -> Option<u8> {
    let (&byte, rest) = data.split_first()?;
    *data = rest;
    Some(byte)
}

fn read_u32(data: &mut &[u8]) -> Option<u32> {
    read_leb128(data).ok().and_then(|v| u32::try_from(v).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_name_section(entries: &[(u32, &str)]) -> Vec<u8> {
        let mut payload = leb128_encode(entries.len() as u32);
        for &(idx, name) in entries {
            payload.extend(leb128_encode(idx));
            payload.extend(leb128_encode(name.len() as u32));
            payload.extend(name.as_bytes());
        }

        let mut section = vec![1u8]; // subsection id = function names
        section.extend(leb128_encode(payload.len() as u32));
        section.extend(payload);
        section
    }

    fn leb128_encode(value: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        leb128::write::unsigned(&mut buf, u64::from(value)).unwrap();
        buf
    }

    #[test]
    fn parse_single_function() {
        let data = build_name_section(&[(42, "my_crate::handler::h1234")]);
        let ns = NameSection::new(&data);
        assert_eq!(ns.names.len(), 1);
        assert_eq!(ns.names[&42], "my_crate::handler::h1234");
    }

    #[test]
    fn parse_multiple_functions() {
        let data = build_name_section(&[
            (0, "trigger_panic"),
            (7, "std::panicking::panic_with_hook::hab12cd"),
            (129, "core::panicking::panic_fmt::hff0011"),
        ]);
        let ns = NameSection::new(&data);
        assert_eq!(ns.names.len(), 3);
        assert_eq!(ns.names[&0], "trigger_panic");
        assert_eq!(ns.names[&7], "std::panicking::panic_with_hook::hab12cd");
        assert_eq!(ns.names[&129], "core::panicking::panic_fmt::hff0011");
    }

    #[test]
    fn skips_non_function_subsections() {
        let mut data = Vec::new();

        // Subsection 0 (module name): skip
        let module_name = b"my_module";
        data.push(0u8);
        let mut payload = leb128_encode(module_name.len() as u32);
        payload.extend(module_name);
        data.extend(leb128_encode(payload.len() as u32));
        data.extend(&payload);

        // Subsection 1 (function names): parse
        data.extend(build_name_section(&[(5, "func_a")]));

        let ns = NameSection::new(&data);
        assert_eq!(ns.names.len(), 1);
        assert_eq!(ns.names[&5], "func_a");
    }

    #[test]
    fn empty_input() {
        assert!(NameSection::new(&[]).names.is_empty());
    }

    #[test]
    fn truncated_input_does_not_panic() {
        assert!(NameSection::new(&[1]).names.is_empty());
        assert!(NameSection::new(&[1, 0x80]).names.is_empty());
        assert!(NameSection::new(&[1, 5, 1, 0]).names.is_empty());
    }
}
