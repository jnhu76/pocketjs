//! Live desktop presentation geometry (R1, PicoView #63 Corrective A).
//!
//! Physical client size is first-class authority for the retained GPU target
//! and settled swapchain equality. It is measured on the window/UI thread
//! (`Window::inner_size()`, `WindowEvent::Resized`) and preserved across the
//! worker boundary — never reconstructed as `logical × scale`.
//!
//! Package raster density remains resource/cook authority (fonts, baked
//! assets, `Ui::new_with_raster_density`). It is NOT the live presentation
//! scale.

/// Immutable live presentation snapshot shared by the window thread, runtime
/// worker, renderer, and present path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationGeometry {
    pub logical_w: u32,
    pub logical_h: u32,
    pub physical_w: u32,
    pub physical_h: u32,
    /// `f32::to_bits` of the value passed to `UiRenderer::render_words_scaled`.
    pub render_scale_bits: u32,
}

impl PresentationGeometry {
    /// Build from **live measured** physical client size + OS scale.
    ///
    /// `physical` must come from `Window::inner_size()` / `Resized(size)`,
    /// not from `logical × scale`.
    pub fn from_live(logical: (u32, u32), physical: (u32, u32), os_scale: f64) -> Self {
        let render_scale = if os_scale > 0.0 && os_scale.is_finite() {
            os_scale as f32
        } else {
            1.0f32
        };
        Self {
            logical_w: logical.0,
            logical_h: logical.1,
            physical_w: physical.0.max(1),
            physical_h: physical.1.max(1),
            render_scale_bits: render_scale.to_bits(),
        }
    }

    pub fn logical(&self) -> (u32, u32) {
        (self.logical_w, self.logical_h)
    }

    pub fn physical(&self) -> (u32, u32) {
        (self.physical_w, self.physical_h)
    }

    /// Exact f32 raster scale identity — bits match `render_words_scaled`.
    pub fn effective_render_scale(&self) -> f32 {
        f32::from_bits(self.render_scale_bits)
    }

    /// Settled exact present: retained target equals live swapchain.
    pub fn is_exact_present(&self, swapchain: (u32, u32)) -> bool {
        self.physical() == swapchain && self.physical_w > 0 && self.physical_h > 0
    }
}

/// Product logical viewport authority vs live presentation facts.
///
/// `fixed` freezes the **layout/logical** viewport only. Physical client size
/// and live OS scale remain presentation authority and must keep updating
/// (monitor DPI change on a size-locked window still resizes the client).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportPolicy {
    /// Logical layout follows live client (`physical / scale`).
    Dynamic,
    /// Logical layout is the product/package fixed viewport.
    Fixed,
}

/// Single geometry resolver for boot, `Resized`, and `ScaleFactorChanged`.
///
/// - **Fixed:** logical = `product_logical`; physical/scale still measured.
/// - **Dynamic:** logical derived from measured physical ÷ live scale.
/// - **Both:** physical is measured (never `logical × scale` / density).
pub fn resolve_geometry(
    policy: ViewportPolicy,
    product_logical: (u32, u32),
    measured_physical: (u32, u32),
    os_scale: f64,
) -> PresentationGeometry {
    let scale = if os_scale > 0.0 && os_scale.is_finite() {
        os_scale
    } else {
        1.0
    };
    let logical = match policy {
        ViewportPolicy::Fixed => product_logical,
        ViewportPolicy::Dynamic => (
            (measured_physical.0 as f64 / scale)
                .round()
                .clamp(1.0, 8192.0) as u32,
            (measured_physical.1 as f64 / scale)
                .round()
                .clamp(1.0, 8192.0) as u32,
        ),
    };
    PresentationGeometry::from_live(logical, measured_physical, os_scale)
}

/// Child compositor target size = package child logical × **live** scale.
///
/// Never `logical × package_raster_density`. Children have no measured OS
/// window; their physical surface is the parent presentation scale applied
/// to the package logical extent.
pub fn child_surface_size(logical: (u32, u32), live_scale: f32) -> (u32, u32) {
    let scale = if live_scale.is_finite() && live_scale > 0.0 {
        live_scale
    } else {
        1.0
    };
    (
        ((logical.0 as f32 * scale).round() as u32).max(1),
        ((logical.1 as f32 * scale).round() as u32).max(1),
    )
}

/// Explicit demand-render identity for presentation (R1).
///
/// Includes physical target size and the **effective** render scale bits
/// actually passed to the GPU renderer — not raw f64 OS-scale bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSignature {
    pub draw_hash: u64,
    pub raster_revision: u64,
    pub physical_w: u32,
    pub physical_h: u32,
    pub render_scale_bits: u32,
}

impl RenderSignature {
    pub fn new(
        draw_hash: u64,
        raster_revision: u64,
        physical: (u32, u32),
        effective_render_scale: f32,
    ) -> Self {
        Self {
            draw_hash,
            raster_revision,
            physical_w: physical.0,
            physical_h: physical.1,
            render_scale_bits: effective_render_scale.to_bits(),
        }
    }

    pub fn from_geometry(
        draw_hash: u64,
        raster_revision: u64,
        geometry: PresentationGeometry,
    ) -> Self {
        Self {
            draw_hash,
            raster_revision,
            physical_w: geometry.physical_w,
            physical_h: geometry.physical_h,
            render_scale_bits: geometry.render_scale_bits,
        }
    }

    pub fn needs_rerender(&self, previous: Option<Self>) -> bool {
        previous != Some(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_size_is_measured_not_round_tripped() {
        // 125% session: logical 960×640, measured physical 1200×800.
        // A buggy rebuild logical×scale would also be 1200×800 here — use a
        // case where rounding or OS chrome would diverge if someone cheated.
        let geo = PresentationGeometry::from_live((960, 640), (1201, 801), 1.25);
        assert_eq!(geo.physical(), (1201, 801));
        assert_eq!(geo.logical(), (960, 640));
        assert_eq!(geo.effective_render_scale(), 1.25f32);
        // Signature scale bits are the effective f32, not f64 OS bits.
        assert_eq!(geo.render_scale_bits, 1.25f32.to_bits());
        assert_ne!(geo.render_scale_bits, 1.25f64.to_bits() as u32);
    }

    #[test]
    fn settled_settled_equality_uses_physical_and_swapchain() {
        let geo = PresentationGeometry::from_live((960, 640), (1200, 800), 1.25);
        assert!(geo.is_exact_present((1200, 800)));
        assert!(!geo.is_exact_present((1201, 801)));
        assert!(!geo.is_exact_present((1200, 0)));
    }

    #[test]
    fn boot_does_not_reconstruct_physical_from_logical_times_scale() {
        // If physical were rebuilt as logical×scale, 960*1.25=1200 would hide
        // the measured 1199. Authority keeps 1199.
        let geo = PresentationGeometry::from_live((960, 640), (1199, 799), 1.25);
        assert_eq!(geo.physical_w, 1199);
        assert_eq!(geo.physical_h, 799);
        let rebuilt = (
            (960f64 * 1.25).round() as u32,
            (640f64 * 1.25).round() as u32,
        );
        assert_eq!(rebuilt, (1200, 800));
        assert_ne!(geo.physical(), rebuilt);
    }

    #[test]
    fn render_signature_different_physical_size_unequal() {
        let a = RenderSignature::new(0xabc, 9, (1920, 1280), 2.0);
        let b = RenderSignature::new(0xabc, 9, (1200, 800), 2.0);
        assert_ne!(a, b);
        assert!(a.needs_rerender(Some(b)));
        assert!(b.needs_rerender(Some(a)));
    }

    #[test]
    fn render_signature_different_effective_scale_unequal() {
        let a = RenderSignature::new(0xabc, 9, (1920, 1280), 2.0);
        let c = RenderSignature::new(0xabc, 9, (1920, 1280), 1.25);
        assert_ne!(a, c);
        assert!(a.needs_rerender(Some(c)));
    }

    #[test]
    fn render_signature_identical_raster_inputs_equal() {
        let a = RenderSignature::new(0xabc, 9, (1920, 1280), 2.0);
        let d = RenderSignature::new(0xabc, 9, (1920, 1280), 2.0);
        assert_eq!(a, d);
        assert!(!a.needs_rerender(Some(d)));
        // First frame always renders.
        assert!(a.needs_rerender(None));
        // Draw or revision change also rerenders.
        assert_ne!(
            a,
            RenderSignature::new(0xabd, 9, (1920, 1280), 2.0)
        );
        assert_ne!(
            a,
            RenderSignature::new(0xabc, 10, (1920, 1280), 2.0)
        );
    }

    #[test]
    fn signature_from_geometry_uses_effective_scale_bits() {
        let geo = PresentationGeometry::from_live((720, 480), (1440, 960), 2.0);
        let sig = RenderSignature::from_geometry(1, 2, geo);
        assert_eq!(sig.physical_w, 1440);
        assert_eq!(sig.physical_h, 960);
        assert_eq!(sig.render_scale_bits, geo.effective_render_scale().to_bits());
        assert_eq!(sig, RenderSignature::new(1, 2, (1440, 960), 2.0));
    }

    #[test]
    fn fixed_boot_keeps_product_logical_with_measured_physical() {
        // Product plan 720×480; measured client at 125% is 899×599 (not
        // reconstructed as 720×1.25). Logical must not be re-derived.
        let geo = resolve_geometry(ViewportPolicy::Fixed, (720, 480), (899, 599), 1.25);
        assert_eq!(geo.logical(), (720, 480));
        assert_eq!(geo.physical(), (899, 599));
        assert_eq!(geo.effective_render_scale(), 1.25f32);
        assert_eq!(geo.render_scale_bits, 1.25f32.to_bits());
    }

    #[test]
    fn fixed_dpi_change_updates_presentation_not_logical() {
        let before = resolve_geometry(ViewportPolicy::Fixed, (720, 480), (720, 480), 1.0);
        let after = resolve_geometry(ViewportPolicy::Fixed, (720, 480), (1080, 720), 1.5);
        // Layout stays frozen.
        assert_eq!(after.logical(), (720, 480));
        // Presentation facts move.
        assert_eq!(after.physical(), (1080, 720));
        assert_eq!(after.effective_render_scale(), 1.5f32);
        assert_ne!(before.physical(), after.physical());
        assert_ne!(
            before.render_scale_bits,
            after.render_scale_bits
        );
        // RenderSignature must invalidate on live presentation change.
        let sig_before = RenderSignature::from_geometry(1, 2, before);
        let sig_after = RenderSignature::from_geometry(1, 2, after);
        assert_ne!(sig_before, sig_after);
        assert!(sig_after.needs_rerender(Some(sig_before)));
        // Settled exact uses live physical, not frozen 720×480.
        assert!(after.is_exact_present((1080, 720)));
        assert!(!after.is_exact_present((720, 480)));
    }

    #[test]
    fn dynamic_boot_derives_logical_from_measured_physical() {
        let geo = resolve_geometry(ViewportPolicy::Dynamic, (720, 480), (1200, 800), 1.25);
        assert_eq!(geo.logical(), (960, 640));
        assert_eq!(geo.physical(), (1200, 800));
        assert_eq!(geo.effective_render_scale(), 1.25f32);
    }

    #[test]
    fn child_target_uses_live_scale_not_package_density() {
        // child logical 400×300, live_scale 1.25 → 500×375.
        // package_density=2 would wrongly yield 800×600 — must not happen.
        let child = child_surface_size((400, 300), 1.25);
        assert_eq!(child, (500, 375));
        assert_ne!(child, (800, 600));
        assert_ne!(child, child_surface_size((400, 300), 2.0));
        // Zero/invalid scale falls back to identity, not density.
        assert_eq!(child_surface_size((400, 300), 0.0), (400, 300));
    }
}
