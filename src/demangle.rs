//! Rust symbol demangling and hash-suffix stripping.

use std::borrow::Cow;

use rustc_demangle::demangle;

/// Demangle a Rust symbol and strip the hash suffix.
///
/// Handles both properly mangled symbols (`_ZN...`, `_R...`) and
/// already-demangled path-form symbols (`crate::func::h1a2b3c`) that
/// appear in WASM name sections.
pub(crate) fn demangle_symbol(raw: &str) -> Cow<'_, str> {
    let demangled = demangle(raw);
    // {:#} tells rustc-demangle to omit the hash suffix
    let display = format!("{demangled:#}");

    if display != raw {
        return Cow::Owned(display);
    }

    Cow::Borrowed(strip_hash(raw))
}

/// Strip a trailing `::h<hex_digits>` hash suffix.
///
/// Requires at least 5 hex digits to avoid false positives on short
/// names like `::handler`.
fn strip_hash(s: &str) -> &str {
    if let Some(pos) = s.rfind("::h") {
        let suffix = &s[pos + 3..];
        if suffix.len() >= 5 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            return &s[..pos];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_like_name_preserved() {
        // "hash" has only 4 chars after ::h, so not stripped
        assert_eq!(demangle_symbol("my_crate::hash"), "my_crate::hash");
    }

    #[test]
    fn no_hash_unchanged() {
        assert_eq!(demangle_symbol("my_crate::handler"), "my_crate::handler");
    }

    #[test]
    fn non_hex_suffix_not_stripped() {
        assert_eq!(demangle_symbol("my_crate::helper"), "my_crate::helper");
    }

    #[test]
    fn short_hash_not_stripped() {
        // Only 4 hex chars — below the 5-char threshold
        assert_eq!(
            demangle_symbol("my_crate::func::h1a2b"),
            "my_crate::func::h1a2b"
        );
    }

    #[test]
    fn strip_long_hash() {
        assert_eq!(
            demangle_symbol("my_crate::handler::h1a2b3c4d5e6f7a8b"),
            "my_crate::handler"
        );
    }

    #[test]
    fn strip_path_form_hash() {
        assert_eq!(
            demangle_symbol("my_crate::handler::h86f485cc"),
            "my_crate::handler"
        );
    }
}
