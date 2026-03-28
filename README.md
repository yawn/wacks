# wacks

[![CI](https://github.com/yawn/wacks/actions/workflows/ci.yml/badge.svg)](https://github.com/yawn/wacks/actions/workflows/ci.yml)

Structured WASM panic stack traces for browsers.

`wacks` captures `Error.stack` from inside a WASM panic hook, parses it into structured `Frame`s across Chrome, Firefox, and Safari, and demangles Rust symbols — giving you data suitable for error reporting services (PostHog, Sentry, Datadog, etc.).

## Usage

```rust
use wacks::{capture, Frame};

std::panic::set_hook(Box::new(|info| {
    let frames: Vec<Frame> = capture();
    // forward to your error reporter …
}));
```

Or use the convenience hook:

```rust
wacks::set_panic_hook(|frames, info| {
    // frames: Vec<Frame>, info: &PanicHookInfo
});
```

## Build configuration for useful stack traces

The WASM binary **must** retain its name section for function names to appear in `Error.stack`. Without it, browsers only show `wasm-function[N]` with no symbol — every frame will be anonymous.

Add this to your `Cargo.toml`:

```toml
[profile.release]
strip = "none"           # keep the name section
debug = "line-tables-only" # minimal debug info for file/line mapping
```

If you use `wasm-bindgen`, pass `--keep-debug` to preserve debug info through the bindgen step:

```sh
wasm-bindgen --keep-debug --target web ...
```

## Browser support

| Browser  | Result |
|----------|--------|
| Chrome   | Full frames with demangled function names + WASM byte offsets |
| Firefox  | Full frames with demangled function names + WASM byte offsets |
| Safari   | Full frames with demangled function names (no byte offsets) |

Safari/WebKit nondeterministically drops WASM function names from `Error.stack`. The `name-section` feature (on by default) works around this by reading the WASM binary's name section at runtime via `WebAssembly.Module.customSections()` and backfilling any missing names automatically.

## Source map support

### Runtime resolution (`source-map`)

The `source-map` feature resolves WASM byte offsets to original source locations using a source map v3 JSON file:

```toml
[dependencies]
wacks = { version = "0.1", features = ["source-map"] }
```

### Generating source maps (`wasm2map`)

The `wasm2map` binary converts DWARF debug info embedded in a WASM binary to a source map v3 JSON file:

```sh
cargo install wacks --features source-map-gen
wasm2map input.wasm output.wasm.map
```

This requires `debug = "line-tables-only"` (or higher) in your release profile.

## Features

- `name-section` — resolves missing WASM function names at runtime from the binary's name section. Required for reliable Safari support:

  ```toml
  [dependencies]
  wacks = { version = "0.1", features = ["name-section"] }
  ```

- `source-map` — resolves WASM byte offsets to original file/line/column via a source map v3 file
- `source-map-gen` — builds the `wasm2map` binary for generating source maps from DWARF debug info
- `serde` — derives `Serialize` / `Deserialize` on `Frame`

## License

MIT OR Apache-2.0
