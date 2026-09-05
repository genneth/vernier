// SPDX-License-Identifier: AGPL-3.0-or-later
//! `PdfCanvas`: orchestrates the render thread, the view, and the `PageView`
//! widget. Zoom/pan update the transform and redraw instantly (the GPU
//! resamples the cached texture); a debounced timer asks the render thread
//! for a crisp raster at the settled scale, applied when it arrives. Nothing
//! here blocks.
//!
//! Everything the window needs to know about comes out as a [`CanvasEvent`]
//! through one handler, so the coupling is visible in a single `match`.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use vernier_core::app::AppState;
use vernier_core::geometry::{PageRect, PageSize, ScreenPt};
use vernier_core::view::View;

use super::page_view::PageView;
use super::render::{self, Image, Req, Resp};

/// Settle time after the last zoom/pan before asking for a crisp re-raster.
const RERENDER_DEBOUNCE: Duration = Duration::from_millis(90);
/// Extra viewport fraction rendered on each side, so small pans need no re-render.
const RENDER_MARGIN: f64 = 0.5;

/// Things that happened on the canvas which the window shows elsewhere.
pub enum CanvasEvent {
    /// A document opened: build the page sidebar.
    DocumentOpened { page_count: usize },
    /// A page thumbnail arrived.
    Thumbnail {
        page: usize,
        texture: gtk4::gdk::Texture,
    },
    /// The current page changed (also fires on open).
    PageChanged {
        doc_name: String,
        index: usize,
        count: usize,
    },
    /// The zoom readout changed ("Fit Page" / "Fit Width" / "NN%").
    ZoomChanged { label: String },
    /// The current page's geometry arrived with this many snappable vertices.
    GeometryLoaded { vertex_count: usize },
    /// Something failed; `what` is user-facing, `detail` is for the log.
    Error { what: String, detail: String },
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
struct Rendered {
    gen: u64,
    scale: f64,
    covers: PageRect,
}

pub struct PdfCanvas {
    pub area: PageView,
    pub state: Rc<RefCell<AppState>>,
    req_tx: mpsc::Sender<Req>,
    page: Cell<usize>,
    /// Page sizes cached from Open, so fit/clip math needs no Document.
    page_sizes: RefCell<Vec<PageSize>>,
    /// Active zoom mode; the fit modes re-fit on resize and page change.
    zoom_mode: Cell<ZoomMode>,
    doc_name: RefCell<String>,
    /// Bumped per document open; thumbnails for an older document are dropped.
    doc_id: Cell<u64>,
    /// Bumped on every page load; results for an older generation are dropped.
    gen: Cell<u64>,
    /// Monotonic id per render request; a Rendered older than the last applied
    /// is ignored (guards against out-of-order completions).
    render_id: Cell<u64>,
    applied_render_id: Cell<u64>,
    rendered: Cell<Option<Rendered>>,
    #[allow(clippy::type_complexity)]
    handler: RefCell<Option<Box<dyn Fn(CanvasEvent)>>>,
    /// Pending debounced re-raster request (cancelled/re-armed on each zoom/pan).
    rerender_timer: RefCell<Option<glib::SourceId>>,
}

impl PdfCanvas {
    pub fn new(state: Rc<RefCell<AppState>>) -> Rc<Self> {
        let area = PageView::new(state.clone());
        let (req_tx, resp_rx) = render::spawn();
        let this = Rc::new(PdfCanvas {
            area,
            state,
            req_tx,
            page: Cell::new(0),
            page_sizes: RefCell::new(Vec::new()),
            zoom_mode: Cell::new(ZoomMode::FitPage),
            doc_name: RefCell::new(String::new()),
            doc_id: Cell::new(0),
            gen: Cell::new(0),
            render_id: Cell::new(0),
            applied_render_id: Cell::new(0),
            rendered: Cell::new(None),
            handler: RefCell::new(None),
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
            if let Some(this) = weak.upgrade() {
                this.emit(CanvasEvent::Error {
                    what: "The rendering thread stopped; reopen the file".into(),
                    detail: "render thread response channel closed".into(),
                });
            }
        });
        // Re-fit the page when the canvas is resized (Fit Page / Fit Width modes).
        let weak = Rc::downgrade(&this);
        this.area.set_resize_cb(move |old_h| {
            if let Some(this) = weak.upgrade() {
                match this.zoom_mode.get() {
                    ZoomMode::FitPage => this.fit_page(),
                    ZoomMode::FitWidth => this.fit_width_around(old_h),
                    ZoomMode::Free => {}
                }
            }
        });
        this
    }

    /// Install the single event handler (the window's `match`).
    pub fn set_handler(&self, f: impl Fn(CanvasEvent) + 'static) {
        *self.handler.borrow_mut() = Some(Box::new(f));
    }

    fn emit(&self, ev: CanvasEvent) {
        if let CanvasEvent::Error { what, detail } = &ev {
            tracing::error!("{what}: {detail}");
        }
        if let Some(f) = self.handler.borrow().as_ref() {
            f(ev);
        }
    }

    /// Send a request to the render thread; a dead thread is reported, not ignored.
    fn request(&self, req: Req) {
        if self.req_tx.send(req).is_err() {
            self.emit(CanvasEvent::Error {
                what: "The rendering thread stopped; reopen the file".into(),
                detail: "render thread request channel closed".into(),
            });
        }
    }

    // ---- document ---------------------------------------------------------

    pub fn open(self: &Rc<Self>, path: &str) {
        tracing::info!("opening {path}");
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        *self.doc_name.borrow_mut() = name;
        self.doc_id.set(self.doc_id.get() + 1);
        self.request(Req::Open {
            doc: self.doc_id.get(),
            path: path.to_owned(),
        });
    }

    fn on_response(self: &Rc<Self>, resp: Resp) {
        match resp {
            Resp::Opened { doc, page_sizes } => {
                if doc != self.doc_id.get() {
                    return; // an older open, superseded
                }
                let page_count = page_sizes.len();
                *self.page_sizes.borrow_mut() = page_sizes;
                self.emit(CanvasEvent::DocumentOpened { page_count });
                self.load_page(0);
                // Queue every page's thumbnail; the thread renders them only when
                // it has no interactive work, so they can't slow zoom/pan.
                for page in 0..page_count {
                    self.request(Req::Thumbnail { doc, page });
                }
            }
            Resp::Geometry { gen, polylines } => {
                if gen != self.gen.get() {
                    return;
                }
                self.state.borrow_mut().set_geometry(&polylines);
                let vertex_count = self.state.borrow().snap_vertex_count();
                self.emit(CanvasEvent::GeometryLoaded { vertex_count });
            }
            Resp::Rendered {
                gen,
                id,
                image,
                origin,
                scale,
            } => {
                if gen != self.gen.get() || id <= self.applied_render_id.get() {
                    return; // stale page, or superseded by a newer render
                }
                self.applied_render_id.set(id);
                let covers = PageRect {
                    x0: origin.x,
                    y0: origin.y,
                    x1: origin.x + image.width as f64 / scale,
                    y1: origin.y + image.height as f64 / scale,
                };
                if let Some(tex) = make_texture(image) {
                    self.area.set_page_texture(tex, origin, scale);
                    self.rendered.set(Some(Rendered { gen, scale, covers }));
                }
            }
            Resp::Preview { gen, image } => {
                if gen == self.gen.get() {
                    if let Some(tex) = make_texture(image) {
                        self.area.set_preview(tex);
                    }
                }
            }
            Resp::Thumbnail { doc, page, image } => {
                if doc != self.doc_id.get() {
                    return; // belongs to a document we've since closed
                }
                if let Some(texture) = make_texture(image) {
                    self.emit(CanvasEvent::Thumbnail { page, texture });
                }
            }
            Resp::Error { what, message } => self.emit(CanvasEvent::Error {
                what: what.into(),
                detail: message,
            }),
        }
    }

    // ---- pages ------------------------------------------------------------

    pub fn page_count(&self) -> usize {
        self.page_sizes.borrow().len()
    }

    fn current_page_size(&self) -> Option<PageSize> {
        self.page_sizes.borrow().get(self.page.get()).copied()
    }

    pub fn load_page(self: &Rc<Self>, idx: usize) {
        let Some(size) = self.page_sizes.borrow().get(idx).copied() else {
            return;
        };
        self.page.set(idx);
        self.area.begin_page(size);
        self.gen.set(self.gen.get() + 1);
        let gen = self.gen.get();
        let (aw, ah) = self.area.viewport();
        // Position the new page per the active zoom mode. Retain the location
        // across pages (successive sheets are usually the same size), so flipping
        // pages keeps you on the same spot rather than resetting the viewport.
        let cur = self.state.borrow().view();
        let view = match self.zoom_mode.get() {
            ZoomMode::FitPage => View::fit_page(size, aw, ah),
            ZoomMode::FitWidth => {
                // Keep the width fit, but retain the vertical scroll position.
                let zoom = View::fit_width_zoom(size, aw);
                View {
                    zoom,
                    pan: ScreenPt {
                        x: (aw - size.w * zoom) / 2.0,
                        y: cur.pan.y,
                    },
                }
            }
            ZoomMode::Free => cur,
        };
        {
            let mut st = self.state.borrow_mut();
            st.set_view(view);
            st.clear_measure();
            st.clear_geometry();
        }
        self.request(Req::Geometry { gen, page: idx });
        self.request_render();
        self.request(Req::Preview { gen, page: idx });
        tracing::info!("loading page {}/{}", idx + 1, self.page_count());
        self.emit(CanvasEvent::PageChanged {
            doc_name: self.doc_name.borrow().clone(),
            index: idx,
            count: self.page_count(),
        });
        self.notify_zoom();
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

    // ---- rendering --------------------------------------------------------

    /// Ask the render thread for a crisp raster of the visible region (+margin)
    /// at the current zoom. Non-blocking; the texture is applied when it lands.
    fn request_render(self: &Rc<Self>) {
        let Some(size) = self.current_page_size() else {
            return;
        };
        let v = self.state.borrow().view();
        let scale = v.zoom;
        let (aw, ah) = self.area.viewport();
        let visible = v.visible_page_rect(aw, ah).clamp_to(size);
        // Skip if the displayed texture already covers the viewport at this scale
        // (i.e. we've only panned within already-rendered area at the same zoom).
        if self.already_covered(visible, scale) {
            tracing::debug!("render skipped: viewport already covered at scale {scale:.3}");
            return;
        }
        let mx = visible.width() * RENDER_MARGIN;
        let my = visible.height() * RENDER_MARGIN;
        let clip = PageRect {
            x0: visible.x0 - mx,
            y0: visible.y0 - my,
            x1: visible.x1 + mx,
            y1: visible.y1 + my,
        }
        .clamp_to(size);
        self.render_id.set(self.render_id.get() + 1);
        self.request(Req::Render {
            gen: self.gen.get(),
            id: self.render_id.get(),
            page: self.page.get(),
            scale,
            clip,
        });
    }

    /// True if the current texture was rendered for this page at `scale` and
    /// covers `visible` — so a re-render would be redundant.
    fn already_covered(&self, visible: PageRect, scale: f64) -> bool {
        let Some(r) = self.rendered.get() else {
            return false;
        };
        if r.gen != self.gen.get() || (r.scale - scale).abs() > 1e-6 {
            return false; // wrong page, or the zoom changed -> must re-render
        }
        let eps = 0.5;
        r.covers.x0 <= visible.x0 + eps
            && r.covers.y0 <= visible.y0 + eps
            && r.covers.x1 + eps >= visible.x1
            && r.covers.y1 + eps >= visible.y1
    }

    /// Arm (or re-arm) the debounced re-raster request. Called on zoom/pan so we
    /// ask for one crisp raster once the gesture settles, not per event.
    fn schedule_rerender(self: &Rc<Self>) {
        if let Some(id) = self.rerender_timer.borrow_mut().take() {
            id.remove();
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(RERENDER_DEBOUNCE, move || {
            *this.rerender_timer.borrow_mut() = None;
            this.request_render();
        });
        *self.rerender_timer.borrow_mut() = Some(id);
    }

    // ---- zoom / pan -------------------------------------------------------

    fn canvas_center(&self) -> ScreenPt {
        let (w, h) = self.area.viewport();
        ScreenPt {
            x: w / 2.0,
            y: h / 2.0,
        }
    }

    /// Logical px per page-point for physical "actual size" (100%), from the
    /// physical size the monitor showing this window reports (EDID).
    fn actual_zoom(&self) -> f64 {
        self.monitor_dpi().unwrap_or(96.0) / 72.0
    }

    fn monitor_dpi(&self) -> Option<f64> {
        let display = self.area.display();
        let surface = self.area.native()?.surface()?;
        let monitor = display.monitor_at_surface(&surface)?;
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

    fn notify_zoom(&self) {
        let label = match self.zoom_mode.get() {
            ZoomMode::FitPage => "Fit Page".to_string(),
            ZoomMode::FitWidth => "Fit Width".to_string(),
            ZoomMode::Free => format!("{:.0}%", self.zoom_percent()),
        };
        self.emit(CanvasEvent::ZoomChanged { label });
    }

    /// Install `view`, redraw instantly (GPU) and schedule a crisp re-raster.
    fn apply_view(self: &Rc<Self>, mode: ZoomMode, view: View) {
        self.zoom_mode.set(mode);
        self.state.borrow_mut().set_view(view);
        self.area.queue_draw();
        self.schedule_rerender();
        self.notify_zoom();
    }

    /// Set zoom to an absolute value, keeping `anchor` fixed. Manual zoom
    /// leaves the fit modes.
    fn apply_zoom(self: &Rc<Self>, zoom: f64, anchor: ScreenPt) {
        let v = self.state.borrow().view();
        self.apply_view(ZoomMode::Free, v.zoomed_about(zoom, anchor));
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
        let Some(size) = self.current_page_size() else {
            return;
        };
        let (aw, ah) = self.area.viewport();
        self.apply_view(ZoomMode::FitPage, View::fit_page(size, aw, ah));
    }

    /// Fit the page width to the viewport. Becomes the active mode, so the width
    /// re-fits on later window resizes.
    pub fn fit_width(self: &Rc<Self>) {
        let (_, ah) = self.area.viewport();
        self.fit_width_around(ah);
    }

    /// Fit width, keeping the page-point at the centre of a viewport of height
    /// `ref_h` (under the *current* transform) at the centre of the new viewport.
    /// On resize `ref_h` is the previous height, making maximize/unmaximize
    /// return to the same place; from the menu it's the current height.
    fn fit_width_around(self: &Rc<Self>, ref_h: f64) {
        self.zoom_mode.set(ZoomMode::FitWidth);
        let Some(size) = self.current_page_size() else {
            return;
        };
        let (aw, ah) = self.area.viewport();
        let ref_h = if ref_h > 1.0 { ref_h } else { ah };
        let zoom = View::fit_width_zoom(size, aw);
        let v = self.state.borrow().view();
        let page_cy = (ref_h / 2.0 - v.pan.y) / v.zoom;
        let view = View {
            zoom,
            pan: ScreenPt {
                x: (aw - size.w * zoom) / 2.0,
                y: ah / 2.0 - page_cy * zoom,
            },
        };
        self.apply_view(ZoomMode::FitWidth, view);
    }

    pub fn pan_by(self: &Rc<Self>, dx: f64, dy: f64) {
        let v = self.state.borrow().view().panned_by(dx, dy);
        self.state.borrow_mut().set_view(v);
        self.area.queue_draw();
        self.schedule_rerender();
    }
}

/// Wrap RGBA bytes as a GPU texture (one upload, then GPU-resampled per frame).
fn make_texture(img: Image) -> Option<gtk4::gdk::Texture> {
    let (w, h) = (img.width as i32, img.height as i32);
    if w <= 0 || h <= 0 {
        return None;
    }
    let stride = (img.width * 4) as usize;
    let bytes = glib::Bytes::from_owned(img.bytes);
    let tex =
        gtk4::gdk::MemoryTexture::new(w, h, gtk4::gdk::MemoryFormat::R8g8b8a8, &bytes, stride);
    Some(tex.upcast())
}
