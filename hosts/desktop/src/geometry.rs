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
}
