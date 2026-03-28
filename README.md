# wacks

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
| Safari   | Frames dropped (`wasm-stub@[native code]` carries no useful data) |

For Safari, supplement with `PanicHookInfo::location()` to get the panic site's file/line/col.

## Features

- `serde` — derives `Serialize` / `Deserialize` on `Frame`

## License

MIT OR Apache-2.0
