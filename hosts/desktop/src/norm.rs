//! CROSS-OS-NORMALIZED-DESKTOP-STARTUP-1 measurement plumbing.
//!
//! One process-local monotonic origin (`E00_MAIN_ENTRY` = 0 us, stamped at
//! the first statement of the host's `main`), the experiment-local E-series
//! vocabulary, once-per-process stamps, and the `BENCHMARK_CONFIG` line.
//! Silent unless the run opts in with `NORMTRACE=1`; product operation
//! prints nothing and pays one relaxed atomic load per call site.
//!
//! Clock authority: every marker is `Instant::now() - ORIGIN` microseconds —
//! no wall-clock domain is ever mixed in. Stage durations are BEGIN/END
//! differences; completion-from-E00 is carried by the `us` field itself.
//!
//! This module is deliberately dependency-free so the arm A/B control
//! examples can include it verbatim (`#[path]`).

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static ORIGIN: OnceLock<Instant> = OnceLock::new();
static RUN_ID: OnceLock<String> = OnceLock::new();
static INITED: AtomicBool = AtomicBool::new(false);
static ENABLED: AtomicBool = AtomicBool::new(false);
static MARKED: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
static GUEST_ID: Mutex<Option<String>> = Mutex::new(None);
static CONFIG_DONE: AtomicBool = AtomicBool::new(false);
static FORCED_SCALE: OnceLock<Option<f64>> = OnceLock::new();

/// Anchor the monotonic origin. Must be the host process's first statement.
pub fn init() {
    let _ = ORIGIN.set(Instant::now());
    let _ = RUN_ID.set(std::env::var("NORMTRACE_RUN").unwrap_or_else(|_| "0".into()));
    ENABLED.store(
        matches!(std::env::var("NORMTRACE").as_deref(), Ok("1")),
        Ordering::Relaxed,
    );
    INITED.store(true, Ordering::Relaxed);
}

/// Stamp the FIRST occurrence of `event` (us from E00) on stderr as
/// `NORMTRACE,run=<id>,thread=<name>,event=<event>,us=<us>`.
pub fn once(event: &'static str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    {
        let mut marked = MARKED
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .unwrap();
        if !marked.insert(event) {
            return;
        }
    }
    emit(event);
}

fn emit(event: &str) {
    let us = ORIGIN.get().map(|o| o.elapsed().as_micros()).unwrap_or(0);
    let thread = std::thread::current();
    eprintln!(
        "NORMTRACE,run={},thread={},event={event},us={us}",
        RUN_ID.get().map(String::as_str).unwrap_or("0"),
        thread.name().unwrap_or("unnamed"),
    );
}

/// Record the guest artifact identity read by the runtime boot (arm C).
pub fn set_guest(app: &str, js: &[u8], pak: &[u8]) {
    let js_hash = fnv1a64(js);
    let pak_hash = fnv1a64(pak);
    *GUEST_ID.lock().unwrap() = Some(format!(
        "{app}|js_bytes={js_len}|js_fnv={js_hash:#018x}|pak_bytes={pak_len}|pak_fnv={pak_hash:#018x}",
        js_len = js.len(),
        pak_len = pak.len(),
    ));
}

pub fn guest_id() -> Option<String> {
    GUEST_ID.lock().unwrap().clone()
}

/// Emit the run's `BENCHMARK_CONFIG` line exactly once.
pub fn emit_config(json: &str) {
    if !CONFIG_DONE.swap(true, Ordering::Relaxed) {
        println!("BENCHMARK_CONFIG {json}");
    }
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Normalized-raster affordance: pin the winit scale factor override so the
/// logical viewport maps 1:1 onto physical client pixels on BOTH hosts
/// (no-op where the monitor already runs 1.0x). Measurement-only.
pub fn forced_scale() -> Option<f64> {
    *FORCED_SCALE.get_or_init(|| {
        std::env::var("POCKET_FORCE_SCALE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
    })
}
