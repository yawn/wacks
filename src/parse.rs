//! `Error.stack` string parsing into structured frames.

use crate::format::StackFormat;
use crate::Frame;

impl Frame {
    /// Parse an `Error.stack` string into structured [`Frame`]s.
    ///
    /// Supports Chrome/V8 and Firefox/SpiderMonkey stack formats.
    /// Safari/JSC frames (`wasm-stub@[native code]`) are dropped since
    /// they carry no useful information.
    ///
    /// Returns frames in newest-first order (matching browser convention).
    pub fn parse(stack: &str) -> Vec<Self> {
        let format = StackFormat::detect(stack);

        stack
            .lines()
            .filter_map(|line| format.parse_line(line))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::format::{is_in_app, JsLocation, StackFormat, WasmLocation};
    use crate::Frame;

    #[test]
    fn detect_spidermonkey_format() {
        let stack =
            "my_crate::func::h123abc@http://localhost/app.wasm:wasm-function[1]:0x100\n";
        assert_eq!(StackFormat::detect(stack), StackFormat::SpiderMonkey);
    }

    #[test]
    fn detect_unknown_empty() {
        assert_eq!(StackFormat::detect(""), StackFormat::Unknown);
    }

    #[test]
    fn detect_unknown_error_only() {
        assert_eq!(StackFormat::detect("Error: boom"), StackFormat::Unknown);
    }

    #[test]
    fn detect_v8_format() {
        let stack = "Error\n    at my_crate::func::h123abc (wasm://wasm/abc:wasm-function[1]:0x100)\n";
        assert_eq!(StackFormat::detect(stack), StackFormat::V8);
    }

    #[test]
    fn empty_stack_returns_empty() {
        assert!(Frame::parse("").is_empty());
    }

    #[test]
    fn error_header_only() {
        assert!(Frame::parse("Error: something went wrong").is_empty());
    }

    #[test]
    fn frame_display_js() {
        let f = Frame {
            function: Some("Object.__wbg_new".into()),
            raw_function: Some("Object.__wbg_new".into()),
            filename: Some("index.js".into()),
            lineno: Some(187),
            colno: Some(13),
            wasm_function_index: None,
            wasm_byte_offset: None,
            in_app: false,
        };
        assert_eq!(f.to_string(), "Object.__wbg_new at index.js:187:13");
    }

    #[test]
    fn frame_display_unknown() {
        let f = Frame {
            function: None,
            raw_function: None,
            filename: None,
            lineno: None,
            colno: None,
            wasm_function_index: Some(10),
            wasm_byte_offset: Some(0xff),
            in_app: true,
        };
        assert_eq!(f.to_string(), "<unknown> at wasm-function[10]:0xff");
    }

    #[test]
    fn frame_display_wasm() {
        let f = Frame {
            function: Some("my_crate::handler".into()),
            raw_function: Some("my_crate::handler::h86f485cc".into()),
            filename: None,
            lineno: None,
            colno: None,
            wasm_function_index: Some(58),
            wasm_byte_offset: Some(0x9065),
            in_app: true,
        };
        assert_eq!(f.to_string(), "my_crate::handler at wasm-function[58]:0x9065");
    }

    #[test]
    fn in_app_user_code() {
        assert!(is_in_app("my_crate::handler"));
        assert!(is_in_app("app::routes::index"));
    }

    #[test]
    fn js_location_full() {
        let loc = JsLocation::parse("http://localhost:3030/index.js:187:13");
        assert_eq!(loc.filename.as_deref(), Some("http://localhost:3030/index.js"));
        assert_eq!(loc.lineno, Some(187));
        assert_eq!(loc.colno, Some(13));
    }

    #[test]
    fn js_location_line_only() {
        let loc = JsLocation::parse("http://localhost:3030/index.js:42");
        assert_eq!(loc.filename.as_deref(), Some("http://localhost:3030/index.js"));
        assert_eq!(loc.lineno, Some(42));
        assert_eq!(loc.colno, None);
    }

    #[test]
    fn js_location_no_line() {
        let loc = JsLocation::parse("http://localhost:3030/index.js");
        assert!(loc.filename.is_some());
        assert!(loc.lineno.is_none());
        assert!(loc.colno.is_none());
    }

    #[test]
    fn mixed_wasm_and_js_frames() {
        let stack = "\
Error
    at std::panicking::begin_panic::ha1b2c3d4 (wasm://wasm/abc:wasm-function[100]:0x5000)
    at my_crate::level_3::haaaabbbb (wasm://wasm/abc:wasm-function[50]:0x3000)
    at my_crate::level_2::hccccdddd (wasm://wasm/abc:wasm-function[49]:0x2f00)
    at my_crate::level_1::heeeeffff (wasm://wasm/abc:wasm-function[48]:0x2e00)
    at Object.__wbg_call_123 (http://localhost:8080/index.js:200:15)
";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 5);

        assert!(!frames[0].in_app);
        assert_eq!(frames[0].function.as_deref(), Some("std::panicking::begin_panic"));
        assert!(frames[1].in_app);
        assert!(frames[2].in_app);
        assert!(frames[3].in_app);
        assert!(!frames[4].in_app);
        assert!(frames[4].wasm_function_index.is_none());
        assert_eq!(frames[4].lineno, Some(200));
    }

    #[test]
    fn not_in_app_generic_std_impls() {
        assert!(!is_in_app("<core::fmt::Arguments>::new_v1"));
        assert!(!is_in_app("<alloc::string::String as core::fmt::Display>::fmt"));
    }

    #[test]
    fn not_in_app_panic_infra() {
        assert!(!is_in_app("__rust_start_panic"));
        assert!(!is_in_app("rust_begin_unwind"));
        assert!(!is_in_app("rust_panic"));
        assert!(!is_in_app("console_error_panic_hook::hook"));
    }

    #[test]
    fn not_in_app_std() {
        assert!(!is_in_app("std::panicking::begin_panic"));
        assert!(!is_in_app("core::result::unwrap_failed"));
        assert!(!is_in_app("alloc::raw_vec::RawVec"));
    }

    #[test]
    fn not_in_app_wasm_bindgen() {
        assert!(!is_in_app("wasm_bindgen::convert::closures"));
        assert!(!is_in_app("Object.__wbg_new_abcdef"));
        assert!(!is_in_app("__wbindgen_throw"));
    }

    #[test]
    fn parse_spidermonkey_anonymous() {
        let stack = "@http://localhost:3030/index_bg.wasm:wasm-function[5]:0x42\n";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].function.is_none());
        assert!(frames[0].raw_function.is_none());
        assert_eq!(frames[0].wasm_function_index, Some(5));
    }

    #[test]
    fn parse_spidermonkey_js_frame() {
        let stack = "__wbg_new_abc@http://localhost:3030/index.js:42:10\n";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert!(!f.in_app);
        assert_eq!(f.filename.as_deref(), Some("http://localhost:3030/index.js"));
        assert_eq!(f.lineno, Some(42));
        assert_eq!(f.colno, Some(10));
    }

    #[test]
    fn parse_spidermonkey_module_prefix_stripped() {
        let stack = "my_module.wasm.core::panicking::panic_fmt::hb8badb9a@http://localhost:3030/app.wasm:wasm-function[130]:0x1000\n";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].function.as_deref(), Some("core::panicking::panic_fmt"));
        assert!(!frames[0].in_app);
    }

    #[test]
    fn parse_spidermonkey_multi_frame() {
        let stack = "\
console_error_panic_hook::Error::new::hb2b929@http://localhost:3030/index_bg.wasm:wasm-function[222]:0x11dae
my_crate::handler::h86f485cc@http://localhost:3030/index_bg.wasm:wasm-function[58]:0x9065
";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 2);
        assert!(!frames[0].in_app);
        assert!(frames[1].in_app);
    }

    #[test]
    fn parse_spidermonkey_wasm_frame() {
        let stack =
            "my_crate::handler::h86f485cc@http://localhost:3030/index_bg.wasm:wasm-function[58]:0x9065\n";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.function.as_deref(), Some("my_crate::handler"));
        assert_eq!(
            f.raw_function.as_deref(),
            Some("my_crate::handler::h86f485cc")
        );
        assert_eq!(f.wasm_function_index, Some(58));
        assert_eq!(f.wasm_byte_offset, Some(0x9065));
        assert!(f.in_app);
    }

    #[test]
    fn parse_v8_anonymous_wasm_frame() {
        let stack = "Error\n    at wasm://wasm/abc:wasm-function[10]:0xff\n";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert!(f.function.is_none());
        assert!(f.raw_function.is_none());
        assert_eq!(f.wasm_function_index, Some(10));
        assert_eq!(f.wasm_byte_offset, Some(0xff));
        assert!(f.in_app);
    }

    #[test]
    fn parse_v8_js_frame() {
        let stack =
            "Error\n    at Object.__wbg_new_abcdef (http://localhost:3030/index.js:187:13)\n";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.function.as_deref(), Some("Object.__wbg_new_abcdef"));
        assert!(f.wasm_function_index.is_none());
        assert_eq!(
            f.filename.as_deref(),
            Some("http://localhost:3030/index.js")
        );
        assert_eq!(f.lineno, Some(187));
        assert_eq!(f.colno, Some(13));
        assert!(!f.in_app);
    }

    #[test]
    fn parse_v8_module_prefix_stripped() {
        let stack = "\
Error
    at my_module.wasm.std::panicking::panic_with_hook::hab12cd (wasm://wasm/abc:wasm-function[88]:0x1000)
    at my_module.wasm.my_crate::handler::h86f485cc (wasm://wasm/abc:wasm-function[58]:0x9065)
";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].function.as_deref(), Some("std::panicking::panic_with_hook"));
        assert_eq!(frames[0].raw_function.as_deref(), Some("std::panicking::panic_with_hook::hab12cd"));
        assert!(!frames[0].in_app);
        assert_eq!(frames[1].function.as_deref(), Some("my_crate::handler"));
        assert!(frames[1].in_app);
    }

    #[test]
    fn parse_v8_multi_frame() {
        let stack = "\
Error
    at console_error_panic_hook::Error::new::hb2b929 (wasm://wasm/16d24f76:wasm-function[222]:0x11dae)
    at my_crate::handler::h86f485cc (wasm://wasm/16d24f76:wasm-function[58]:0x9065)
";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 2);
        assert!(!frames[0].in_app);
        assert!(frames[1].in_app);
    }

    #[test]
    fn parse_v8_named_wasm_frame() {
        let stack = "\
Error
    at my_crate::handler::h86f485cc (wasm://wasm/16d24f76:wasm-function[58]:0x9065)
";
        let frames = Frame::parse(stack);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.function.as_deref(), Some("my_crate::handler"));
        assert_eq!(
            f.raw_function.as_deref(),
            Some("my_crate::handler::h86f485cc")
        );
        assert_eq!(f.wasm_function_index, Some(58));
        assert_eq!(f.wasm_byte_offset, Some(0x9065));
        assert!(f.in_app);
        assert!(f.filename.is_none());
    }

    #[test]
    fn safari_native_code_dropped() {
        let stack = "wasm-stub@[native code]\n@[native code]\n";
        let frames = Frame::parse(stack);
        assert!(frames.is_empty());
    }

    #[test]
    fn wasm_location_bare_webkit() {
        let loc = WasmLocation::parse("wasm-function[7]");
        assert_eq!(loc.function_index, Some(7));
        assert_eq!(loc.byte_offset, None);
    }

    #[test]
    fn wasm_location_full() {
        let loc = WasmLocation::parse("wasm://wasm/16d24f76:wasm-function[222]:0x11dae");
        assert_eq!(loc.function_index, Some(222));
        assert_eq!(loc.byte_offset, Some(0x11dae));
    }

    #[test]
    fn wasm_location_no_offset() {
        let loc = WasmLocation::parse("wasm://wasm/abc:wasm-function[5]");
        assert_eq!(loc.function_index, Some(5));
        assert_eq!(loc.byte_offset, None);
    }

    #[test]
    fn wasm_location_spidermonkey_url() {
        let loc =
            WasmLocation::parse("http://localhost:3030/index_bg.wasm:wasm-function[58]:0x9065");
        assert_eq!(loc.function_index, Some(58));
        assert_eq!(loc.byte_offset, Some(0x9065));
    }
}
