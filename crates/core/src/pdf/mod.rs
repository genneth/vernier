// SPDX-License-Identifier: AGPL-3.0-or-later
//! PDF rendering and vector-geometry extraction.
//!
//! This is the only module that touches MuPDF. Everything it hands out is
//! plain data in page space (`Rgba`, `Polyline`), so the rest of the core and
//! the shell never see a MuPDF type.
use crate::geometry::PagePt;

mod mupdf_backend;
pub use mupdf_backend::MupdfBackend;

/// An RGBA8 image of a rendered page region, row-major, no padding (stride == width*4).
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    /// Page-space point corresponding to the top-left pixel of this region.
    pub origin: PagePt,
    /// Pixels per page-point this region was rendered at.
    pub scale: f64,
}
