//! Runtime font-atlas extension: IME input can commit ANY codepoint, and
//! the pak's baked atlases only cover what the build saw. Instead of
//! guessing a charset at build time (and shipping megabytes of hanzi the
//! user may never type), the host rasterizes missing glyphs from a system
//! CJK font on first sight, appends them to the slot's FONT ATLAS v3 blob
//! (spec.ts — cmap stays codepoint-sorted, coverage is gid-linear, so
//! appending is cheap), and reloads the slot through the spec
//! `loadFontAtlas` op. The renderer re-uploads a slot whose glyph count
//! moved; layout re-measures on the reload's dirty flag. Latin keeps its
//! baked Inter forms — only unseen codepoints go through the fallback.

use std::collections::HashSet;
use std::path::Path;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};

const FONT_MAGIC: u32 = 0x4146_4344; // 'DCFA' LE
const HEADER: usize = 16;
const CMAP_ENTRY: usize = 8;
/// Appended-glyph ceiling per slot — far above any real typing session,
/// well under the u16 gid space and GPU texture limits at 64 columns.
const MAX_GLYPHS: u16 = 6000;

/// Font px per slot — mirrors framework/compiler/tailwind.ts FONT_PX (slots 0..6 =
/// 12/14/16/18/20/24/36, bold = +7 at the same px). tests/note.test.ts pins
/// the same table.
fn slot_px(slot: u8) -> f32 {
    [12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 36.0][(slot % 7) as usize]
}

/// System fonts that cover CJK, tried in order; the first whose face maps
/// '中' wins. The file is mmapped — resident memory stays at the pages the
/// rasterizer actually touches, not the collection's tens of MB.
///
/// Discovery is per platform: the known macOS collections; the installed-
/// fonts registry on Windows; the standard font directories elsewhere.
/// Every candidate is coverage-verified before it wins, so a wrong
/// preference costs one mmap and nothing else.
#[cfg(target_os = "macos")]
const FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/System/Library/Fonts/Supplemental/Songti.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
];

struct GlyphSource {
    map: memmap2::Mmap,
    index: u32,
}

/// The coverage probe: a face that lacks this codepoint is not a CJK
/// fallback candidate.
const PROBE: char = '中';

/// Mmap `path` and return face `index` if the collection covers the probe.
fn open_covering(path: &Path, index: u32) -> Option<(GlyphSource, String)> {
    let file = std::fs::File::open(path).ok()?;
    let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else {
        return None;
    };
    let font = ab_glyph::FontRef::try_from_slice_and_index(&map, index).ok()?;
    if font.glyph_id(PROBE).0 == 0 {
        return None;
    }
    Some((GlyphSource { map, index }, format!("{path:?}#{index}")))
}

/// Every face of a font file (collections carry several; plain files carry
/// one — index 1 then fails to parse and stops the loop).
fn open_first_covering(path: &Path) -> Option<(GlyphSource, String)> {
    (0..8u32).find_map(|index| open_covering(path, index))
}

/// True when the extension can rasterize from this file kind (.fon/.fnt
/// bitmap fonts and Type1 pairs register alongside real files).
fn is_font_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".ttc") || lower.ends_with(".otf")
}

impl GlyphSource {
    fn find() -> Option<(GlyphSource, String)> {
        platform_find()
    }

    fn font(&self) -> Option<FontRef<'_>> {
        FontRef::try_from_slice_and_index(&self.map, self.index).ok()
    }
}

// macOS — the system collections, tried in coverage order.
#[cfg(target_os = "macos")]
fn platform_find() -> Option<(GlyphSource, String)> {
    FONT_CANDIDATES
        .iter()
        .filter(|p| Path::new(p).exists())
        .find_map(|p| open_first_covering(Path::new(p)))
}

// Windows — installed fonts come from the registry: value name is the face
// ("Microsoft YaHei & Microsoft YaHei UI (TrueType)"), data is the file
// ("msyh.ttc", relative to the fonts dir; per-user installs are absolute).
#[cfg(target_os = "windows")]
fn platform_find() -> Option<(GlyphSource, String)> {
    // Preferred CJK families, best first — an ordering over discovered
    // names, not a font-path list. Anything covering the probe still wins
    // if none of these is installed.
    const PREFER: &[&str] = &[
        "microsoft yahei ui",
        "microsoft yahei",
        "noto sans cjk",
        "noto serif cjk",
        "source han sans",
        "source han serif",
        "dengxian",
        "simsun",
        "simhei",
        "ms gothic",
        "malgun gothic",
    ];

    let key = windows_registry::LOCAL_MACHINE
        .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts")
        .ok()?;
    let mut names: Vec<(String, String)> = Vec::new();
    let values = key.values().ok()?;
    for (name, value) in values {
        let Ok(file) = String::try_from(value) else { continue };
        names.push((name.to_lowercase(), file));
    }

    for want in PREFER {
        for (name, file) in &names {
            if name.contains(want) {
                if let Some(hit) = windows_open(file) {
                    return Some(hit);
                }
            }
        }
    }
    // No preferred family installed (or none of them covered): fall back
    // to honest discovery — first registered file that covers the probe.
    names
        .iter()
        .filter_map(|(_, file)| windows_open(file))
        .next()
}

#[cfg(target_os = "windows")]
fn windows_open(spec: &str) -> Option<(GlyphSource, String)> {
    use std::path::PathBuf;
    // Type1 entries carry "name.bmp, name.pfm" — the first file only.
    let first = spec.split(',').next().unwrap_or("").trim();
    if !is_font_file(first) {
        return None;
    }
    let path = if Path::new(first).is_absolute() {
        PathBuf::from(first)
    } else {
        std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join("Fonts"))
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows\Fonts"))
            .join(first)
    };
    open_first_covering(&path)
}

// Linux/BSD — scan the standard font trees (fontconfig's default dirs;
// discovery stays a directory walk, not a daemon dependency).
#[cfg(all(unix, not(target_os = "macos")))]
fn platform_find() -> Option<(GlyphSource, String)> {
    use std::path::PathBuf;
    // Preferred CJK families by file-name stem, best first — an ordering
    // over discovered files, verified by coverage like everywhere else.
    const PREFER: &[&str] = &[
        "notosanscjk",
        "notoserifcjk",
        "sourcehansans",
        "sourcehanserif",
        "wenquanyi",
        "wqy",
        "droidsansfallback",
        "arphic",
        "uming",
        "ukai",
    ];

    let mut roots: Vec<PathBuf> = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share/fonts"));
        roots.push(home.join(".fonts"));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for root in &roots {
        collect_font_files(root, 0, &mut files);
    }
    // Stable preference ordering, then the first file that actually covers.
    files.sort_by_key(|path| {
        let stem = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let rank = PREFER
            .iter()
            .position(|p| stem.contains(p))
            .unwrap_or(PREFER.len());
        (rank, path.clone())
    });
    files.iter().find_map(|path| open_first_covering(path))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn collect_font_files(dir: &Path, depth: u8, out: &mut Vec<std::path::PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            collect_font_files(&path, depth + 1, out);
        } else {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if is_font_file(&name) {
                out.push(path);
            }
        }
    }
}

/// One slot's parsed FONT ATLAS blob, appendable.
struct SlotAtlas {
    slot: u8,
    cell_w: u8,
    cell_h: u8,
    baseline: u8,
    line_height: u8,
    flags: u8,
    density: u8,
    /// (codepoint, gid, advance, xoff) — serialized codepoint-sorted.
    cmap: Vec<(u32, u16, u8, u8)>,
    /// gid-linear coverage cells.
    coverage: Vec<u8>,
    known: HashSet<u32>,
    dirty: bool,
}

impl SlotAtlas {
    fn parse(blob: &[u8]) -> Option<SlotAtlas> {
        if blob.len() < HEADER {
            return None;
        }
        let u16at = |o: usize| u16::from_le_bytes([blob[o], blob[o + 1]]);
        if u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) != FONT_MAGIC {
            return None;
        }
        let version = u16at(4);
        if version != 2 && version != 3 {
            return None;
        }
        let glyph_count = u16at(6) as usize;
        let (cell_w, cell_h, baseline, line_height, slot, flags) =
            (blob[8], blob[9], blob[10], blob[11], blob[12], blob[13]);
        let density = if version == 3 { blob[14].max(1) } else { 1 };
        let cmap_end = HEADER + glyph_count * CMAP_ENTRY;
        let cell_bytes = cell_w as usize * cell_h as usize * (density as usize).pow(2);
        if blob.len() < cmap_end + glyph_count * cell_bytes {
            return None;
        }
        let mut cmap = Vec::with_capacity(glyph_count);
        let mut known = HashSet::with_capacity(glyph_count);
        for g in 0..glyph_count {
            let o = HEADER + g * CMAP_ENTRY;
            let cp = u32::from_le_bytes([blob[o], blob[o + 1], blob[o + 2], blob[o + 3]]);
            cmap.push((cp, u16at(o + 4), blob[o + 6], blob[o + 7]));
            known.insert(cp);
        }
        Some(SlotAtlas {
            slot,
            cell_w,
            cell_h,
            baseline,
            line_height,
            flags,
            density,
            cmap,
            coverage: blob[cmap_end..cmap_end + glyph_count * cell_bytes].to_vec(),
            known,
            dirty: false,
        })
    }

    fn glyph_count(&self) -> u16 {
        self.cmap.len() as u16
    }

    /// Rasterize `cp` from `font` into a new appended cell.
    fn append(&mut self, font: &FontRef<'_>, cp: char) {
        if self.glyph_count() >= MAX_GLYPHS {
            return;
        }
        let gid_font = font.glyph_id(cp);
        if gid_font.0 == 0 {
            return; // fallback font lacks it too — the core's tofu handles it
        }
        let px = slot_px(self.slot);
        let density = self.density as f32;
        let advance = font
            .as_scaled(PxScale::from(px))
            .h_advance(gid_font)
            .round()
            .clamp(0.0, 255.0) as u8;

        let cov_w = self.cell_w as usize * self.density as usize;
        let cov_h = self.cell_h as usize * self.density as usize;
        let mut cell = vec![0u8; cov_w * cov_h];
        let glyph = gid_font.with_scale_and_position(
            PxScale::from(px * density),
            point(0.0, self.baseline as f32 * density),
        );
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|x, y, c| {
                let cx = bounds.min.x as i32 + x as i32;
                let cy = bounds.min.y as i32 + y as i32;
                if cx >= 0 && (cx as usize) < cov_w && cy >= 0 && (cy as usize) < cov_h {
                    let dst = &mut cell[cy as usize * cov_w + cx as usize];
                    *dst = (*dst).max((c * 255.0) as u8);
                }
            });
        }

        let gid = self.glyph_count();
        self.coverage.extend_from_slice(&cell);
        self.cmap.push((cp as u32, gid, advance, 0));
        self.known.insert(cp as u32);
        self.dirty = true;
    }

    /// Serialize back to a v3 blob (cmap re-sorted by codepoint).
    fn blob(&self) -> Vec<u8> {
        let count = self.glyph_count();
        let mut cmap = self.cmap.clone();
        cmap.sort_by_key(|&(cp, ..)| cp);
        let mut out = Vec::with_capacity(HEADER + cmap.len() * CMAP_ENTRY + self.coverage.len());
        out.extend_from_slice(&FONT_MAGIC.to_le_bytes());
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&[
            self.cell_w,
            self.cell_h,
            self.baseline,
            self.line_height,
            self.slot,
            self.flags,
            self.density,
            0,
        ]);
        for (cp, gid, adv, xoff) in cmap {
            out.extend_from_slice(&cp.to_le_bytes());
            out.extend_from_slice(&gid.to_le_bytes());
            out.push(adv);
            out.push(xoff);
        }
        out.extend_from_slice(&self.coverage);
        out
    }
}

/// All of a pak's font slots + the system fallback face.
pub struct CjkAtlases {
    source: Option<GlyphSource>,
    slots: Vec<SlotAtlas>,
}

impl CjkAtlases {
    pub fn from_pak(pak: &[u8]) -> CjkAtlases {
        let slots: Vec<SlotAtlas> = pocket_ui_wgpu::walk_pak(pak)
            .into_iter()
            .filter(|e| e.key.starts_with("ui:font."))
            .filter_map(|e| SlotAtlas::parse(e.blob))
            .collect();
        let source = match GlyphSource::find() {
            Some((source, name)) => {
                log::info!("note-widget: CJK fallback font {name}");
                Some(source)
            }
            None => {
                log::warn!("note-widget: no CJK-capable system font found — non-Latin input will tofu");
                None
            }
        };
        CjkAtlases { source, slots }
    }

    /// Make sure every non-ASCII codepoint in `text` exists in every slot.
    /// Returns the rebuilt blobs of the slots that grew (feed them to
    /// `Ui::load_font_atlas`); empty when nothing was missing.
    pub fn ensure(&mut self, text: &str) -> Vec<Vec<u8>> {
        let missing: Vec<char> = {
            let mut seen = HashSet::new();
            text.chars()
                .filter(|c| (*c as u32) > 0x7f && !c.is_control())
                .filter(|c| self.slots.iter().any(|s| !s.known.contains(&(*c as u32))))
                .filter(|c| seen.insert(*c))
                .collect()
        };
        if missing.is_empty() {
            return Vec::new();
        }
        let Some(font) = self.source.as_ref().and_then(|s| s.font()) else {
            return Vec::new();
        };
        for cp in &missing {
            for slot in &mut self.slots {
                if !slot.known.contains(&(*cp as u32)) {
                    slot.append(&font, *cp);
                }
            }
        }
        let mut blobs = Vec::new();
        for slot in &mut self.slots {
            if std::mem::take(&mut slot.dirty) {
                blobs.push(slot.blob());
            }
        }
        if !blobs.is_empty() {
            log::info!(
                "note-widget: extended {} font slot(s) with {} new glyph(s)",
                blobs.len(),
                missing.len()
            );
        }
        blobs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal well-formed v3 blob: header + N cmap entries + N coverage
    /// cells (2x2 at density 1).
    fn synth_blob(glyphs: &[(u32, u16)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&FONT_MAGIC.to_le_bytes());
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&(glyphs.len() as u16).to_le_bytes());
        out.extend_from_slice(&[2, 2, 2, 2, /*slot*/ 3, /*flags*/ 0, /*density*/ 1, 0]);
        for &(cp, gid) in glyphs {
            out.extend_from_slice(&cp.to_le_bytes());
            out.extend_from_slice(&gid.to_le_bytes());
            out.push(4); // advance
            out.push(0); // xoff
        }
        out.extend(std::iter::repeat_n(0u8, glyphs.len() * 4));
        out
    }

    #[test]
    fn parse_rejects_non_atlas_blobs() {
        assert!(SlotAtlas::parse(b"").is_none());
        assert!(SlotAtlas::parse(b"not a font atlas blob!").is_none());
    }

    #[test]
    fn blob_round_trip_is_codepoint_sorted() {
        // Serialized out of codepoint order; blob() must re-sort.
        let unsorted = synth_blob(&[(0x4e2d, 2), (0x41, 0), (0xe9, 1)]);
        let atlas = SlotAtlas::parse(&unsorted).expect("synth blob parses");
        assert_eq!(atlas.slot, 3);
        assert_eq!(atlas.glyph_count(), 3);
        assert!(atlas.known.contains(&0x4e2d));
        let again = SlotAtlas::parse(&atlas.blob()).expect("re-serialized blob parses");
        let cps: Vec<u32> = again.cmap.iter().map(|&(cp, ..)| cp).collect();
        assert_eq!(cps, vec![0x41, 0xe9, 0x4e2d], "cmap serialized sorted");
        assert_eq!(again.coverage.len(), atlas.coverage.len());
        let third = SlotAtlas::parse(&again.blob()).expect("stable across re-serializations");
        assert_eq!(third.cmap, again.cmap);
        assert_eq!(third.coverage, again.coverage);
    }
}
