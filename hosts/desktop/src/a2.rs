// ---------------------------------------------------------------------------
// A2 native image-resource proof harness (desktop-host test material).
//
// Generates a deterministic ≥4K pattern in native code, registers it through
// `Ui::register_native_texture` (the host-side image-resource seam), and hands
// the guest ONLY bounded semantic state — the generation-tagged handle and
// dimensions — over the existing svc channel (spec ops 30..32). The guest
// composes it with ordinary image nodes; retirement is native `free_texture`.
//
// Every A2 svc line and lifecycle event is logged with byte counts so the
// O(1) guest-boundary traffic can be contrasted with the 31.6 MiB native
// pixel plane. Enabled by `--a2-harness`; off by default and absent from
// stock app paths.
// ---------------------------------------------------------------------------

const A2_SERVICE: &str = "picoview-a2";
const A2_W: u32 = 3840;
const A2_H: u32 = 2160;

/// One lifecycle schedule entry. Kept as a flat table so the run is
/// reproducible from the command line alone.
enum A2Action {
    /// Register `id` with pattern `variant` and announce it to the guest.
    Announce(&'static str, u8),
    /// Natively retire a live resource (id).
    Retire(&'static str),
    /// Register a stress replacement, then retire its predecessor.
    RetirePrevious(&'static str, &'static str, u8),
    /// Log the boundary totals once.
    Summary,
}

const A2_SCHEDULE: [(u64, A2Action); 13] = [
    (2, A2Action::Announce("A", 0)),
    (150, A2Action::Announce("B", 1)),
    (300, A2Action::Retire("A")),
    (360, A2Action::RetirePrevious("R0", "B", 0)),
    (400, A2Action::RetirePrevious("R1", "R0", 1)),
    (440, A2Action::RetirePrevious("R2", "R1", 0)),
    (480, A2Action::RetirePrevious("R3", "R2", 1)),
    (520, A2Action::RetirePrevious("R4", "R3", 0)),
    (560, A2Action::RetirePrevious("R5", "R4", 1)),
    (600, A2Action::RetirePrevious("R6", "R5", 0)),
    (640, A2Action::RetirePrevious("R7", "R6", 1)),
    (680, A2Action::Retire("R7")),
    (720, A2Action::Summary),
];

struct A2Harness {
    active: bool,
    done: usize,
    /// (id, handle, pixel bytes) per live A2 resource.
    live: Vec<(&'static str, i32, usize)>,
    tx_lines: u64,
    tx_bytes: u64,
    rx_lines: u64,
    rx_bytes: u64,
}

impl A2Harness {
    fn new(active: bool) -> Self {
        Self {
            active,
            done: 0,
            live: Vec::new(),
            tx_lines: 0,
            tx_bytes: 0,
            rx_lines: 0,
            rx_bytes: 0,
        }
    }

    fn tick(&mut self, tick: u64, surface: &UiSurface) {
        if !self.active || self.done >= A2_SCHEDULE.len() || A2_SCHEDULE[self.done].0 != tick {
            return;
        }
        while self.done < A2_SCHEDULE.len() && A2_SCHEDULE[self.done].0 == tick {
            match &A2_SCHEDULE[self.done].1 {
                A2Action::Announce(id, variant) => self.announce(surface, id, *variant),
                A2Action::Retire(id) => {
                    if self.live.iter().any(|(known, _, _)| *known == *id) {
                        self.retire(surface, id);
                    }
                }
                A2Action::RetirePrevious(id, previous, variant) => {
                    self.announce(surface, id, *variant);
                    if self.live.iter().any(|(known, _, _)| known == previous) {
                        self.retire(surface, previous);
                    }
                }
                A2Action::Summary => self.summary(),
            }
            self.done += 1;
        }
    }

    /// Native producer → registration → bounded svc announcement.
    fn announce(&mut self, surface: &UiSurface, id: &'static str, variant: u8) {
        let start = Instant::now();
        let pixels = a2_pattern(A2_W, A2_H, variant);
        let generate_us = start.elapsed().as_micros();
        let bytes = pixels.len();
        let start = Instant::now();
        let (handle, live_bytes) = surface.with_ui(|ui| {
            let handle = ui.register_native_texture(&pixels, A2_W, A2_H, pocketjs_core::spec::psm::PSM_8888, true);
            (handle, ui.texture_live_bytes())
        });
        let register_us = start.elapsed().as_micros();
        // `pixels` (≈31.6 MiB) die here, on the native side. Nothing pixel-
        // sized ever leaves this function.
        drop(pixels);
        if handle < 0 {
            eprintln!("A2EVENT,register-failed,id={id}");
            return;
        }
        self.live.push((id, handle, bytes));
        eprintln!(
            "A2EVENT,register,id={id},handle={handle},w={A2_W},h={A2_H},bytes={bytes},liveBytes={live_bytes},generateUs={generate_us},registerUs={register_us}"
        );
        self.push(
            surface,
            &json!({"t": "a2img", "id": id, "handle": handle, "w": A2_W, "h": A2_H}).to_string(),
        );
    }

    /// Explicit native retirement: pixel bytes drop synchronously, the slot
    /// generation bumps, and outstanding (guest-retained) handles go stale.
    fn retire(&mut self, surface: &UiSurface, id: &str) {
        let Some(index) = self.live.iter().position(|(known, _, _)| *known == id) else {
            return;
        };
        let (_, handle, bytes) = self.live.remove(index);
        let (live_before, live_after) = surface.with_ui(|ui| {
            let before = ui.texture_live_bytes();
            ui.free_texture(handle);
            (before, ui.texture_live_bytes())
        });
        eprintln!(
            "A2EVENT,retire,id={id},handle={handle},bytes={bytes},liveBefore={live_before},liveAfter={live_after}"
        );
        self.push(
            surface,
            &json!({"t": "a2retired", "id": id, "handle": handle}).to_string(),
        );
    }

    fn push(&mut self, surface: &UiSurface, line: &str) {
        surface.svc_push(line.to_string());
        self.tx_lines += 1;
        self.tx_bytes += line.len() as u64 + 1; // + newline, per the batch contract
        eprintln!("A2SVC,tx,{},{line}", line.len() + 1);
    }

    /// Guest → host A2 traffic (acks/probes): counted, logged, never forwarded
    /// as an intent. This is one side of the boundary audit.
    fn observe_rx(&mut self, line: &str) {
        self.rx_lines += 1;
        self.rx_bytes += line.len() as u64 + 1;
        eprintln!("A2SVC,rx,{},{line}", line.len() + 1);
    }

    fn summary(&self) {
        eprintln!(
            "A2BOUNDARY,txLines={},txBytes={},rxLines={},rxBytes={},totalBoundaryBytes={},nativePixelBytes={} — guest-boundary traffic is O(1) semantic fields",
            self.tx_lines,
            self.tx_bytes,
            self.rx_lines,
            self.rx_bytes,
            self.tx_bytes + self.rx_bytes,
            A2_W as u64 * A2_H as u64 * 4
        );
    }
}

/// Deterministic PSM_8888 (little-endian R,G,B,A per pixel) diagnostic
/// pattern: four quadrant fields, 240 px grid, both diagonals, a center
/// crosshair and an 8 px border. Variant shifts the palette so A/B frames
/// are visually distinguishable. Pure integer math — no decoder, no RNG.
fn a2_pattern(w: u32, h: u32, variant: u8) -> Vec<u8> {
    let mut pixels = vec![0u8; w as usize * h as usize * 4];
    let quadrant = |x: u32, y: u32| ((x >= w / 2) as u8) | ((y >= h / 2) as u8) << 1;
    // Per-quadrant base RGB (variant 0: red/green/blue/amber; variant 1:
    // cyan/magenta/lime/violet; further variants rotate the channels).
    const PALETTES: [[[u8; 3]; 4]; 2] = [
        [[168, 32, 26], [36, 120, 44], [30, 52, 158], [190, 140, 24]],
        [[22, 118, 128], [142, 34, 116], [88, 150, 30], [96, 52, 150]],
    ];
    let palette = PALETTES[(variant % 2) as usize];
    let accent = if variant % 2 == 0 { [250u8, 250, 250] } else { [16u8, 16, 20] };
    let border = if variant % 2 == 0 { [255u8, 214, 0] } else { [0u8, 229, 255] };
    let grid = (w / 16).max(16);
    for y in 0..h {
        for x in 0..w {
            let (mut r, mut g, mut b) = {
                let base = palette[quadrant(x, y) as usize];
                (base[0], base[1], base[2])
            };
            let in_border = x < 8 || y < 8 || x >= w - 8 || y >= h - 8;
            let on_grid = x % grid == 0 || y % grid == 0;
            let on_diagonal = (x as u64 * h as u64) / w as u64 == y as u64
                || (w - 1 - x) as u64 * h as u64 / w as u64 == y as u64;
            let on_cross = x == w / 2 || y == h / 2;
            let dx = x.abs_diff(w / 2) as u32;
            let dy = y.abs_diff(h / 2) as u32;
            let on_center = dx * dx + dy * dy < 24_u32 * 24;
            if in_border {
                (r, g, b) = (border[0], border[1], border[2]);
            } else if on_center {
                (r, g, b) = (accent[0], accent[1], accent[2]);
            } else if on_cross || on_diagonal {
                (r, g, b) = (accent[0], accent[1], accent[2]);
            } else if on_grid {
                r = r.saturating_add(48);
                g = g.saturating_add(48);
                b = b.saturating_add(48);
            }
            let i = (y as usize * w as usize + x as usize) * 4;
            pixels[i] = r;
            pixels[i + 1] = g;
            pixels[i + 2] = b;
            pixels[i + 3] = 255;
        }
    }
    pixels
}
