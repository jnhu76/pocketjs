//! OS clipboard as a platform abstraction.
//!
//! The desktop host previously talked to `pbcopy`/`pbpaste` directly
//! (macOS-only, warning on other platforms). This crate owns the system
//! boundary instead:
//!
//! - macOS: `pbcopy`/`pbpaste` (unchanged behavior — the zero-dependency
//!   road stays the macOS road).
//! - Windows: the Win32 clipboard (CF_UNICODETEXT, so UTF-16; all Unicode
//!   round-trips, including CJK and multiline).
//! - other platforms: unsupported — callers get a clear result instead of
//!   a silent log line.
//!
//! The API is deliberately the smallest thing a text editor needs: put
//! text on, take text off. A future Linux backend slots in behind the same
//! two functions.

use std::io;

/// Copy `text` to the system clipboard. Returns Ok when the platform has a
/// backend and the copy succeeded. An empty string is a no-op that leaves
/// the clipboard untouched.
pub fn copy(text: &str) -> io::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        let mut child = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        child.stdin.as_mut().unwrap().write_all(text.as_bytes())?;
        child.wait()?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        windows::copy(text)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "clipboard copy unsupported on this platform",
        ))
    }
}

/// Read the system clipboard as a UTF-8 string. `None` when the clipboard
/// holds no text (or the platform has no backend).
pub fn paste() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("pbpaste").output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }
    #[cfg(target_os = "windows")]
    {
        windows::paste()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::io;
    use std::time::Duration;

    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };

    /// CF_UNICODETEXT — clipboard data as UTF-16, NUL-terminated.
    const CF_UNICODETEXT: u32 = 13;

    fn to_io(e: windows::core::Error) -> io::Error {
        io::Error::other(e.message())
    }

    /// OpenClipboard fails while another process owns the clipboard; the
    /// docs recommend retrying. Bounded so a wedged peer cannot hang a
    /// paste.
    fn open_clipboard_with_retry() -> io::Result<()> {
        for _ in 0..8 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        Err(io::Error::other("clipboard is busy"))
    }

    pub(super) fn copy(text: &str) -> io::Result<()> {
        // The clipboard is a process-shared resource; every step that can
        // fail must be checked or the window could hold the lock forever.
        unsafe {
            open_clipboard_with_retry()?;
            let result = (|| -> io::Result<()> {
                EmptyClipboard().map_err(to_io)?;
                // UTF-16 including the NUL terminator, in bytes.
                let mut units: Vec<u16> = text.encode_utf16().collect();
                units.push(0);
                let bytes = units.len() * 2;
                let mem = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(to_io)?;
                let ptr = GlobalLock(mem);
                if ptr.is_null() {
                    let _ = GlobalFree(mem);
                    return Err(io::Error::other("GlobalLock failed"));
                }
                std::ptr::copy_nonoverlapping(units.as_ptr() as *const u8, ptr.cast(), bytes);
                // GlobalUnlock's BOOL is FALSE exactly when the object
                // becomes fully unlocked — the success case — so windows-rs
                // turns the normal path into an Err(ERROR_SUCCESS). The
                // handle is ours (or the system's, after SetClipboardData),
                // so the return is meaningless here.
                let _ = GlobalUnlock(mem);
                match SetClipboardData(CF_UNICODETEXT, HANDLE(mem.0)) {
                    // Success: the system owns `mem` now.
                    Ok(_) => Ok(()),
                    // Failure: the transfer never happened; reclaim the
                    // allocation so it cannot leak.
                    Err(e) => {
                        let _ = GlobalFree(mem);
                        Err(to_io(e))
                    }
                }
            })();
            let close = CloseClipboard();
            match (result, close) {
                (Err(e), _) => Err(e),
                (Ok(()), Err(e)) => Err(to_io(e)),
                (Ok(()), Ok(())) => Ok(()),
            }
        }
    }

    pub(super) fn paste() -> Option<String> {
        unsafe {
            open_clipboard_with_retry().ok()?;
            let handle = match GetClipboardData(CF_UNICODETEXT) {
                Ok(h) if !h.is_invalid() => h,
                _ => {
                    let _ = CloseClipboard();
                    return None;
                }
            };
            let mem = HGLOBAL(handle.0);
            let ptr = GlobalLock(mem);
            if ptr.is_null() {
                let _ = CloseClipboard();
                return None;
            }
            // Bound the read by the allocation size — never walk past the
            // clipboard block, whatever its contents claim.
            let units = GlobalSize(mem) / 2;
            let slice = std::slice::from_raw_parts(ptr.cast::<u16>(), units);
            let end = slice.iter().position(|&u| u == 0).unwrap_or(slice.len());
            let text = String::from_utf16(&slice[..end]).ok();
            let _ = GlobalUnlock(mem);
            let _ = CloseClipboard();
            text
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::Mutex;

        /// The clipboard is process-global — the harness runs tests in
        /// parallel threads, so every test serializes through this lock.
        static CLIP: Mutex<()> = Mutex::new(());

        fn clear() {
            unsafe {
                let _ = OpenClipboard(None);
                let _ = EmptyClipboard();
                let _ = CloseClipboard();
            }
        }

        #[test]
        fn copy_paste_round_trips_ascii() {
            let _guard = CLIP.lock().unwrap();
            clear();
            copy("hello clipboard").unwrap();
            assert_eq!(paste().as_deref(), Some("hello clipboard"));
        }

        #[test]
        fn copy_paste_round_trips_cjk() {
            let _guard = CLIP.lock().unwrap();
            clear();
            copy("你好，世界").unwrap();
            assert_eq!(paste().as_deref(), Some("你好，世界"));
        }

        #[test]
        fn copy_paste_round_trips_multiline() {
            let _guard = CLIP.lock().unwrap();
            clear();
            copy("line one\nline two\n中文行").unwrap();
            assert_eq!(paste().as_deref(), Some("line one\nline two\n中文行"));
        }

        #[test]
        fn empty_clipboard_pastes_none() {
            let _guard = CLIP.lock().unwrap();
            clear();
            assert_eq!(paste(), None);
        }

        #[test]
        fn empty_copy_leaves_clipboard_alone() {
            let _guard = CLIP.lock().unwrap();
            clear();
            copy("hello clipboard").unwrap();
            copy("").unwrap();
            assert_eq!(paste().as_deref(), Some("hello clipboard"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_copy_is_a_noop_everywhere() {
        assert!(copy("").is_ok());
    }

    #[test]
    fn unsupported_platforms_report_clearly() {
        // On platforms without a backend the gate must say so — the macOS
        // note host previously swallowed this as a bare warning.
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let err = copy("x").unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
            assert_eq!(paste(), None);
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let _ = copy("x");
        }
    }
}