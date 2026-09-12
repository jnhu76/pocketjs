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

/// Wall-clock epoch microseconds — joins A3 events with FRAME_TRACE lines
/// (which carry the same clock) for first-image timestamp correlation.
fn epoch_us() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
}

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

/// EXIF orientation → display transform (EXIF 2.32 tag 0x0112 table),
/// implemented as rotate-then-flip-horizontal:
///   2 mirror-H = flipH ∘ R0            5 transpose  = flipH ∘ R90
///   3 rotate 180 = R180                7 transverse = flipH ∘ R270
///   4 flip-V = flipH ∘ R180            6 rotate 90 CW  = R90
///                                      8 rotate 270 CW = R270
/// Undefined values fall back to orientation 1.
fn exif_orientation_transform(value: u16) -> PlaneTransform {
    match value {
        2 => PlaneTransform { rotate: QuarterTurn::R0, flip_h: true },
        3 => PlaneTransform { rotate: QuarterTurn::R180, flip_h: false },
        4 => PlaneTransform { rotate: QuarterTurn::R180, flip_h: true },
        5 => PlaneTransform { rotate: QuarterTurn::R90, flip_h: true },
        6 => PlaneTransform { rotate: QuarterTurn::R90, flip_h: false },
        7 => PlaneTransform { rotate: QuarterTurn::R270, flip_h: true },
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
    /// Per-stage decode diagnostics, microseconds:
    /// [container+frame, metadata, decode-copy (incl. orientation pull)].
    stages_us: [u128; 3],
}

/// Result of probing the decoder's own source-transform scaling path
/// (A4): `supported` reports whether the inbox decoder exposes it, and
/// `native_*` the closest size it would actually produce (stored space).
pub(crate) struct SourceTransformProbe {
    native_w: u32,
    native_h: u32,
    supported: bool,
}

/// A scaled decode: the plane is oriented (display space), `native_*` is
/// the closest stored-space size the decoder reported, and
/// `via_source_transform` distinguishes the decoder's own DCT scaling
/// from the generic `IWICBitmapScaler` fallback.
pub(crate) struct ScaledDecode {
    decoded: A3Decoded,
    native_w: u32,
    native_h: u32,
    via_source_transform: bool,
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
    use super::{A3DecodeError, A3Decoded, PlaneTransform, QuarterTurn, ScaledDecode, SourceTransformProbe};
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

    /// Sniff + stream + forced-JPEG decoder + frame 0, shared by every
    /// decode entry (full, scaled, probe).
    fn open_frame(
        bytes: &[u8],
    ) -> Result<(w::IWICImagingFactory, w::IWICBitmapFrameDecode), A3DecodeError> {
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
        Ok((f, frame))
    }

    /// Apply the orientation transform to an already-materialized plane
    /// via a memory-backed IWICBitmap (never over the stream decoder —
    /// column strips from the decoder re-decode the same MCU rows).
    fn orient_plane(
        f: &w::IWICImagingFactory,
        pixels: Vec<u8>,
        w: u32,
        h: u32,
        transform: super::PlaneTransform,
    ) -> Result<(Vec<u8>, u32, u32), A3DecodeError> {
        if transform.rotate == super::QuarterTurn::R0 && !transform.flip_h {
            return Ok((pixels, w, h));
        }
        let format = w::GUID_WICPixelFormat32bppRGBA;
        let stride = w.checked_mul(4).ok_or(A3DecodeError::TooLarge)?;
        let bitmap: w::IWICBitmap = unsafe {
            f.CreateBitmapFromMemory(w, h, &format, stride, &pixels)
        }
        .map_err(|_| A3DecodeError::Corrupt)?;
        let mut source: w::IWICBitmapSource =
            bitmap.cast::<w::IWICBitmapSource>().map_err(|_| A3DecodeError::Corrupt)?;
        if transform.rotate != super::QuarterTurn::R0 {
            let option = match transform.rotate {
                super::QuarterTurn::R90 => w::WICBitmapTransformRotate90,
                super::QuarterTurn::R180 => w::WICBitmapTransformRotate180,
                super::QuarterTurn::R270 => w::WICBitmapTransformRotate270,
                super::QuarterTurn::R0 => unreachable!("excluded above"),
            };
            source = flip_rotator(f, &source, option)?;
        }
        if transform.flip_h {
            source = flip_rotator(f, &source, w::WICBitmapTransformFlipHorizontal)?;
        }
        let (mut rw, mut rh) = (0u32, 0u32);
        unsafe { source.GetSize(&mut rw, &mut rh) }.map_err(|_| A3DecodeError::Corrupt)?;
        let plane = super::checked_plane_bytes(rw, rh)?;
        let stride_rot = rw.checked_mul(4).ok_or(A3DecodeError::TooLarge)?;
        let mut out = vec![0u8; plane as usize];
        unsafe { source.CopyPixels(std::ptr::null(), stride_rot, &mut out) }
            .map_err(|_| A3DecodeError::Corrupt)?;
        Ok((out, rw, rh))
    }

    /// Pull a 32bppRGBA plane out of any bitmap source via a format
    /// converter (the decoder's fast native conversion path); used by the
    /// full decode, the scaler fallback, and as the pre-transform step.
    pub(super) fn copy_plane(
        f: &w::IWICImagingFactory,
        source: &w::IWICBitmapSource,
    ) -> Result<(Vec<u8>, u32, u32), A3DecodeError> {
        let converter = unsafe { f.CreateFormatConverter() }.map_err(|_| A3DecodeError::Corrupt)?;
        unsafe {
            converter.Initialize(
                source,
                &w::GUID_WICPixelFormat32bppRGBA,
                w::WICBitmapDitherTypeNone,
                None,
                0.0,
                w::WICBitmapPaletteTypeCustom,
            )
        }
        .map_err(|_| A3DecodeError::Corrupt)?;
        let (mut cw, mut ch) = (0u32, 0u32);
        unsafe { converter.GetSize(&mut cw, &mut ch) }.map_err(|_| A3DecodeError::Corrupt)?;
        let plane = super::checked_plane_bytes(cw, ch)?;
        let stride = cw.checked_mul(4).ok_or(A3DecodeError::TooLarge)?;
        let mut pixels = vec![0u8; plane as usize];
        unsafe { converter.CopyPixels(std::ptr::null(), stride, &mut pixels) }
            .map_err(|_| A3DecodeError::Corrupt)?;
        Ok((pixels, cw, ch))
    }

    pub(super) fn decode_jpeg(bytes: &[u8]) -> Result<A3Decoded, A3DecodeError> {
        let all = std::time::Instant::now();
        let mut stages_us = [0u128; 3];
        let (f, frame) = open_frame(bytes)?;
        stages_us[0] = all.elapsed().as_micros();
        let orientation = read_orientation(&frame);
        stages_us[1] = all.elapsed().as_micros() - stages_us[0];

        // Native orientation. IMPORTANT: the transform is applied to a
        // MATERIALIZED memory bitmap, never directly over the stream
        // decoder — a rotator pulling column strips from the JPEG decoder
        // re-decodes the same MCU rows per output row (measured ~2.2 s for
        // a 1.2 MP frame vs ~20 ms from memory). Decode first on the
        // decoder's fast native path, then rotate in memory.
        let transform: PlaneTransform = super::exif_orientation_transform(orientation);
        let frame_source: w::IWICBitmapSource =
            frame.cast::<w::IWICBitmapSource>().map_err(|_| A3DecodeError::Corrupt)?;
        let copy_start = all.elapsed().as_micros();
        let (pixels, w_out, h_out) = copy_plane(&f, &frame_source)?;
        let (pixels, w_out, h_out) = orient_plane(&f, pixels, w_out, h_out, transform)?;
        stages_us[2] = all.elapsed().as_micros() - copy_start;
        Ok(A3Decoded { pixels, w: w_out, h: h_out, orientation, stages_us })
    }

    /// A4: query the inbox decoder's own source-transform scaling path.
    /// `GetClosestSize` is in/out — desired size on input, closest
    /// supported (native DCT) size on output.
    pub(super) fn probe_source_transform(
        bytes: &[u8],
        want_w: u32,
        want_h: u32,
    ) -> Result<SourceTransformProbe, A3DecodeError> {
        let (_, frame) = open_frame(bytes)?;
        let transform = frame.cast::<w::IWICBitmapSourceTransform>().ok();
        let Some(t) = transform else {
            let (mut fw, mut fh) = (0u32, 0u32);
            unsafe { frame.GetSize(&mut fw, &mut fh) }.map_err(|_| A3DecodeError::Corrupt)?;
            return Ok(SourceTransformProbe { native_w: fw, native_h: fh, supported: false });
        };
        let (mut nw, mut nh) = (want_w.max(1), want_h.max(1));
        unsafe { t.GetClosestSize(&mut nw, &mut nh) }.map_err(|_| A3DecodeError::Corrupt)?;
        Ok(SourceTransformProbe { native_w: nw, native_h: nh, supported: true })
    }

    /// A4: decode at viewport-appropriate resolution. The request is in
    /// DISPLAY (oriented) space and inverted to stored space; the decoder's
    /// own DCT scaling produces the closest native size, and the generic
    /// `IWICBitmapScaler` (streaming — the full plane is never materialized)
    /// is the fallback if the source-transform path is unavailable.
    pub(super) fn decode_jpeg_scaled(
        bytes: &[u8],
        display_w: u32,
        display_h: u32,
    ) -> Result<ScaledDecode, A3DecodeError> {
        let all = std::time::Instant::now();
        if display_w == 0 || display_h == 0 {
            return Err(A3DecodeError::Corrupt);
        }
        let (f, frame) = open_frame(bytes)?;
        let orientation = read_orientation(&frame);
        let transform: PlaneTransform = super::exif_orientation_transform(orientation);
        // Invert the orientation for the stored-space request: odd
        // quarter-turns swap the axes between stored and display space.
        let (req_w, req_h) = if matches!(transform.rotate, QuarterTurn::R90 | QuarterTurn::R270) {
            (display_h, display_w)
        } else {
            (display_w, display_h)
        };

        let frame_source: w::IWICBitmapSource =
            frame.cast::<w::IWICBitmapSource>().map_err(|_| A3DecodeError::Corrupt)?;
        let source_transform = frame.cast::<w::IWICBitmapSourceTransform>().ok();

        // Preferred path: the decoder's own DCT scaling. CopyPixels on the
        // transform interface only accepts the codec's NATIVE pixel format
        // (GetClosestPixelFormat; E_INVALIDARG otherwise), so decode in
        // that format and normalize to RGBA through a WIC converter over
        // the small scaled plane.
        let mut via_st: Option<(Vec<u8>, u32, u32)> = None;
        let mut native = (req_w, req_h);
        if let Some(t) = source_transform.as_ref() {
            let (mut nw, mut nh) = (req_w, req_h);
            if unsafe { t.GetClosestSize(&mut nw, &mut nh) }.is_ok() {
                let mut fmt = w::GUID_WICPixelFormat32bppBGRA;
                unsafe { t.GetClosestPixelFormat(&mut fmt) }.ok();
                let bpp: u32 = if fmt == w::GUID_WICPixelFormat32bppBGRA
                    || fmt == w::GUID_WICPixelFormat32bppRGBA
                {
                    4
                } else if fmt == w::GUID_WICPixelFormat24bppBGR {
                    3
                } else {
                    0
                };
                if bpp > 0 {
                    let cap = super::checked_plane_bytes(nw, nh)?;
                    let stride = nw.checked_mul(bpp).ok_or(A3DecodeError::TooLarge)?;
                    let len = (stride as u64) * (nh as u64);
                    if len > cap {
                        return Err(A3DecodeError::TooLarge);
                    }
                    let mut raw = vec![0u8; len as usize];
                    unsafe {
                        t.CopyPixels(
                            std::ptr::null(),
                            nw,
                            nh,
                            &fmt,
                            w::WICBitmapTransformRotate0,
                            stride,
                            &mut raw,
                        )
                    }
                    .map_err(|_| A3DecodeError::Corrupt)?;
                    if fmt == w::GUID_WICPixelFormat32bppRGBA {
                        via_st = Some((raw, nw, nh));
                    } else {
                        let bitmap: w::IWICBitmap = unsafe {
                            f.CreateBitmapFromMemory(nw, nh, &fmt, stride, &raw)
                        }
                        .map_err(|_| A3DecodeError::Corrupt)?;
                        let src = bitmap
                            .cast::<w::IWICBitmapSource>()
                            .map_err(|_| A3DecodeError::Corrupt)?;
                        let (pixels, w, h) = copy_plane(&f, &src)?;
                        via_st = Some((pixels, w, h));
                    }
                    native = (nw, nh);
                }
            }
        }

        let (pixels, sw, sh, native_w, native_h, via_source_transform) = if let Some(
            (pixels, w, h),
        ) = via_st
        {
            (pixels, w, h, native.0, native.1, true)
        } else {
            // Fallback: streaming scaler over the converter (native WIC
            // downscale; the full-resolution plane is never materialized).
            // Clamp so we never request upscaling beyond the source size.
            let (mut fw, mut fh) = (0u32, 0u32);
            unsafe { frame.GetSize(&mut fw, &mut fh) }.map_err(|_| A3DecodeError::Corrupt)?;
            let (dw, dh) = (req_w.min(fw).max(1), req_h.min(fh).max(1));
            let converter = unsafe { f.CreateFormatConverter() }
                .map_err(|_| A3DecodeError::Corrupt)?;
            unsafe {
                converter.Initialize(
                    &frame_source,
                    &w::GUID_WICPixelFormat32bppRGBA,
                    w::WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    w::WICBitmapPaletteTypeCustom,
                )
            }
            .map_err(|_| A3DecodeError::Corrupt)?;
            let scaler = unsafe { f.CreateBitmapScaler() }.map_err(|_| A3DecodeError::Corrupt)?;
            unsafe {
                scaler.Initialize(
                    &converter.cast::<w::IWICBitmapSource>().map_err(|_| A3DecodeError::Corrupt)?,
                    dw,
                    dh,
                    w::WICBitmapInterpolationModeLinear,
                )
            }
            .map_err(|_| A3DecodeError::Corrupt)?;
            let scaler_source: w::IWICBitmapSource =
                scaler.cast::<w::IWICBitmapSource>().map_err(|_| A3DecodeError::Corrupt)?;
            let (pixels, sw, sh) = copy_plane(&f, &scaler_source)?;
            (pixels, sw, sh, fw, fh, false)
        };

        let (pixels, w_out, h_out) = orient_plane(&f, pixels, sw, sh, transform)?;
        let _ = all;
        Ok(ScaledDecode {
            decoded: A3Decoded {
                pixels,
                w: w_out,
                h: h_out,
                orientation,
                stages_us: [0, 0, 0],
            },
            native_w,
            native_h,
            via_source_transform,
        })
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

#[cfg(windows)]
fn decode_jpeg_wic_scaled(
    bytes: &[u8],
    display_w: u32,
    display_h: u32,
) -> Result<ScaledDecode, A3DecodeError> {
    wic::decode_jpeg_scaled(bytes, display_w, display_h)
}

#[cfg(not(windows))]
fn decode_jpeg_wic_scaled(
    _bytes: &[u8],
    _display_w: u32,
    _display_h: u32,
) -> Result<ScaledDecode, A3DecodeError> {
    Err(A3DecodeError::DecoderUnavailable)
}

#[cfg(windows)]
fn probe_source_transform(
    bytes: &[u8],
    want_w: u32,
    want_h: u32,
) -> Result<SourceTransformProbe, A3DecodeError> {
    wic::probe_source_transform(bytes, want_w, want_h)
}

#[cfg(not(windows))]
fn probe_source_transform(
    _bytes: &[u8],
    _want_w: u32,
    _want_h: u32,
) -> Result<SourceTransformProbe, A3DecodeError> {
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
    fn observe_rx(&mut self, surface: &UiSurface, line: &str, tick: u64) {
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
                    return self.announce_error(surface, "?", "bad_request", tick);
                }
                match value["path"].as_str() {
                    Some(path) => self.handle_open(surface, &req, path.into(), tick, None),
                    None => self.announce_error(surface, &req, "bad_request", tick),
                }
            }
            Some("a4open") => {
                // A4 view request: like a3open, with optional Fit target
                // (display-space box). No fit dims = 100% / full decode.
                let req = value["req"].as_str().unwrap_or("").to_string();
                if req.is_empty() {
                    return self.announce_error(surface, "?", "bad_request", tick);
                }
                let fit = match (
                    value["fitW"].as_u64(),
                    value["fitH"].as_u64(),
                ) {
                    (Some(w), Some(h)) if w > 0 && h > 0 => Some((w.min(8192) as u32, h.min(8192) as u32)),
                    (None, None) => None,
                    _ => {
                        return self.announce_error(surface, &req, "bad_request", tick);
                    }
                };
                match value["path"].as_str() {
                    Some(path) => self.handle_open(surface, &req, path.into(), tick, fit),
                    None => self.announce_error(surface, &req, "bad_request", tick),
                }
            }
            Some("a4ack") | Some("a3ack") => {
                eprintln!(
                    "A3EVENT,ack,req={},bound={},geom={},tick={tick},epochUs={}",
                    value["req"],
                    value["bound"].as_str().unwrap_or(""),
                    value["geom"],
                    epoch_us()
                );
            }
            _ => {}
        }
    }

    fn handle_open(
        &mut self,
        surface: &UiSurface,
        req: &str,
        path: std::path::PathBuf,
        tick: u64,
        fit: Option<(u32, u32)>,
    ) {
        let all = Instant::now();
        let mode = if fit.is_some() { "fit" } else { "full" };
        eprintln!(
            "A3EVENT,open,req={req},path={},mode={mode},fit={},tick={tick},epochUs={}",
            path.display(),
            fit.map_or(String::new(), |(w, h)| format!("{w}x{h}")),
            epoch_us()
        );
        // T3_SOURCE_OPEN_BEGIN..file-in-memory: the source file handle is
        // closed the moment `read` returns — never held during decode and
        // never held merely because the image stays visible.
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return self.announce_error(surface, req, "missing", tick);
            }
            Err(_) => {
                return self.announce_error(surface, req, "unreadable", tick);
            }
        };
        let open_us = all.elapsed().as_micros();
        let (decoded, native_w, native_h, via_source_transform, decode_us) = {
            let start = Instant::now();
            match fit {
                Some((fw, fh)) => match decode_jpeg_wic_scaled(&bytes, fw, fh) {
                    Ok(out) => {
                        let native = (out.native_w, out.native_h);
                        let via = out.via_source_transform;
                        (out.decoded, native.0, native.1, via, start.elapsed().as_micros())
                    }
                    Err(error) => {
                        return self.announce_error(surface, req, error.code(), tick);
                    }
                },
                None => match decode_jpeg_wic(&bytes) {
                    Ok(decoded) => {
                        let full = (decoded.w, decoded.h);
                        (decoded, full.0, full.1, true, start.elapsed().as_micros())
                    }
                    Err(error) => {
                        return self.announce_error(surface, req, error.code(), tick);
                    }
                },
            }
        };
        let plane = decoded.pixels.len();
        let (w, h, orientation) = (decoded.w, decoded.h, decoded.orientation);
        let stages = decoded.stages_us;
        eprintln!(
            "A3STAGES,req={req},containerFrameUs={},metadataUs={},pixelsUs={}",
            stages[0], stages[1], stages[2]
        );
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
            return self.announce_error(surface, req, "rejected", tick);
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
            "A3EVENT,img,req={req},handle={handle},w={w},h={h},orient={orientation},plane={plane},liveBytes={live_bytes},mode={mode},nativeW={native_w},nativeH={native_h},via={via},openUs={open_us},decodeUs={decode_us},registerUs={register_us},retiredReq={retired_req},totalUs={},tick={tick},epochUs={}",
            all.elapsed().as_micros(),
            epoch_us(),
            via = if via_source_transform { "source_transform" } else { "scaler" }
        );
        self.push(
            surface,
            &json!({"t": "a3img", "req": req, "handle": handle, "w": w, "h": h, "orient": orientation,
                    "mode": mode, "nativeW": native_w, "nativeH": native_h})
                .to_string(),
        );
        self.boundary(plane);
    }

    fn announce_error(&mut self, surface: &UiSurface, req: &str, code: &str, tick: u64) {
        self.failures += 1;
        let current_plane = self.live.as_ref().map_or(0, |(_, _, plane)| *plane);
        eprintln!("A3EVENT,error,req={req},code={code},tick={tick},epochUs={}", epoch_us());
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
