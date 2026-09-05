// SPDX-License-Identifier: AGPL-3.0-or-later
use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Context;
use mupdf::device::{Device, NativeDevice};
use mupdf::path::{Path as MuPath, PathWalker};
use mupdf::{
    ColorParams, Colorspace, DisplayList, Document, IRect, Matrix, Pixmap, Rect, StrokeState,
};

use super::Rgba;
use crate::geometry::{flatten_cubic, PagePt, PageRect, PageSize, Polyline};

/// Flattening tolerance for curves, in page points.
const CURVE_TOLERANCE: f64 = 0.3;

/// An open PDF document. Not `Send`: MuPDF handles are raw pointers, so the
/// owning thread must do all the work (see the shell's render thread).
pub struct MupdfBackend {
    doc: Document,
    /// The current page's display list, parsed once and replayed for every
    /// render (any zoom / sub-rect) so we never re-interpret the page per frame.
    list_cache: RefCell<Option<(usize, DisplayList)>>,
}

impl MupdfBackend {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let doc = Document::open(path).with_context(|| format!("open {path}"))?;
        Ok(MupdfBackend {
            doc,
            list_cache: RefCell::new(None),
        })
    }

    pub fn page_count(&self) -> anyhow::Result<usize> {
        Ok(self.doc.page_count()? as usize)
    }

    pub fn page_size(&self, page: usize) -> anyhow::Result<PageSize> {
        let p = self.doc.load_page(page as i32)?;
        let b = p.bounds()?;
        Ok(PageSize {
            w: (b.x1 - b.x0) as f64,
            h: (b.y1 - b.y0) as f64,
        })
    }

    /// Every stroked or filled path on `page`, flattened to polylines in page
    /// space (the same frame `render_region` draws in).
    pub fn extract_geometry(&self, page: usize) -> anyhow::Result<Vec<Polyline>> {
        let p = self.doc.load_page(page as i32)?;
        let collector = Rc::new(RefCell::new(Collector::default()));
        let device = Device::from_native(GeomDevice(Rc::clone(&collector)))?;
        p.run(&device, &Matrix::IDENTITY)?;
        collector.borrow_mut().flush();
        let polylines = collector.borrow().polylines.clone();
        Ok(polylines)
    }

    /// Render only the page-space rectangle `clip` at `scale` px per page-point.
    /// Rendering just the visible region keeps cost bounded regardless of zoom
    /// (a full-page render at high zoom is enormous).
    pub fn render_region(&self, page: usize, scale: f64, clip: PageRect) -> anyhow::Result<Rgba> {
        self.ensure_display_list(page)?;
        // Device-space pixel rect for the requested page-space clip.
        let irect = IRect::new(
            (clip.x0 * scale).floor() as i32,
            (clip.y0 * scale).floor() as i32,
            (clip.x1 * scale).ceil() as i32,
            (clip.y1 * scale).ceil() as i32,
        );
        // Render only this region: a pixmap covering `irect`, cleared to white
        // paper, with a draw device clipped to it. RGB (n=3) -> expand to RGBA.
        let mut pm = Pixmap::new_with_rect(&Colorspace::device_rgb(), irect, false)?;
        pm.clear_with(255)?;
        let device = Device::from_pixmap_with_clip(&pm, irect)?;
        {
            // Replay the cached display list (no page re-parse), culling commands
            // outside the rendered region via the (device-space) scissor — the
            // real speed win at high zoom, where most paths fall outside `irect`.
            let scissor = Rect::new(
                irect.x0 as f32,
                irect.y0 as f32,
                irect.x1 as f32,
                irect.y1 as f32,
            );
            let cache = self.list_cache.borrow();
            let (_, list) = cache.as_ref().expect("display list cached above");
            list.run(
                &device,
                &Matrix::new_scale(scale as f32, scale as f32),
                scissor,
            )?;
        }
        drop(device);

        let w = pm.width() as usize;
        let h = pm.height() as usize;
        let n = pm.n() as usize;
        let stride = pm.stride() as usize;
        let src = pm.samples();
        let mut bytes = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let row = &src[y * stride..y * stride + w * n];
            for px in row.chunks_exact(n) {
                bytes.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        Ok(Rgba {
            width: w as u32,
            height: h as u32,
            bytes,
            origin: PagePt {
                x: pm.x() as f64 / scale,
                y: pm.y() as f64 / scale,
            },
            scale,
        })
    }

    /// Ensure `list_cache` holds the display list for `page` (building it once).
    fn ensure_display_list(&self, page: usize) -> anyhow::Result<()> {
        let mut cache = self.list_cache.borrow_mut();
        if cache.as_ref().is_none_or(|(pg, _)| *pg != page) {
            let p = self.doc.load_page(page as i32)?;
            *cache = Some((page, p.to_display_list(true)?));
        }
        Ok(())
    }
}

/// Accumulates flattened polylines for one page in page (device-identity) space.
#[derive(Default)]
struct Collector {
    polylines: Vec<Polyline>,
    current: Vec<PagePt>,
}

impl Collector {
    fn flush(&mut self) {
        if self.current.len() > 1 {
            self.polylines
                .push(Polyline(std::mem::take(&mut self.current)));
        } else {
            self.current.clear();
        }
    }
}

/// `Path::walk` consumes the walker by value, so it carries a cheap Rc handle
/// plus the path's CTM. Walked points are path-local; multiplying by the CTM
/// puts them in page/device space (matching what gets rendered) — without this
/// the snap geometry is offset from the drawing.
struct Walker {
    collector: Rc<RefCell<Collector>>,
    ctm: Matrix,
}

impl Walker {
    fn pt(&self, x: f32, y: f32) -> PagePt {
        let (x, y) = self.ctm.transform_xy(x, y);
        PagePt {
            x: x as f64,
            y: y as f64,
        }
    }
}

impl PathWalker for Walker {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        let mut c = self.collector.borrow_mut();
        c.flush();
        c.current.push(p);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.collector.borrow_mut().current.push(p);
    }
    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, ex: f32, ey: f32) {
        let (p1, p2, p3) = (self.pt(c1x, c1y), self.pt(c2x, c2y), self.pt(ex, ey));
        let mut c = self.collector.borrow_mut();
        let p0 = c.current.last().copied().unwrap_or(p1);
        let pts = flatten_cubic(p0, p1, p2, p3, CURVE_TOLERANCE);
        c.current.extend(pts);
    }
    fn close(&mut self) {}
}

struct GeomDevice(Rc<RefCell<Collector>>);

impl GeomDevice {
    fn walk(&self, path: &MuPath, ctm: Matrix) {
        let _ = path.walk(Walker {
            collector: Rc::clone(&self.0),
            ctm,
        });
        self.0.borrow_mut().flush();
    }
}

impl NativeDevice for GeomDevice {
    fn fill_path(
        &mut self,
        path: &MuPath,
        _even_odd: bool,
        ctm: Matrix,
        _cs: &Colorspace,
        _color: &[f32],
        _alpha: f32,
        _cp: ColorParams,
    ) {
        self.walk(path, ctm);
    }
    fn stroke_path(
        &mut self,
        path: &MuPath,
        _stroke: &StrokeState,
        ctm: Matrix,
        _cs: &Colorspace,
        _color: &[f32],
        _alpha: f32,
        _cp: ColorParams,
    ) {
        self.walk(path, ctm);
    }
}
