// SPDX-License-Identifier: AGPL-3.0-or-later
//! `PageView`: the canvas widget. Its `snapshot()` presents the rendered page
//! as GPU textures placed under the current `View` transform (so pan/zoom are
//! GPU work, not per-frame CPU rasterisation) and paints the Cairo overlay on
//! top in the same transform, so snap markers stay aligned to the page.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use vernier_core::app::AppState;
use vernier_core::geometry::{PagePt, PageSize, ScreenRect};
use vernier_core::tools::dimension::DimId;

use super::overlay;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PageView {
        pub state: RefCell<Option<Rc<RefCell<AppState>>>>,
        /// The crisp texture of the rendered viewport region, with the page
        /// point at its top-left and the px-per-point it was rendered at.
        pub texture: RefCell<Option<(gtk4::gdk::Texture, PagePt, f64)>>,
        /// Coarse full-page preview, drawn (soft) under the crisp texture so pan/
        /// zoom never reveals blank page — it sharpens as crisp rasters land.
        pub preview: RefCell<Option<gtk4::gdk::Texture>>,
        /// Full page size, so the page extent and preview can be placed.
        pub page_size: Cell<Option<PageSize>>,
        /// What the last overlay draw painted, for hit-testing.
        pub close_rect: Cell<Option<ScreenRect>>,
        pub label_rects: RefCell<Vec<(DimId, ScreenRect)>>,
        /// Last allocated size + a callback fired (with the previous height) when
        /// it changes — drives the re-fit-on-resize of the fit zoom modes.
        pub last_alloc: Cell<(i32, i32)>,
        #[allow(clippy::type_complexity)]
        pub resize_cb: RefCell<Option<Box<dyn Fn(f64)>>>,
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
            if let Some(size) = self.page_size.get() {
                let tl = v.page_to_screen(PagePt { x: 0.0, y: 0.0 });
                let page_rect = graphene::Rect::new(
                    tl.x as f32,
                    tl.y as f32,
                    (size.w * v.zoom) as f32,
                    (size.h * v.zoom) as f32,
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
            if let Some((tex, origin, rs)) = self.texture.borrow().as_ref() {
                let tl = v.page_to_screen(*origin);
                let s = v.zoom / rs;
                let dw = tex.width() as f64 * s;
                let dh = tex.height() as f64 * s;
                snapshot.append_texture(
                    tex,
                    &graphene::Rect::new(tl.x as f32, tl.y as f32, dw as f32, dh as f32),
                );
            }

            let cr = snapshot.append_cairo(&graphene::Rect::new(0.0, 0.0, w, h));
            let painted = overlay::draw(&cr, widget.upcast_ref::<gtk4::Widget>(), &app);
            self.close_rect.set(painted.close_rect);
            self.label_rects.replace(painted.label_rects);
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            let old = self.last_alloc.get();
            if old != (width, height) {
                self.last_alloc.set((width, height));
                if let Some(cb) = self.resize_cb.borrow().as_ref() {
                    cb(old.1 as f64);
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
    pub fn new(state: Rc<RefCell<AppState>>) -> Self {
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
    pub fn close_rect(&self) -> Option<ScreenRect> {
        self.imp().close_rect.get()
    }

    /// Screen rects of all dimension label chips, from the last overlay draw.
    pub fn label_rects(&self) -> Vec<(DimId, ScreenRect)> {
        self.imp().label_rects.borrow().clone()
    }

    /// The viewport size in logical pixels (at least 1×1).
    pub fn viewport(&self) -> (f64, f64) {
        (self.width().max(1) as f64, self.height().max(1) as f64)
    }

    pub(super) fn set_page_texture(&self, tex: gtk4::gdk::Texture, origin: PagePt, scale: f64) {
        *self.imp().texture.borrow_mut() = Some((tex, origin, scale));
        self.queue_draw();
    }

    /// Register a callback fired (with the previous viewport height) whenever
    /// the widget's allocated size changes.
    pub(super) fn set_resize_cb(&self, f: impl Fn(f64) + 'static) {
        *self.imp().resize_cb.borrow_mut() = Some(Box::new(f));
    }

    /// Start showing a new page: its extent, and no textures yet (white paper
    /// shows until the preview lands).
    pub(super) fn begin_page(&self, size: PageSize) {
        let imp = self.imp();
        imp.page_size.set(Some(size));
        *imp.preview.borrow_mut() = None;
        *imp.texture.borrow_mut() = None;
        self.queue_draw();
    }

    pub(super) fn set_preview(&self, tex: gtk4::gdk::Texture) {
        *self.imp().preview.borrow_mut() = Some(tex);
        self.queue_draw();
    }
}
