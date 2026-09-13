//! WINDOWS-STARTUP-CALLPATH-REALITY-AUDIT-1 measurement-only hook.
//!
//! Lets the embedding host route stage-boundary stamps from inside this
//! vendored wgpu-hal onto the host's single monotonic E-series origin
//! (same pattern as pocket3d's `set_norm_mark`). Unset (the default)
//! costs one `OnceLock` load per boundary and prints nothing; no product
//! behavior depends on it. This module exists ONLY in the audit's vendored
//! copy and is never upstreamed.

use std::sync::OnceLock;

pub type NormMark = fn(&'static str);

static NORM_MARK: OnceLock<NormMark> = OnceLock::new();

pub fn set_norm_mark(mark: Option<NormMark>) {
    if let Some(mark) = mark {
        let _ = NORM_MARK.set(mark);
    }
}

pub(crate) fn norm_mark(event: &'static str) {
    if let Some(mark) = NORM_MARK.get() {
        mark(event);
    }
}
