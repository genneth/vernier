// SPDX-License-Identifier: AGPL-3.0-or-later
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk4::glib;
use gtk4::graphene;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use vernier_core::app::{AppState, Tool};
use vernier_core::geometry::{PagePt, Polyline, ScreenPt};
use vernier_core::pdf::{mupdf_backend::MupdfBackend, PdfBackend};
use vernier_core::view::View;

// Colourblind-safe overlay palette (IBM), distinguished also by shape/style.
const COL_DIM: (f64, f64, f64) = (0.392, 0.561, 1.0); // #648FFF — measurements
const COL_DIM_HOT: (f64, f64, f64) = (0.55, 0.69, 1.0); // hovered measurement
const COL_SCALE: (f64, f64, f64) = (0.863, 0.149, 0.498); // #DC267F — scale calibration
const COL_SNAP: (f64, f64, f64) = (0.996, 0.380, 0.0); // #FE6100 — snap marker

// ===========================================================================
// PageView: a custom widget whose snapshot() draws the page as a GPU texture
// (resampled to the current View transform every frame — so zoom/pan are GPU
// work, not per-frame CPU rasterisation) plus a Cairo overlay sharing the same
// transform (so snap markers stay aligned to the page).
// ===========================================================================
mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct PageView {
        pub state: RefCell<Option<Rc<RefCell<AppState>>>>,
        /// The cached GPU texture of the rendered page region. Kept as one
        /// instance so GSK uploads it once and only resamples thereafter.
        pub texture: RefCell<Option<gtk4::gdk::Texture>>,
        /// Page-space top-left the texture represents, and its px-per-point.
        pub tex_origin: Cell<(f64, f64)>,
        pub tex_scale: Cell<f64>,
        /// Coarse full-page preview, drawn (soft) under the crisp texture so pan/
        /// zoom never reveals blank page — it sharpens as crisp rasters land.
        pub preview: RefCell<Option<gtk4::gdk::Texture>>,
        /// Full page size (pts), so the page extent and preview can be placed.
        pub page_size: Cell<(f64, f64)>,
        /// Screen rect (x, y, w, h) of the × delete badge beside the hovered
        /// dimension's label, refreshed each overlay draw. None when nothing
        /// is hovered.
        pub close_rect: Cell<Option<(f64, f64, f64, f64)>>,
        /// Screen rects of every dimension's label chip, index-aligned with
        /// committed dimensions, refreshed each overlay draw. Fed back into the
        /// core hover hit-test (hovering the chip = hovering the dimension).
        pub label_rects: RefCell<Vec<(f64, f64, f64, f64)>>,
        /// Last allocated size + a callback fired (with the previous size) when
        /// it changes — drives the re-fit-on-resize of the fit zoom modes.
        pub last_alloc: Cell<(i32, i32)>,
        #[allow(clippy::type_complexity)]
        pub resize_cb: RefCell<Option<Box<dyn Fn(i32, i32)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PageView {
        const NAME: &'static str = "VernierPageView";
        type Type = super::PageView;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for PageView {}

    impl WidgetImpl for PageView {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let w = widget.width() as f32;
            let h = widget.height() as f32;

            // Backdrop (dark mat so the white page stands out).
            snapshot.append_color(
                &gtk4::gdk::RGBA::new(0.12, 0.12, 0.13, 1.0),
                &graphene::Rect::new(0.0, 0.0, w, h),
            );

            let Some(state) = self.state.borrow().clone() else {
                return;
            };
            let app = state.borrow();
            let v = app.view();

            // Layered fallbacks over the page extent so a pan/zoom never reveals
            // a blank/dark hole: white paper (instant) < coarse preview (soft) <
            // the crisp viewport texture. Each higher layer covers the last.
            let (pw, ph) = self.page_size.get();
            if pw > 0.0 && ph > 0.0 {
                let tl = v.page_to_screen(PagePt { x: 0.0, y: 0.0 });
                let page_rect = graphene::Rect::new(
                    tl.x as f32,
                    tl.y as f32,
                    (pw * v.zoom) as f32,
                    (ph * v.zoom) as f32,
                );
                snapshot.append_color(&gtk4::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0), &page_rect);
                if let Some(prev) = self.preview.borrow().as_ref() {
                    snapshot.append_scaled_texture(
                        prev,
                        gtk4::gsk::ScalingFilter::Trilinear,
                        &page_rect,
                    );
                }
            }

            // Crisp page texture, GPU-scaled to the current view transform.
            if let Some(tex) = self.texture.borrow().as_ref() {
                let (ox, oy) = self.tex_origin.get();
                let rs = self.tex_scale.get();
                let tl = v.page_to_screen(PagePt { x: ox, y: oy });
                let s = v.zoom / rs;
                let dw = tex.width() as f64 * s;
                let dh = tex.height() as f64 * s;
                snapshot.append_texture(
                    tex,
                    &graphene::Rect::new(tl.x as f32, tl.y as f32, dw as f32, dh as f32),
                );
            }

            // Overlay (dimensions, calibration, snap marker) via Cairo, same transform.
            let cr = snapshot.append_cairo(&graphene::Rect::new(0.0, 0.0, w, h));
            draw_overlay(
                &cr,
                widget.upcast_ref::<gtk4::Widget>(),
                &app,
                &v,
                &self.close_rect,
                &self.label_rects,
            );
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            let old = self.last_alloc.get();
            if old != (width, height) {
                self.last_alloc.set((width, height));
                if let Some(cb) = self.resize_cb.borrow().as_ref() {
                    cb(old.0, old.1);
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct PageView(ObjectSubclass<imp::PageView>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl PageView {
    fn new(state: Rc<RefCell<AppState>>) -> Self {
        let obj: Self = glib::Object::new();
        obj.set_hexpand(true);
        obj.set_vexpand(true);
        // Clip drawing to our own allocation: snapshot() draws the texture at a
        // transform that grows past the widget when zoomed in, and the default
        // Overflow::Visible would paint it over sibling widgets (the header).
        obj.set_overflow(gtk4::Overflow::Hidden);
        obj.set_accessible_role(gtk4::AccessibleRole::Img);
        obj.update_property(&[gtk4::accessible::Property::Label("Plan canvas")]);
        *obj.imp().state.borrow_mut() = Some(state);
        obj
    }

    /// Screen rect of the hovered dimension's × delete badge, if showing.
    pub fn close_rect(&self) -> Option<(f64, f64, f64, f64)> {
        self.imp().close_rect.get()
    }

    /// Screen rects of all dimension label chips, from the last overlay draw.
    pub fn label_rects(&self) -> Vec<(f64, f64, f64, f64)> {
        self.imp().label_rects.borrow().clone()
    }

    fn set_page_texture(&self, tex: gtk4::gdk::Texture, origin: (f64, f64), scale: f64) {
        let imp = self.imp();
        imp.tex_origin.set(origin);
        imp.tex_scale.set(scale);
        *imp.texture.borrow_mut() = Some(tex);
        self.queue_draw();
    }

    /// Register a callback fired (with the previous allocated size) whenever the
    /// widget's allocated size changes.
    fn set_resize_cb(&self, f: impl Fn(i32, i32) + 'static) {
        *self.imp().resize_cb.borrow_mut() = Some(Box::new(f));
    }

    fn set_page_size(&self, w: f64, h: f64) {
        self.imp().page_size.set((w, h));
    }

    fn set_preview(&self, tex: gtk4::gdk::Texture) {
        *self.imp().preview.borrow_mut() = Some(tex);
        self.queue_draw();
    }

    fn clear_preview(&self) {
        *self.imp().preview.borrow_mut() = None;
    }
}

// ===========================================================================
// Render service: a dedicated thread that OWNS the mupdf Document. The Document
// is !Send (raw pointers), so it can never cross a thread boundary — the thread
// opens it itself and only ever exchanges Send messages with the main thread:
// requests in over an mpsc channel, results out over an async-channel drained on
// the GTK main context. This keeps rasterisation (tens to ~150 ms) entirely off
// the UI loop, so zoom/pan never block.
// ===========================================================================
/// Target width (px) for sidebar page thumbnails.
const THUMB_W: f64 = 180.0;
/// Target long-edge (px) for the coarse full-page preview.
const PREVIEW_LONG_EDGE: f64 = 1600.0;

enum Req {
    Open {
        doc: u64,
        path: String,
    },
    Geometry {
        gen: u64,
        page: usize,
    },
    Render {
        gen: u64,
        id: u64,
        page: usize,
        scale: f64,
        clip: (f64, f64, f64, f64),
    },
    Preview {
        gen: u64,
        page: usize,
    },
    Thumbnail {
        doc: u64,
        page: usize,
    },
}

enum Resp {
    Opened {
        page_count: usize,
        page_sizes: Vec<(f64, f64)>,
    },
    Geometry {
        gen: u64,
        polylines: Vec<Polyline>,
    },
    Rendered {
        gen: u64,
        id: u64,
        bytes: Vec<u8>,
        width: u32,
        height: u32,
        origin: (f64, f64),
        scale: f64,
    },
    Preview {
        gen: u64,
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    Thumbnail {
        doc: u64,
        page: usize,
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    Error(String),
}

fn process(
    req: Req,
    backend: &mut Option<MupdfBackend>,
    doc_id: &mut u64,
    thumbs: &mut VecDeque<usize>,
    out: &async_channel::Sender<Resp>,
) {
    match req {
        Req::Open { doc, path } => {
            *doc_id = doc;
            thumbs.clear(); // a new document invalidates pending thumbnails
            match MupdfBackend::open(&path) {
                Ok(b) => {
                    let n = b.page_count();
                    let sizes = (0..n).map(|i| b.page_size_pts(i)).collect();
                    *backend = Some(b);
                    let _ = out.send_blocking(Resp::Opened {
                        page_count: n,
                        page_sizes: sizes,
                    });
                }
                Err(e) => {
                    let _ = out.send_blocking(Resp::Error(format!("open: {e}")));
                }
            }
        }
        Req::Geometry { gen, page } => {
            if let Some(b) = backend.as_ref() {
                match b.extract_geometry(page) {
                    Ok(polylines) => {
                        let _ = out.send_blocking(Resp::Geometry { gen, polylines });
                    }
                    Err(e) => {
                        let _ = out.send_blocking(Resp::Error(format!("geometry: {e}")));
                    }
                }
            }
        }
        Req::Render {
            gen,
            id,
            page,
            scale,
            clip,
        } => {
            if let Some(b) = backend.as_ref() {
                let t0 = std::time::Instant::now();
                match b.render_region(page, scale, clip) {
                    Ok(img) => {
                        tracing::debug!(
                            "rendered page {page} region {}x{} (scale {scale:.3}) in {:.1} ms (off-thread)",
                            img.width,
                            img.height,
                            t0.elapsed().as_secs_f64() * 1000.0
                        );
                        let _ = out.send_blocking(Resp::Rendered {
                            gen,
                            id,
                            bytes: img.bytes,
                            width: img.width,
                            height: img.height,
                            origin: img.origin,
                            scale: img.scale,
                        });
                    }
                    Err(e) => {
                        let _ = out.send_blocking(Resp::Error(format!("render: {e}")));
                    }
                }
            }
        }
        Req::Preview { gen, page } => {
            if let Some(b) = backend.as_ref() {
                // Whole page at a low fixed long-edge — a soft fallback layer.
                let (pw, ph) = b.page_size_pts(page);
                let long = pw.max(ph).max(1.0);
                let scale = (PREVIEW_LONG_EDGE / long).clamp(0.05, 4.0);
                match b.render_region(page, scale, (0.0, 0.0, pw, ph)) {
                    Ok(img) => {
                        tracing::debug!(
                            "preview rendered page {page} ({}x{})",
                            img.width,
                            img.height
                        );
                        let _ = out.send_blocking(Resp::Preview {
                            gen,
                            bytes: img.bytes,
                            width: img.width,
                            height: img.height,
                        });
                    }
                    Err(e) => {
                        let _ = out.send_blocking(Resp::Error(format!("preview: {e}")));
                    }
                }
            }
        }
        Req::Thumbnail { doc, page } => {
            // Queue only; rendered later when no interactive work is pending.
            if doc == *doc_id {
                thumbs.push_back(page);
            }
        }
    }
}

fn render_thumbnail(
    backend: &Option<MupdfBackend>,
    doc: u64,
    page: usize,
    out: &async_channel::Sender<Resp>,
) {
    let Some(b) = backend.as_ref() else { return };
    let (pw, ph) = b.page_size_pts(page);
    if pw <= 0.0 || ph <= 0.0 {
        return;
    }
    let scale = (THUMB_W / pw).max(0.01);
    match b.render_region(page, scale, (0.0, 0.0, pw, ph)) {
        Ok(img) => {
            let _ = out.send_blocking(Resp::Thumbnail {
                doc,
                page,
                bytes: img.bytes,
                width: img.width,
                height: img.height,
            });
        }
        Err(e) => tracing::warn!("thumbnail page {page}: {e}"),
    }
}

fn spawn_render_thread() -> (mpsc::Sender<Req>, async_channel::Receiver<Resp>) {
    let (req_tx, req_rx) = mpsc::channel::<Req>();
    let (resp_tx, resp_rx) = async_channel::unbounded::<Resp>();
    std::thread::Builder::new()
        .name("vernier-render".into())
        .spawn(move || {
            let mut backend: Option<MupdfBackend> = None;
            let mut doc_id: u64 = 0;
            let mut thumbs: VecDeque<usize> = VecDeque::new();
            // Serve interactive requests first; when the queue is idle, render one
            // pending thumbnail at a time, re-checking for interactive work before
            // each — so live zoom/pan always preempts thumbnail rendering.
            loop {
                let first = if thumbs.is_empty() {
                    match req_rx.recv() {
                        Ok(r) => r,
                        Err(_) => break,
                    }
                } else {
                    match req_rx.try_recv() {
                        Ok(r) => r,
                        Err(mpsc::TryRecvError::Empty) => {
                            if let Some(page) = thumbs.pop_front() {
                                render_thumbnail(&backend, doc_id, page, &resp_tx);
                            }
                            continue;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => break,
                    }
                };
                // Drain the queued batch, coalescing Render to the most recent one
                // (stale zoom levels are worthless); run the rest in order.
                let mut batch = vec![first];
                while let Ok(more) = req_rx.try_recv() {
                    batch.push(more);
                }
                let last_render = batch.iter().rposition(|r| matches!(r, Req::Render { .. }));
                for (i, req) in batch.into_iter().enumerate() {
                    if matches!(req, Req::Render { .. }) && Some(i) != last_render {
                        continue; // superseded by a newer render in this batch
                    }
                    process(req, &mut backend, &mut doc_id, &mut thumbs, &resp_tx);
                }
            }
        })
        .expect("spawn render thread");
    (req_tx, resp_rx)
}

/// How zoom reacts to a resize: the fit modes re-fit the page; Free holds a
/// fixed zoom level (entered by manual zoom / a chosen percentage).
#[derive(Clone, Copy, PartialEq)]
enum ZoomMode {
    FitPage,
    FitWidth,
    Free,
}

/// The page-space region the currently-displayed texture covers, so we can skip
/// re-rendering when the viewport is already covered at the same scale.
#[derive(Clone, Copy)]
struct RenderedInfo {
    gen: u64,
    ox: f64,
    oy: f64,
    scale: f64,
    cw: f64,
    ch: f64,
}

// ===========================================================================
// PdfCanvas: orchestrates the render thread, view state, and the PageView
// widget. Zoom/pan update the transform and redraw instantly (the GPU resamples
// the cached texture); a debounced timer asks the render thread for a crisp
// raster at the settled scale, applied when it arrives — the loop never blocks.
// ===========================================================================
pub struct PdfCanvas {
    pub area: PageView,
    pub state: Rc<RefCell<AppState>>,
    req_tx: mpsc::Sender<Req>,
    page: Cell<usize>,
    page_count: Cell<usize>,
    /// Active zoom mode; the fit modes re-fit on resize and page change.
    zoom_mode: Cell<ZoomMode>,
    /// Page sizes (pts) cached from Open, so fit/clip math needs no Document.
    page_sizes: RefCell<Vec<(f64, f64)>>,
    doc_name: RefCell<String>,
    /// Bumped per document open; thumbnails for an older document are dropped.
    doc_id: Cell<u64>,
    /// Bumped on every page load; results for an older generation are dropped.
    gen: Cell<u64>,
    /// Monotonic id per render request; a Rendered older than the last applied
    /// is ignored (guards against out-of-order completions).
    render_id: Cell<u64>,
    applied_render_id: Cell<u64>,
    /// What the displayed texture covers, to skip redundant same-zoom renders.
    rendered: Cell<Option<RenderedInfo>>,
    #[allow(clippy::type_complexity)]
    page_listener: RefCell<Option<Box<dyn Fn(&str, usize, usize)>>>,
    /// Called on open with the page count, to (re)build the thumbnail sidebar.
    #[allow(clippy::type_complexity)]
    doc_listener: RefCell<Option<Box<dyn Fn(usize)>>>,
    /// Called per thumbnail as it arrives: (page index, thumbnail texture).
    #[allow(clippy::type_complexity)]
    thumb_listener: RefCell<Option<Box<dyn Fn(usize, gtk4::gdk::Texture)>>>,
    /// Called with the zoom readout label ("Fit Page" / "Fit Width" / "NN%").
    #[allow(clippy::type_complexity)]
    zoom_listener: RefCell<Option<Box<dyn Fn(String)>>>,
    /// Pending debounced re-raster request (cancelled/re-armed on each zoom/pan).
    rerender_timer: RefCell<Option<glib::SourceId>>,
}

impl PdfCanvas {
    pub fn new(state: Rc<RefCell<AppState>>) -> Rc<Self> {
        let area = PageView::new(state.clone());
        let (req_tx, resp_rx) = spawn_render_thread();
        let this = Rc::new(PdfCanvas {
            area,
            state,
            req_tx,
            page: Cell::new(0),
            page_count: Cell::new(0),
            zoom_mode: Cell::new(ZoomMode::FitPage),
            page_sizes: RefCell::new(Vec::new()),
            doc_name: RefCell::new(String::new()),
            doc_id: Cell::new(0),
            gen: Cell::new(0),
            render_id: Cell::new(0),
            applied_render_id: Cell::new(0),
            rendered: Cell::new(None),
            page_listener: RefCell::new(None),
            doc_listener: RefCell::new(None),
            thumb_listener: RefCell::new(None),
            zoom_listener: RefCell::new(None),
            rerender_timer: RefCell::new(None),
        });
        // Drain render-thread results on the main context. A weak ref keeps this
        // future from holding the canvas alive on its own.
        let weak = Rc::downgrade(&this);
        glib::spawn_future_local(async move {
            while let Ok(resp) = resp_rx.recv().await {
                let Some(this) = weak.upgrade() else { break };
                this.on_response(resp);
            }
        });
        // Re-fit the page when the canvas is resized (Fit Page / Fit Width modes).
        let weak = Rc::downgrade(&this);
        this.area.set_resize_cb(move |old_w, old_h| {
            let _ = old_w;
            if let Some(this) = weak.upgrade() {
                this.on_resize(old_h as f64);
            }
        });
        this
    }

    /// React to a canvas resize per the active mode. `old_h` is the previous
    /// viewport height, used to keep the viewport centre stationary in Fit Width.
    fn on_resize(self: &Rc<Self>, old_h: f64) {
        match self.zoom_mode.get() {
            ZoomMode::FitPage => self.fit_page(),
            ZoomMode::FitWidth => self.fit_width_around(old_h),
            ZoomMode::Free => {}
        }
    }

    fn on_response(self: &Rc<Self>, resp: Resp) {
        match resp {
            Resp::Opened {
                page_count,
                page_sizes,
            } => {
                self.page_count.set(page_count);
                *self.page_sizes.borrow_mut() = page_sizes;
                if let Some(f) = self.doc_listener.borrow().as_ref() {
                    f(page_count);
                }
                self.load_page(0);
                // Queue every page's thumbnail; the thread renders them only when
                // it has no interactive work, so they can't slow zoom/pan.
                let doc = self.doc_id.get();
                for page in 0..page_count {
                    let _ = self.req_tx.send(Req::Thumbnail { doc, page });
                }
            }
            Resp::Geometry { gen, polylines } => {
                if gen == self.gen.get() {
                    tracing::debug!("snap geometry arrived ({} polylines)", polylines.len());
                    self.state.borrow_mut().set_geometry(polylines);
                }
            }
            Resp::Rendered {
                gen,
                id,
                bytes,
                width,
                height,
                origin,
                scale,
            } => {
                if gen != self.gen.get() || id <= self.applied_render_id.get() {
                    return; // stale page, or superseded by a newer render
                }
                self.applied_render_id.set(id);
                if let Some(tex) = make_texture(bytes, width, height) {
                    self.area.set_page_texture(tex, origin, scale);
                    self.rendered.set(Some(RenderedInfo {
                        gen,
                        ox: origin.0,
                        oy: origin.1,
                        scale,
                        cw: width as f64 / scale,
                        ch: height as f64 / scale,
                    }));
                }
            }
            Resp::Preview {
                gen,
                bytes,
                width,
                height,
            } => {
                if gen == self.gen.get() {
                    if let Some(tex) = make_texture(bytes, width, height) {
                        self.area.set_preview(tex);
                    }
                }
            }
            Resp::Thumbnail {
                doc,
                page,
                bytes,
                width,
                height,
            } => {
                if doc != self.doc_id.get() {
                    return; // belongs to a document we've since closed
                }
                if let (Some(tex), Some(f)) = (
                    make_texture(bytes, width, height),
                    self.thumb_listener.borrow().as_ref(),
                ) {
                    f(page, tex);
                }
            }
            Resp::Error(e) => tracing::warn!("render thread: {e}"),
        }
    }

    pub fn open(self: &Rc<Self>, path: &str) -> anyhow::Result<()> {
        tracing::info!("opening {path}");
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        *self.doc_name.borrow_mut() = name;
        self.doc_id.set(self.doc_id.get() + 1);
        self.req_tx
            .send(Req::Open {
                doc: self.doc_id.get(),
                path: path.to_owned(),
            })
            .map_err(|_| anyhow::anyhow!("render thread is gone"))?;
        Ok(())
    }

    pub fn set_page_listener(&self, f: impl Fn(&str, usize, usize) + 'static) {
        *self.page_listener.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked on open with the page count (to build the sidebar).
    pub fn set_doc_listener(&self, f: impl Fn(usize) + 'static) {
        *self.doc_listener.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked per thumbnail as it arrives (page, texture).
    pub fn set_thumb_listener(&self, f: impl Fn(usize, gtk4::gdk::Texture) + 'static) {
        *self.thumb_listener.borrow_mut() = Some(Box::new(f));
    }

    /// Set the callback invoked with the zoom readout label.
    pub fn set_zoom_listener(&self, f: impl Fn(String) + 'static) {
        *self.zoom_listener.borrow_mut() = Some(Box::new(f));
    }

    fn notify_page(&self) {
        if let Some(f) = self.page_listener.borrow().as_ref() {
            f(&self.doc_name.borrow(), self.page.get(), self.page_count());
        }
    }

    pub fn load_page(self: &Rc<Self>, idx: usize) {
        let Some((pw, ph)) = self.page_sizes.borrow().get(idx).copied() else {
            return;
        };
        self.page.set(idx);
        self.area.set_page_size(pw, ph);
        self.area.clear_preview(); // drop the previous page's preview; white shows until the new one
        self.gen.set(self.gen.get() + 1);
        let gen = self.gen.get();
        let aw = self.area.width().max(1) as f64;
        let ah = self.area.height().max(1) as f64;
        // Position the new page per the active zoom mode. Retain the location
        // across pages (successive sheets are usually the same size), so flipping
        // pages keeps you on the same spot rather than resetting the viewport.
        let (z, pan) = match self.zoom_mode.get() {
            ZoomMode::FitPage => {
                let z = ((aw / pw).min(ah / ph) * 0.97).clamp(0.05, 40.0);
                (
                    z,
                    ScreenPt {
                        x: (aw - pw * z) / 2.0,
                        y: (ah - ph * z) / 2.0,
                    },
                )
            }
            ZoomMode::FitWidth => {
                // Keep the width fit, but retain the vertical scroll position.
                let z = ((aw / pw) * 0.99).clamp(0.05, 40.0);
                let cur_y = self.state.borrow().view().pan.y;
                (
                    z,
                    ScreenPt {
                        x: (aw - pw * z) / 2.0,
                        y: cur_y,
                    },
                )
            }
            ZoomMode::Free => {
                // Retain zoom and pan exactly — same region of the new page.
                let v = self.state.borrow().view();
                (v.zoom, v.pan)
            }
        };
        {
            let mut st = self.state.borrow_mut();
            st.set_zoom(z);
            st.set_pan(pan);
            st.clear_measure();
        }
        let _ = self.req_tx.send(Req::Geometry { gen, page: idx });
        self.request_render();
        let _ = self.req_tx.send(Req::Preview { gen, page: idx });
        tracing::info!("loading page {}/{}", idx + 1, self.page_count());
        self.notify_page();
        self.notify_zoom();
    }

    pub fn page_count(&self) -> usize {
        self.page_count.get()
    }

    pub fn next_page(self: &Rc<Self>) {
        let cur = self.page.get();
        if cur + 1 < self.page_count() {
            self.load_page(cur + 1);
        }
    }

    pub fn prev_page(self: &Rc<Self>) {
        let cur = self.page.get();
        if cur > 0 {
            self.load_page(cur - 1);
        }
    }

    /// Ask the render thread for a crisp raster of the visible region (+margin)
    /// at the current zoom. Non-blocking; the texture is applied when it lands.
    fn request_render(self: &Rc<Self>) {
        let page = self.page.get();
        let Some((pw, ph)) = self.page_sizes.borrow().get(page).copied() else {
            return;
        };
        let v = self.state.borrow().view();
        let scale = v.zoom.max(0.05);
        // Skip if the displayed texture already covers the viewport at this scale
        // (i.e. we've only panned within already-rendered area at the same zoom).
        if self.already_covered(&v, pw, ph, scale) {
            tracing::debug!("render skipped: viewport already covered at scale {scale:.3}");
            return;
        }
        let clip = self.visible_page_rect(&v, pw, ph, 0.5);
        self.render_id.set(self.render_id.get() + 1);
        let _ = self.req_tx.send(Req::Render {
            gen: self.gen.get(),
            id: self.render_id.get(),
            page,
            scale,
            clip,
        });
    }

    /// True if the current texture was rendered for this page at `scale` and
    /// covers the visible (∩ page) region — so a re-render would be redundant.
    fn already_covered(&self, v: &View, pw: f64, ph: f64, scale: f64) -> bool {
        let Some(r) = self.rendered.get() else {
            return false;
        };
        if r.gen != self.gen.get() || (r.scale - scale).abs() > 1e-6 {
            return false; // wrong page, or the zoom changed -> must re-render
        }
        let aw = self.area.width().max(1) as f64;
        let ah = self.area.height().max(1) as f64;
        let tl = v.screen_to_page(ScreenPt { x: 0.0, y: 0.0 });
        let br = v.screen_to_page(ScreenPt { x: aw, y: ah });
        let (vx0, vy0) = (tl.x.max(0.0), tl.y.max(0.0));
        let (vx1, vy1) = (br.x.min(pw), br.y.min(ph));
        let eps = 0.5;
        r.ox <= vx0 + eps
            && r.oy <= vy0 + eps
            && r.ox + r.cw + eps >= vx1
            && r.oy + r.ch + eps >= vy1
    }

    /// Arm (or re-arm) the debounced re-raster request. Called on zoom/pan so we
    /// ask for one crisp raster once the gesture settles, not per event.
    fn schedule_rerender(self: &Rc<Self>) {
        if let Some(id) = self.rerender_timer.borrow_mut().take() {
            id.remove();
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(90), move || {
            *this.rerender_timer.borrow_mut() = None;
            this.request_render();
        });
        *self.rerender_timer.borrow_mut() = Some(id);
    }

    fn visible_page_rect(&self, v: &View, pw: f64, ph: f64, margin: f64) -> (f64, f64, f64, f64) {
        let aw = self.area.width().max(1) as f64;
        let ah = self.area.height().max(1) as f64;
        let tl = v.screen_to_page(ScreenPt { x: 0.0, y: 0.0 });
        let br = v.screen_to_page(ScreenPt { x: aw, y: ah });
        let mx = (br.x - tl.x) * margin;
        let my = (br.y - tl.y) * margin;
        (
            (tl.x - mx).max(0.0),
            (tl.y - my).max(0.0),
            (br.x + mx).min(pw),
            (br.y + my).min(ph),
        )
    }

    fn canvas_center(&self) -> ScreenPt {
        ScreenPt {
            x: self.area.width() as f64 / 2.0,
            y: self.area.height() as f64 / 2.0,
        }
    }

    /// Logical px per page-point for physical "actual size" (100%), derived from
    /// the monitor's reported physical size (EDID — trusting screen calibration).
    fn actual_zoom(&self) -> f64 {
        self.monitor_dpi().unwrap_or(96.0) / 72.0
    }

    fn monitor_dpi(&self) -> Option<f64> {
        let monitor = gtk4::gdk::Display::default()?
            .monitors()
            .item(0)
            .and_downcast::<gtk4::gdk::Monitor>()?;
        let geo = monitor.geometry();
        let wmm = monitor.width_mm();
        if wmm <= 0 || geo.width() <= 0 {
            return None;
        }
        Some(geo.width() as f64 / (wmm as f64 / 25.4))
    }

    /// Current zoom as a percentage of actual (physical) size.
    pub fn zoom_percent(&self) -> f64 {
        self.state.borrow().view().zoom / self.actual_zoom() * 100.0
    }

    /// The zoom readout label: the mode name in a fit mode, else the percentage.
    fn zoom_label(&self) -> String {
        match self.zoom_mode.get() {
            ZoomMode::FitPage => "Fit Page".to_string(),
            ZoomMode::FitWidth => "Fit Width".to_string(),
            ZoomMode::Free => format!("{:.0}%", self.zoom_percent()),
        }
    }

    fn notify_zoom(&self) {
        if let Some(f) = self.zoom_listener.borrow().as_ref() {
            f(self.zoom_label());
        }
    }

    /// Set zoom to an absolute value, keeping `anchor` fixed; redraw instantly
    /// (GPU) and schedule a crisp re-raster. Manual zoom leaves the fit modes.
    fn apply_zoom(self: &Rc<Self>, zoom: f64, anchor: ScreenPt) {
        self.zoom_mode.set(ZoomMode::Free);
        {
            let mut st = self.state.borrow_mut();
            let v = st.view();
            let new_zoom = zoom.clamp(0.05, 40.0);
            let k = new_zoom / v.zoom;
            st.set_pan(ScreenPt {
                x: anchor.x - k * (anchor.x - v.pan.x),
                y: anchor.y - k * (anchor.y - v.pan.y),
            });
            st.set_zoom(new_zoom);
        }
        self.area.queue_draw();
        self.schedule_rerender();
        self.notify_zoom();
    }

    /// Multiply zoom by `factor`, keeping `anchor` fixed (scroll-wheel zoom).
    pub fn zoom_about(self: &Rc<Self>, factor: f64, anchor: ScreenPt) {
        let cur = self.state.borrow().view().zoom;
        self.apply_zoom(cur * factor, anchor);
    }

    pub fn zoom_in(self: &Rc<Self>) {
        self.zoom_about(1.25, self.canvas_center());
    }

    pub fn zoom_out(self: &Rc<Self>) {
        self.zoom_about(1.0 / 1.25, self.canvas_center());
    }

    /// Set zoom to `pct` percent of actual size, about the canvas centre.
    pub fn set_zoom_percent_centered(self: &Rc<Self>, pct: f64) {
        self.apply_zoom(self.actual_zoom() * pct / 100.0, self.canvas_center());
    }

    /// 100% — one page-point at its physical size (trusts the monitor's DPI).
    pub fn zoom_actual(self: &Rc<Self>) {
        self.apply_zoom(self.actual_zoom(), self.canvas_center());
    }

    /// Fit the whole page in the viewport, centred. Becomes the active mode, so
    /// the page re-fits on later window resizes.
    pub fn fit_page(self: &Rc<Self>) {
        self.zoom_mode.set(ZoomMode::FitPage);
        let Some((pw, ph)) = self.page_sizes.borrow().get(self.page.get()).copied() else {
            return;
        };
        let aw = self.area.width().max(1) as f64;
        let ah = self.area.height().max(1) as f64;
        let z = ((aw / pw).min(ah / ph) * 0.97).clamp(0.05, 40.0);
        {
            let mut st = self.state.borrow_mut();
            st.set_zoom(z);
            st.set_pan(ScreenPt {
                x: (aw - pw * z) / 2.0,
                y: (ah - ph * z) / 2.0,
            });
        }
        self.area.queue_draw();
        self.schedule_rerender();
        self.notify_zoom();
    }

    /// Fit the page width to the viewport. Becomes the active mode, so the width
    /// re-fits on later window resizes.
    pub fn fit_width(self: &Rc<Self>) {
        let ah = self.area.height().max(1) as f64;
        self.fit_width_around(ah);
    }

    /// Fit width, keeping the page-point at the centre of a viewport of height
    /// `ref_h` (under the *current* transform) at the centre of the new viewport.
    /// On resize `ref_h` is the previous height, making maximize/unmaximize
    /// return to the same place; from the menu it's the current height.
    fn fit_width_around(self: &Rc<Self>, ref_h: f64) {
        self.zoom_mode.set(ZoomMode::FitWidth);
        let Some((pw, _ph)) = self.page_sizes.borrow().get(self.page.get()).copied() else {
            return;
        };
        let aw = self.area.width().max(1) as f64;
        let ah = self.area.height().max(1) as f64;
        let ref_h = if ref_h > 1.0 { ref_h } else { ah };
        let z = ((aw / pw) * 0.99).clamp(0.05, 40.0);
        {
            let mut st = self.state.borrow_mut();
            let v = st.view();
            let page_cy = (ref_h / 2.0 - v.pan.y) / v.zoom;
            st.set_zoom(z);
            st.set_pan(ScreenPt {
                x: (aw - pw * z) / 2.0,
                y: ah / 2.0 - page_cy * z,
            });
        }
        self.area.queue_draw();
        self.schedule_rerender();
        self.notify_zoom();
    }

    pub fn pan_by(self: &Rc<Self>, dx: f64, dy: f64) {
        {
            let mut st = self.state.borrow_mut();
            let mut pan = st.view().pan;
            pan.x += dx;
            pan.y += dy;
            st.set_pan(pan);
        }
        self.area.queue_draw();
        self.schedule_rerender();
    }
}

/// Wrap RGBA bytes as a GPU texture (one upload, then GPU-resampled per frame).
fn make_texture(bytes: Vec<u8>, width: u32, height: u32) -> Option<gtk4::gdk::Texture> {
    let (w, h) = (width as i32, height as i32);
    if w <= 0 || h <= 0 {
        return None;
    }
    let stride = (width * 4) as usize;
    let bytes = glib::Bytes::from_owned(bytes);
    let tex =
        gtk4::gdk::MemoryTexture::new(w, h, gtk4::gdk::MemoryFormat::R8g8b8a8, &bytes, stride);
    Some(tex.upcast())
}

/// The measurement / calibration overlay, in screen space. Shared by the canvas.
fn draw_overlay(
    cr: &gtk4::cairo::Context,
    widget: &gtk4::Widget,
    app: &AppState,
    v: &View,
    close_rect: &Cell<Option<(f64, f64, f64, f64)>>,
    label_rects: &RefCell<Vec<(f64, f64, f64, f64)>>,
) {
    close_rect.set(None);
    let dims = app.dimensions();
    let hovered = app.hovered_dimension();
    let mut rects = Vec::with_capacity(dims.committed().len());
    for (i, (a, b)) in dims.committed().iter().enumerate() {
        let sa = v.page_to_screen(*a);
        let sb = v.page_to_screen(*b);
        let hot = hovered == Some(i);
        let col = if hot { COL_DIM_HOT } else { COL_DIM };
        halo_line(cr, sa, sb, col, if hot { 4.0 } else { 2.5 }, false);
        dot(cr, sa, col);
        dot(cr, sb, col);
        // The chip is identical hovered or not, so nothing shifts; the delete
        // badge appears OUTSIDE it, in the empty space off its right edge.
        let rect = pill(
            widget,
            cr,
            (sa.x + sb.x) / 2.0,
            (sa.y + sb.y) / 2.0,
            &app.format_len(a.distance(b)),
        );
        rects.push(rect);
        if hot {
            let (x, y, w, h) = rect;
            close_rect.set(Some(close_badge(cr, x + w + 12.0, y + h / 2.0)));
        }
    }
    label_rects.replace(rects);

    match app.active_tool() {
        Tool::Measure => {
            if let (Some(a), Some(c)) = (dims.pending(), app.cursor()) {
                let sa = v.page_to_screen(a);
                let sc = v.page_to_screen(c);
                halo_line(cr, sa, sc, COL_DIM, 2.5, false);
                dot(cr, sa, COL_DIM);
                pill(
                    widget,
                    cr,
                    sc.x + 16.0,
                    sc.y - 16.0,
                    &app.format_len(a.distance(&c)),
                );
            }
        }
        Tool::SetScale => {
            let ss = app.set_scale_tool();
            if let Some((a, b)) = ss.pair() {
                let sa = v.page_to_screen(a);
                let sb = v.page_to_screen(b);
                halo_line(cr, sa, sb, COL_SCALE, 2.5, true);
                dot(cr, sa, COL_SCALE);
                dot(cr, sb, COL_SCALE);
            } else if let (Some(a), Some(c)) = (ss.pending(), app.cursor()) {
                let sa = v.page_to_screen(a);
                let sc = v.page_to_screen(c);
                halo_line(cr, sa, sc, COL_SCALE, 2.5, true);
                dot(cr, sa, COL_SCALE);
            }
        }
    }

    if let Some(c) = app.cursor() {
        let s = v.page_to_screen(c);
        cr.rectangle(s.x - 5.0, s.y - 5.0, 10.0, 10.0);
        cr.set_line_width(4.0);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
        let _ = cr.stroke_preserve();
        cr.set_line_width(2.0);
        cr.set_source_rgb(COL_SNAP.0, COL_SNAP.1, COL_SNAP.2);
        let _ = cr.stroke();
    }
}

/// A line with a dark casing/halo underneath so it reads on any PDF background.
fn halo_line(
    cr: &gtk4::cairo::Context,
    a: ScreenPt,
    b: ScreenPt,
    c: (f64, f64, f64),
    w: f64,
    dashed: bool,
) {
    cr.move_to(a.x, a.y);
    cr.line_to(b.x, b.y);
    cr.set_dash(&[], 0.0);
    cr.set_line_width(w + 3.0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    let _ = cr.stroke_preserve();
    if dashed {
        cr.set_dash(&[8.0, 5.0], 0.0);
    }
    cr.set_line_width(w);
    cr.set_source_rgb(c.0, c.1, c.2);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
}

/// A small round × delete badge centred at (cx, cy). Returns its hit rect,
/// padded for clickability.
fn close_badge(cr: &gtk4::cairo::Context, cx: f64, cy: f64) -> (f64, f64, f64, f64) {
    let r = 9.0;
    cr.arc(cx, cy, r, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.82);
    let _ = cr.fill();
    cr.arc(cx, cy, r, 0.0, std::f64::consts::TAU);
    cr.set_line_width(1.5);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
    let _ = cr.stroke();
    let k = 3.5;
    cr.set_line_width(2.0);
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(cx - k, cy - k);
    cr.line_to(cx + k, cy + k);
    cr.move_to(cx + k, cy - k);
    cr.line_to(cx - k, cy + k);
    let _ = cr.stroke();
    (cx - 11.0, cy - 11.0, 22.0, 22.0)
}

/// A filled endpoint dot with a dark halo ring.
fn dot(cr: &gtk4::cairo::Context, s: ScreenPt, c: (f64, f64, f64)) {
    cr.arc(s.x, s.y, 4.0, 0.0, std::f64::consts::TAU);
    cr.set_line_width(3.0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    let _ = cr.stroke_preserve();
    cr.set_source_rgb(c.0, c.1, c.2);
    let _ = cr.fill();
}

/// A rounded dark pill with white text, centred at (cx, cy). Uses the widget's
/// own font via Pango — system family + size, honouring text-scaling — in bold.
/// Returns the chip's screen rect (x, y, w, h).
fn pill(
    widget: &gtk4::Widget,
    cr: &gtk4::cairo::Context,
    cx: f64,
    cy: f64,
    text: &str,
) -> (f64, f64, f64, f64) {
    let layout = widget.create_pango_layout(Some(text));
    if let Some(mut fd) = widget.pango_context().font_description() {
        fd.set_weight(pango::Weight::Bold);
        layout.set_font_description(Some(&fd));
    }
    let (tw, th) = layout.pixel_size();
    let (tw, th) = (tw as f64, th as f64);
    let pad = 5.0;
    let w = tw + pad * 2.0;
    let h = th + pad * 2.0;
    let x = cx - w / 2.0;
    let y = cy - h / 2.0;
    let r = 5.0;
    use std::f64::consts::{FRAC_PI_2, PI};
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.82);
    let _ = cr.fill();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(cx - tw / 2.0, cy - th / 2.0);
    pangocairo::functions::show_layout(cr, &layout);
    (x, y, w, h)
}
