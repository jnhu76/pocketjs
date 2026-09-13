// ---------------------------------------------------------------------------
// C2 staged residency probe (desktop-host measurement material).
//
// Prints one stderr line per call — `A7EVENT,mem,<stage>,wsPrivateBytes=<n>,
// privateBytes=<n>` — with a process private-residency snapshot from
// GetProcessMemoryInfo:
//   - wsPrivateBytes = PROCESS_MEMORY_COUNTERS_EX2.PrivateWorkingSetSize
//     (the "Working Set - Private" perf-counter quantity);
//   - privateBytes   = PagefileUsage (private committed bytes, the
//     "Private Bytes" perf-counter quantity).
//
// Inactive unless POCKET_MEM_STAGE=1; the off path is one boolean read and
// allocates nothing. Non-Windows builds compile to a no-op. Stage deltas are
// honest only where lifetime/order makes them attributable — attribution
// rules live in the PicoView corrective evidence report, not here.
// ---------------------------------------------------------------------------

/// Snapshot of the two private-memory quantities BENCHMARK §7 names.
#[derive(Clone, Copy, Debug)]
pub struct MemSnapshot {
    pub ws_private: u64,
    pub private_bytes: u64,
}

fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("POCKET_MEM_STAGE").is_ok_and(|v| v == "1"))
}

#[cfg(windows)]
pub fn stage(name: &str) {
    if !enabled() {
        return;
    }
    let Some(snapshot) = snapshot() else {
        return;
    };
    eprintln!(
        "A7EVENT,mem,{name},wsPrivateBytes={},privateBytes={}",
        snapshot.ws_private, snapshot.private_bytes
    );
}

#[cfg(not(windows))]
pub fn stage(_name: &str) {}

#[cfg(windows)]
fn snapshot() -> Option<MemSnapshot> {
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut counters = PROCESS_MEMORY_COUNTERS_EX2::default();
    // GetProcessMemoryInfo validates `cb`, so passing the EX2 layout with
    // its own size selects the EX2 fields on Windows 11.
    unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut _,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
        )
    }
    .ok()
    .map(|_| MemSnapshot {
        ws_private: counters.PrivateWorkingSetSize as u64,
        private_bytes: counters.PagefileUsage as u64,
    })
}
