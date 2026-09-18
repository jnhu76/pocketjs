//! Library surface for shared Desktop presentation mechanisms (R1).
//!
//! Stock `hosts/desktop` binaries and product hosts (PicoView) consume the
//! same geometry/signature types from this crate. Do not copy these rules
//! into product trees.
//!
//! This export intentionally covers only generic presentation identity:
//! measured physical client size, live OS scale, viewport policy, and
//! render-signature invalidation. Host/ApplicationHandler lifecycle stays
//! binary-private.

pub mod geometry;

pub use geometry::{
    DESKTOP_DYNAMIC_MAX, DESKTOP_DYNAMIC_MIN, PresentationGeometry, RenderSignature, ViewportPolicy,
    child_surface_size, resolve_geometry,
};
