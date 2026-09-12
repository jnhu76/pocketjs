// ---------------------------------------------------------------------------
// A3 WIC-first JPEG proof harness (desktop-host test material).
//
// The guest sends a bounded intent — {"t":"a3open","req","path"} — over the
// existing svc channel; the host reads the file (closing it before decode),
// decodes through Windows Imaging Component, applies EXIF orientation
// NATIVELY, registers the RGBA plane through the A2 image-resource seam
// (`Ui::register_native_texture`), synchronously retires the replaced
// resource, and announces ONLY bounded semantic state:
// {"t":"a3img","req","handle","w","h","orient"} or
// {"t":"a3error","req","code"}.
//
// No encoded byte and no decoded pixel ever crosses QuickJS. Admission
// (native dimension bound + overflow-checked byte length) runs before the
// plane is allocated. Enabled by `--a3-harness` + `--a3-file`; off by
// default and absent from stock app paths.
// ---------------------------------------------------------------------------

const A3_SERVICE: &str = "picoview-a3";

/// EXIF `Orientation` quarter-turn component (clockwise).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum QuarterTurn {
    R0,
    R90,
    R180,
    R270,
}

/// Native plane transform, applied rotate-then-flip-horizontal so two
/// chained `IWICBitmapFlipRotator`s express every EXIF orientation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct PlaneTransform {
    rotate: QuarterTurn,
    flip_h: bool,
}

/// EXIF orientation → display transform (EXIF 2.32 tag 0x0112 table).
/// Undefined values fall back to orientation 1.
fn exif_orientation_transform(value: u16) -> PlaneTransform {
    match value {
        2 => PlaneTransform { rotate: QuarterTurn::R0, flip_h: true },
        3 => PlaneTransform { rotate: QuarterTurn::R180, flip_h: false },
        4 => PlaneTransform { rotate: QuarterTurn::R180, flip_h: true },
        5 => PlaneTransform { rotate: QuarterTurn::R270, flip_h: true },
        6 => PlaneTransform { rotate: QuarterTurn::R90, flip_h: false },
        7 => PlaneTransform { rotate: QuarterTurn::R90, flip_h: true },
        8 => PlaneTransform { rotate: QuarterTurn::R270, flip_h: false },
        _ => PlaneTransform { rotate: QuarterTurn::R0, flip_h: false },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum A3DecodeError {
    /// Container is not JPEG (content masquerading under a JPEG job).
    NotJpeg,
    /// Container parses as JPEG but pixels cannot be produced.
    Corrupt,
    /// Post-transform plane exceeds the native admission bound.
    TooLarge,
    /// WIC/COM is unavailable on this host platform.
    DecoderUnavailable,
}

impl A3DecodeError {
    fn code(self) -> &'static str {
        match self {
            Self::NotJpeg => "not_jpeg",
            Self::Corrupt => "corrupt",
            Self::TooLarge => "too_large",
            Self::DecoderUnavailable => "decoder_unavailable",
        }
    }
}

struct A3Decoded {
    pixels: Vec<u8>,
    w: u32,
    h: u32,
    orientation: u16,
}

/// Admission check for a decoded RGBA plane, run BEFORE allocation: the
/// core's native dimension bound (`NATIVE_TEX_MAX_DIM`) per axis and an
/// overflow-checked byte length.
fn checked_plane_bytes(w: u32, h: u32) -> Result<u64, A3DecodeError> {
    if w == 0 || h == 0 {
        return Err(A3DecodeError::Corrupt);
    }
    if w > pocketjs_core::NATIVE_TEX_MAX_DIM || h > pocketjs_core::NATIVE_TEX_MAX_DIM {
        return Err(A3DecodeError::TooLarge);
    }
    (w as u64)
        .checked_mul(h as u64)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(A3DecodeError::TooLarge)
}

#[cfg(windows)]
mod wic {
    use super::{A3DecodeError, A3Decoded, PlaneTransform, QuarterTurn};
    use windows::core::Interface;
    use windows::Win32::Graphics::Imaging as w;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Variant::VT_UI2;

    thread_local! {
        /// Process-lifetime COM init: once per thread, never uninitialized.
        static COM_INIT: () = {
            unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
        };
        static FACTORY: std::cell::RefCell<Option<w::IWICImagingFactory>> =
            const { std::cell::RefCell::new(None) };
    }

    fn factory() -> Result<w::IWICImagingFactory, A3DecodeError> {
        COM_INIT.with(|_| {});
        FACTORY.with(|cell| {
            if let Some(existing) = cell.borrow().clone() {
                return Ok(existing);
            }
            let created: w::IWICImagingFactory = unsafe {
                CoCreateInstance(&w::CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
            }
            .map_err(|_| A3DecodeError::DecoderUnavailable)?;
            *cell.borrow_mut() = Some(created.clone());
            Ok(created)
        })
    }

    /// EXIF orientation from the frame's App1/IFD0/EXIF metadata; absent or
    /// malformed metadata is orientation 1, never a decode failure.
    fn read_orientation(frame: &w::IWICBitmapFrameDecode) -> u16 {
        const QUERY: windows::core::PCWSTR =
            windows::core::w!("/app1/ifd/exif/{ushort=274}");
        let Ok(reader) = (unsafe { frame.GetMetadataQueryReader() }) else {
            return 1;
        };
        let mut value = PROPVARIANT::default();
        if unsafe { reader.GetMetadataByName(QUERY, &mut value) }.is_err() {
            return 1;
        }
        // SAFETY: union field reads guarded by the VARENUM tag.
        if unsafe { value.Anonymous.Anonymous.vt } == VT_UI2 {
            unsafe { value.Anonymous.Anonymous.Anonymous.uiVal }
        } else {
            1
        }
    }

    fn flip_rotator(
        f: &w::IWICImagingFactory,
        source: &w::IWICBitmapSource,
        transform: w::WICBitmapTransformOptions,
    ) -> Result<w::IWICBitmapSource, A3DecodeError> {
        let rotator = unsafe { f.CreateBitmapFlipRotator() }.map_err(|_| A3DecodeError::Corrupt)?;
        unsafe { rotator.Initialize(source, transform) }.map_err(|_| A3DecodeError::Corrupt)?;
        rotator.cast::<w::IWICBitmapSource>().map_err(|_| A3DecodeError::Corrupt)
    }

    pub(super) fn decode_jpeg(bytes: &[u8]) -> Result<A3Decoded, A3DecodeError> {
        // Content sniff: a JPEG stream starts with SOI. Anything else under
        // a JPEG job (PNG bytes, text, emptiness) is rejected before any
        // decoder runs.
        if bytes.len() < 2 || bytes[0] != 0xff || bytes[1] != 0xd8 {
            return Err(A3DecodeError::NotJpeg);
        }
        let f = factory()?;
        // The source file handle is already closed by the caller; decode
        // runs purely from the in-memory encoded bytes.
        let stream: w::IWICStream =
            unsafe { f.CreateStream() }.map_err(|_| A3DecodeError::DecoderUnavailable)?;
        unsafe { stream.InitializeFromMemory(bytes) }.map_err(|_| A3DecodeError::Corrupt)?;
        // The JPEG container GUID is forced explicitly, so the decode can
        // never silently fall through to another installed codec.
        let decoder: w::IWICBitmapDecoder = unsafe {
            f.CreateDecoder(&w::GUID_ContainerFormatJpeg, std::ptr::null())
        }
        .map_err(|_| A3DecodeError::DecoderUnavailable)?;
        unsafe { decoder.Initialize(&stream, w::WICDecodeMetadataCacheOnDemand) }
            .map_err(|_| A3DecodeError::Corrupt)?;
        let frame = unsafe { decoder.GetFrame(0) }.map_err(|_| A3DecodeError::Corrupt)?;
        let orientation = read_orientation(&frame);

        // Native orientation: rotate (if any), then horizontal flip (if any).
        let transform: PlaneTransform = super::exif_orientation_transform(orientation);
        let mut source: w::IWICBitmapSource =
            frame.cast::<w::IWICBitmapSource>().map_err(|_| A3DecodeError::Corrupt)?;
        if transform.rotate != QuarterTurn::R0 {
            let option = match transform.rotate {
                QuarterTurn::R90 => w::WICBitmapTransformRotate90,
                QuarterTurn::R180 => w::WICBitmapTransformRotate180,
                QuarterTurn::R270 => w::WICBitmapTransformRotate270,
                QuarterTurn::R0 => unreachable!("excluded above"),
            };
            source = flip_rotator(&f, &source, option)?;
        }
        if transform.flip_h {
            source = flip_rotator(&f, &source, w::WICBitmapTransformFlipHorizontal)?;
        }

        // Single native conversion to the core's PSM_8888 byte order
        // (R,G,B,A in memory); no guest-side pixel rewrite exists.
        let converter = unsafe { f.CreateFormatConverter() }.map_err(|_| A3DecodeError::Corrupt)?;
        unsafe {
            converter.Initialize(
                &source,
                &w::GUID_WICPixelFormat32bppRGBA,
                w::WICBitmapDitherTypeNone,
                None,
                0.0,
                w::WICBitmapPaletteTypeCustom,
            )
        }
        .map_err(|_| A3DecodeError::Corrupt)?;

        let (mut w_out, mut h_out) = (0u32, 0u32);
        unsafe { converter.GetSize(&mut w_out, &mut h_out) }.map_err(|_| A3DecodeError::Corrupt)?;
        let plane = super::checked_plane_bytes(w_out, h_out)?;
        let stride = w_out.checked_mul(4).ok_or(A3DecodeError::TooLarge)?;
        let mut pixels = vec![0u8; plane as usize];
        unsafe { converter.CopyPixels(std::ptr::null(), stride, &mut pixels) }
            .map_err(|_| A3DecodeError::Corrupt)?;
        Ok(A3Decoded { pixels, w: w_out, h: h_out, orientation })
    }
}

#[cfg(windows)]
fn decode_jpeg_wic(bytes: &[u8]) -> Result<A3Decoded, A3DecodeError> {
    wic::decode_jpeg(bytes)
}

#[cfg(not(windows))]
fn decode_jpeg_wic(_bytes: &[u8]) -> Result<A3Decoded, A3DecodeError> {
    Err(A3DecodeError::DecoderUnavailable)
}

struct A3Harness {
    active: bool,
    files: Vec<std::path::PathBuf>,
    manifest_sent: bool,
    /// Current resource: (request id, generation-tagged handle, plane bytes).
    live: Option<(String, i32, usize)>,
    tx_lines: u64,
    tx_bytes: u64,
    rx_lines: u64,
    rx_bytes: u64,
    successes: u64,
    failures: u64,
    /// Exact host-pushed svc lines (bounded semantic state; mirrors the
    /// guest outbox for tests and the boundary audit).
    sent: Vec<String>,
}

impl A3Harness {
    fn new(active: bool, files: Vec<std::path::PathBuf>) -> Self {
        Self {
            active,
            files,
            manifest_sent: false,
            live: None,
            tx_lines: 0,
            tx_bytes: 0,
            rx_lines: 0,
            rx_bytes: 0,
            successes: 0,
            failures: 0,
            sent: Vec::new(),
        }
    }

    /// Boot: hand the guest its request manifest — path strings only, once.
    fn boot(&mut self, surface: &UiSurface) {
        if !self.active || self.manifest_sent || self.files.is_empty() {
            return;
        }
        self.manifest_sent = true;
        let files: Vec<String> = self
            .files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        self.push(surface, &json!({"t": "a3manifest", "files": files}).to_string());
    }

    /// Guest → host A3 traffic (open intents, acks): counted, logged, and
    /// (for open intents) answered — never forwarded as an app intent.
    fn observe_rx(&mut self, surface: &UiSurface, line: &str) {
        self.rx_lines += 1;
        self.rx_bytes += line.len() as u64 + 1;
        eprintln!("A3SVC,rx,{},{line}", line.len() + 1);
        if !self.active {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        match value["t"].as_str() {
            Some("a3open") => {
                let req = value["req"].as_str().unwrap_or("").to_string();
                if req.is_empty() {
                    return self.announce_error(surface, "?", "bad_request");
                }
                match value["path"].as_str() {
                    Some(path) => self.handle_open(surface, &req, path.into()),
                    None => self.announce_error(surface, &req, "bad_request"),
                }
            }
            Some("a3ack") => {
                eprintln!(
                    "A3EVENT,ack,req={},bound={}",
                    value["req"],
                    value["bound"].as_str().unwrap_or("")
                );
            }
            _ => {}
        }
    }

    fn handle_open(&mut self, surface: &UiSurface, req: &str, path: std::path::PathBuf) {
        let all = Instant::now();
        eprintln!("A3EVENT,open,req={req},path={}", path.display());
        // T3_SOURCE_OPEN_BEGIN..file-in-memory: the source file handle is
        // closed the moment `read` returns — never held during decode and
        // never held merely because the image stays visible.
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return self.announce_error(surface, req, "missing");
            }
            Err(_) => {
                return self.announce_error(surface, req, "unreadable");
            }
        };
        let open_us = all.elapsed().as_micros();
        let (decoded, decode_us) = {
            let start = Instant::now();
            match decode_jpeg_wic(&bytes) {
                Ok(decoded) => (decoded, start.elapsed().as_micros()),
                Err(error) => return self.announce_error(surface, req, error.code()),
            }
        };
        let plane = decoded.pixels.len();
        let (w, h, orientation) = (decoded.w, decoded.h, decoded.orientation);
        let start = Instant::now();
        let (handle, live_bytes) = surface.with_ui(|ui| {
            (
                ui.register_native_texture(
                    &decoded.pixels,
                    w,
                    h,
                    pocketjs_core::spec::psm::PSM_8888,
                    true,
                ),
                ui.texture_live_bytes(),
            )
        });
        let register_us = start.elapsed().as_micros();
        // The decode plane and encoded bytes die here, on the native side.
        drop(decoded);
        drop(bytes);
        if handle < 0 {
            return self.announce_error(surface, req, "rejected");
        }
        // Synchronous native retirement of the replaced resource — the
        // generation-tagged slot goes stale immediately, no GC involved.
        let mut retired_req = String::new();
        if let Some((previous, previous_handle, _)) = self.live.take() {
            surface.with_ui(|ui| ui.free_texture(previous_handle));
            retired_req = previous;
        }
        self.live = Some((req.to_string(), handle, plane));
        self.successes += 1;
        eprintln!(
            "A3EVENT,img,req={req},handle={handle},w={w},h={h},orient={orientation},plane={plane},liveBytes={live_bytes},openUs={open_us},decodeUs={decode_us},registerUs={register_us},retiredReq={retired_req},totalUs={}",
            all.elapsed().as_micros()
        );
        self.push(
            surface,
            &json!({"t": "a3img", "req": req, "handle": handle, "w": w, "h": h, "orient": orientation})
                .to_string(),
        );
        self.boundary(plane);
    }

    fn announce_error(&mut self, surface: &UiSurface, req: &str, code: &str) {
        self.failures += 1;
        let current_plane = self.live.as_ref().map_or(0, |(_, _, plane)| *plane);
        eprintln!("A3EVENT,error,req={req},code={code}");
        self.push(
            surface,
            &json!({"t": "a3error", "req": req, "code": code}).to_string(),
        );
        self.boundary(current_plane);
    }

    /// Cumulative guest-boundary accounting after every handled request:
    /// traffic is bounded per resource/event and independent of pixel size.
    fn boundary(&self, current_plane: usize) {
        eprintln!(
            "A3BOUNDARY,successes={},failures={},txLines={},txBytes={},rxLines={},rxBytes={},totalBytes={},currentPlane={}",
            self.successes,
            self.failures,
            self.tx_lines,
            self.tx_bytes,
            self.rx_lines,
            self.rx_bytes,
            self.tx_bytes + self.rx_bytes,
            current_plane
        );
    }

    fn push(&mut self, surface: &UiSurface, line: &str) {
        surface.svc_push(line.to_string());
        self.sent.push(line.to_string());
        self.tx_lines += 1;
        self.tx_bytes += line.len() as u64 + 1; // + newline, per the batch contract
        eprintln!("A3SVC,tx,{},{line}", line.len() + 1);
    }
}
