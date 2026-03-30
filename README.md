# wacks

[![CI](https://github.com/yawn/wacks/actions/workflows/ci.yml/badge.svg)](https://github.com/yawn/wacks/actions/workflows/ci.yml)

Structured WASM panic stack traces for browsers.

`wacks` captures `Error.stack` from inside a WASM panic hook, parses it into structured `Frame`s across Chrome, Firefox, and Safari, and demangles Rust symbols — giving you data suitable for error reporting services (PostHog, Sentry, Datadog, etc.).

Function names are always resolved from the WASM binary's [name section](https://webassembly.github.io/spec/core/appendix/custom.html#name-section), making symbolication reliable across all browsers — including Safari/WebKit, which nondeterministically drops names from `Error.stack`.

## Usage

```rust
wacks::Builder::new()
    .sourcemap("app.wasm.js")
    .install(|frames, info| {
        // frames: Vec<Frame>, info: &PanicHookInfo
    });
```

`Frame::parse` works on any target (not just WASM), so you can use it server-side to process stack traces sent from browsers:

```rust
use wacks::Frame;

let frames = Frame::parse(stack_string);
```

## Build configuration for useful stack traces

The WASM binary **must** retain its name section for function names to appear. Without it, every frame will be anonymous.

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

## Source map support

`Builder::sourcemap` rewrites WASM frames into JS-compatible locations (`filename:1:byteOffset`), allowing error reporting backends (PostHog, Sentry, Datadog, etc.) to resolve them against an uploaded source map using standard JS source map consumers.

### Generating source maps (`sourcemap-gen`)

The `sourcemap-gen` binary converts DWARF debug info embedded in a WASM binary to a source map v3 JSON file:

```sh
cargo install wacks --features sourcemap-gen
sourcemap-gen input.wasm output.wasm.map
```

This requires `debug = "line-tables-only"` (or higher) in your release profile. Upload the generated `.map` file to your error reporting service.

## Features

- `sourcemap-gen` — builds the `sourcemap-gen` binary for generating source maps from DWARF debug info
- `serde` — derives `Serialize` / `Deserialize` on `Frame`

## License

MIT OR Apache-2.0
