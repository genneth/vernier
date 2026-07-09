// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::Polyline;

pub mod mupdf_backend;

/// An RGBA8 image of a rendered page region, row-major, no padding (stride == width*4).
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    /// Page-space point corresponding to the top-left pixel of this region.
    pub origin: (f64, f64),
    /// Pixels per page-point this region was rendered at.
    pub scale: f64,
}

pub trait PdfBackend {
    fn page_count(&self) -> usize;
    fn page_size_pts(&self, page: usize) -> (f64, f64);
    fn extract_geometry(&self, page: usize) -> anyhow::Result<Vec<Polyline>>;
    /// Render only the page-space rectangle `clip` = (x0, y0, x1, y1) at `scale`
    /// px per page-point. Rendering just the visible region keeps cost bounded
    /// regardless of zoom (a full-page render at high zoom is enormous).
    fn render_region(
        &self,
        page: usize,
        scale: f64,
        clip: (f64, f64, f64, f64),
    ) -> anyhow::Result<Rgba>;
}
