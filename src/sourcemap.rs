//! Source map rewriting for WASM frames.

use std::cell::OnceCell;

use crate::Frame;

thread_local! {
    pub(crate) static SOURCEMAP_FILENAME: OnceCell<String> = const { OnceCell::new() };
}

pub(crate) trait RewriteForSourcemap {
    fn rewrite_for_sourcemap(&mut self);
}

impl RewriteForSourcemap for [Frame] {
    fn rewrite_for_sourcemap(&mut self) {
        SOURCEMAP_FILENAME.with(|cell| {
            let Some(filename) = cell.get() else { return };
            for frame in self.iter_mut() {
                let Some(offset) = frame.wasm_byte_offset else { continue };
                frame.filename = Some(filename.clone());
                frame.lineno = Some(1);
                frame.colno = u32::try_from(offset).ok();
            }
        });
    }
}
