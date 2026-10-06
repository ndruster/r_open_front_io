//! Port of `src/client/PagePin.ts` — the `/v/<commit>/` pin latch captured
//! once at boot.
//!
//! Module-level `captured: string | null | undefined` crosses as
//! `Option<Option<String>>` (outer `None` = JS `undefined`, inner `None` = JS
//! `null`). `pagePin()` is lazy: the facade read of `window.location.pathname`
//! (scripted `__PPN_PATH`, which can THROW to model a non-browser host) only
//! happens while `captured === undefined`; the call counter pins that a
//! second read never touches the facade. `stripVersionPrefix(...).commit`
//! reuses [`crate::server_list::strip_version_prefix`] — its `.0` is the
//! `string | null` the TS `.commit` field yields.

use crate::js_json::{push_val, read_str, JsVal};
use crate::server_list::strip_version_prefix;

/// The three-state module latch (`undefined` / `null` / `string`).
#[derive(Debug, Default)]
pub struct PagePinState {
    captured: Option<Option<String>>,
    /// The scripted `window.location.pathname`: `None` models the host that
    /// THROWS on the read (no `window` at all).
    path: Option<String>,
    facade_calls: usize,
}

impl PagePinState {
    /// `pagePin()`.
    pub fn page_pin(&mut self) -> Option<String> {
        if self.captured.is_none() {
            self.facade_calls += 1;
            match &self.path {
                Some(p) => {
                    self.captured = Some(strip_version_prefix(p).0);
                }
                None => {
                    // `window.location.pathname` threw -> catch -> null.
                    self.captured = Some(None);
                }
            }
        }
        self.captured.clone().unwrap()
    }

    /// `capturePagePin()` — drop the latch and take the pin now.
    pub fn capture_page_pin(&mut self) {
        self.captured = None;
        self.page_pin();
    }

    /// `resetPagePinForTests()`.
    pub fn reset_for_tests(&mut self) {
        self.captured = None;
    }
}

/// The capture harness.
#[derive(Debug, Default)]
pub struct RigHarness {
    state: PagePinState,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 setup `[mode, plen, (path)*plen]` -> `[0]`. mode 0 = the facade
    ///   returns the path string, mode 1 = the facade THROWS (non-browser
    ///   host; the path payload is still read but never used). Resets the
    ///   latch and the call counter (the capture reloads the module).
    /// 1 pagePin -> `[codec(captured)]` (`[2]` null, `[5,...]` string);
    /// 2 capturePagePin -> `[0]`;
    /// 3 resetPagePinForTests -> `[0]`;
    /// 4 facadeCalls -> `[count]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                let mode = args[i] as u8;
                i += 1;
                let path = read_str(args, &mut i);
                *self = Self::default();
                self.state.path = if mode == 0 { Some(path) } else { None };
                vec![0.0]
            }
            1 => {
                let mut out = Vec::new();
                let v = match self.state.page_pin() {
                    Some(s) => JsVal::Str(s),
                    None => JsVal::Null,
                };
                push_val(&mut out, &v);
                out
            }
            2 => {
                self.state.capture_page_pin();
                vec![0.0]
            }
            3 => {
                self.state.reset_for_tests();
                vec![0.0]
            }
            4 => vec![self.state.facade_calls as f64],
            k => unreachable!("page_pin harness: unknown op kind {k}"),
        }
    }
}
