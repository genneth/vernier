// SPDX-License-Identifier: AGPL-3.0-or-later
mod canvas;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk4::gio;
use libadwaita as adw;
use vernier_core::app::{AppState, Tool};
use vernier_core::geometry::ScreenPt;
use vernier_core::scale::{self, Scale, Unit};

use canvas::PdfCanvas;

const UNITS: [Unit; 4] = [Unit::Mm, Unit::M, Unit::Ft, Unit::In];

pub fn build_window(app: &adw::Application, open_path: Option<String>) {
    let state = Rc::new(RefCell::new(AppState::new()));
    let canvas = PdfCanvas::new(state.clone());

    let header = adw::HeaderBar::new();
    let window_title = adw::WindowTitle::new("Vernier", "");
    header.set_title_widget(Some(&window_title));

    // ---- header start: sidebar toggle + Open ----
    let sidebar_toggle = gtk4::ToggleButton::new();
    sidebar_toggle.set_icon_name("sidebar-show-symbolic");
    sidebar_toggle.update_property(&[gtk4::accessible::Property::Label("Toggle page sidebar")]);
    header.pack_start(&sidebar_toggle);

    let open_btn = gtk4::Button::with_label("Open");
    open_btn.update_property(&[gtk4::accessible::Property::Label("Open")]);
    header.pack_start(&open_btn);

    // ---- header end: primary menu + Set scale ----
    let menu = gio::Menu::new();
    menu.append(Some("Clear Measurements"), Some("win.clear"));
    menu.append(Some("About Vernier"), Some("win.about"));
    let menu_btn = gtk4::MenuButton::new();
    menu_btn.set_icon_name("open-menu-symbolic");
    menu_btn.set_menu_model(Some(&menu));
    menu_btn.update_property(&[gtk4::accessible::Property::Label("Main Menu")]);
    header.pack_end(&menu_btn);

    let scale_btn = gtk4::MenuButton::new();
    scale_btn.set_label("Set scale");
    scale_btn.update_property(&[gtk4::accessible::Property::Label("Set scale")]);
    let (scale_pop, ratio_entry, ratio_apply, len_entry, unit_dd, len_apply, pick_btn) =
        build_scale_popover();
    // Stays open while you click points on the canvas.
    scale_pop.set_autohide(false);
    scale_btn.set_popover(Some(&scale_pop));
    header.pack_end(&scale_btn);

    // ---- header end: zoom control [-] [NN% v] [+] ----
    let zoom_out_btn = gtk4::Button::from_icon_name("zoom-out-symbolic");
    zoom_out_btn.update_property(&[gtk4::accessible::Property::Label("Zoom out")]);
    zoom_out_btn.set_tooltip_text(Some("Zoom out"));
    let zoom_in_btn = gtk4::Button::from_icon_name("zoom-in-symbolic");
    zoom_in_btn.update_property(&[gtk4::accessible::Property::Label("Zoom in")]);
    zoom_in_btn.set_tooltip_text(Some("Zoom in"));
    let zoom_label_btn = gtk4::MenuButton::new();
    zoom_label_btn.set_label("—");
    zoom_label_btn.update_property(&[gtk4::accessible::Property::Label("Zoom level")]);
    zoom_label_btn.set_popover(Some(&build_zoom_popover(&canvas)));

    let zoom_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    zoom_box.add_css_class("linked");
    zoom_box.append(&zoom_out_btn);
    zoom_box.append(&zoom_label_btn);
    zoom_box.append(&zoom_in_btn);
    header.pack_end(&zoom_box);

    {
        let cb = canvas.clone();
        zoom_out_btn.connect_clicked(move |_| cb.zoom_out());
    }
    {
        let cb = canvas.clone();
        zoom_in_btn.connect_clicked(move |_| cb.zoom_in());
    }
    {
        // Keep the readout in sync with the live zoom level / fit mode.
        let zb = zoom_label_btn.clone();
        canvas.set_zoom_listener(move |label| zb.set_label(&label));
    }

    // ---- thumbnail sidebar in an overlay split view ----
    let sidebar_list = gtk4::ListBox::new();
    sidebar_list.add_css_class("navigation-sidebar");
    sidebar_list.update_property(&[gtk4::accessible::Property::Label("Pages")]);
    let sidebar_scroller = gtk4::ScrolledWindow::new();
    sidebar_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    sidebar_scroller.set_child(Some(&sidebar_list));
    sidebar_scroller.set_vexpand(true);

    // Page-nav footer, grouped with the thumbnails. Up/down because pages stack
    // vertically; the page number itself stays in the header subtitle.
    let page_up = gtk4::Button::from_icon_name("go-up-symbolic");
    page_up.update_property(&[gtk4::accessible::Property::Label("Previous page")]);
    page_up.set_tooltip_text(Some("Previous page"));
    page_up.add_css_class("flat");
    page_up.set_hexpand(true);
    let page_down = gtk4::Button::from_icon_name("go-down-symbolic");
    page_down.update_property(&[gtk4::accessible::Property::Label("Next page")]);
    page_down.set_tooltip_text(Some("Next page"));
    page_down.add_css_class("flat");
    page_down.set_hexpand(true);
    let nav_footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    nav_footer.add_css_class("toolbar");
    nav_footer.append(&page_up);
    nav_footer.append(&page_down);

    let sidebar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    sidebar_box.append(&sidebar_scroller);
    sidebar_box.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    sidebar_box.append(&nav_footer);

    let split = adw::OverlaySplitView::new();
    split.set_sidebar(Some(&sidebar_box));
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&canvas.area));
    split.set_content(Some(&toasts));
    split.set_min_sidebar_width(180.0);
    split.set_max_sidebar_width(220.0);
    split.set_show_sidebar(false);
    split.set_vexpand(true);

    // Toggle button <-> sidebar visibility, kept in sync both ways.
    sidebar_toggle
        .bind_property("active", &split, "show-sidebar")
        .bidirectional()
        .sync_create()
        .build();

    // Thumbnails, kept so each arrival can fill the right slot.
    let thumb_pics: Rc<RefCell<Vec<gtk4::Picture>>> = Rc::new(RefCell::new(Vec::new()));

    // On open: rebuild the sidebar with one row per page; auto-show if multipage.
    {
        let lb = sidebar_list.clone();
        let pics = thumb_pics.clone();
        let split = split.clone();
        canvas.set_doc_listener(move |count| {
            while let Some(row) = lb.row_at_index(0) {
                lb.remove(&row);
            }
            pics.borrow_mut().clear();
            for page in 0..count {
                let (row, pic) = make_thumb_row(page);
                lb.append(&row);
                pics.borrow_mut().push(pic);
            }
            split.set_show_sidebar(count > 1);
        });
    }

    // Fill each thumbnail texture as it arrives from the render thread.
    {
        let pics = thumb_pics.clone();
        canvas.set_thumb_listener(move |page, tex| {
            if let Some(pic) = pics.borrow().get(page) {
                pic.set_paintable(Some(&tex));
            }
        });
    }

    // Click a thumbnail -> jump to that page.
    {
        let cb = canvas.clone();
        sidebar_list.connect_row_activated(move |_, row| {
            let idx = row.index();
            if idx >= 0 {
                cb.load_page(idx as usize);
            }
        });
    }

    // Title + current-page highlight follow the active page.
    {
        let wt = window_title.clone();
        let lb = sidebar_list.clone();
        canvas.set_page_listener(move |name, idx, count| {
            wt.set_title(if name.is_empty() { "Vernier" } else { name });
            wt.set_subtitle(&format!("Page {} of {}", idx + 1, count));
            if let Some(row) = lb.row_at_index(idx as i32) {
                lb.select_row(Some(&row));
            }
        });
    }

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&split);

    // ---- Open (env seam for headless tests, else file picker) ----
    {
        let canvas = canvas.clone();
        open_btn.connect_clicked(move |_| {
            if let Ok(path) = std::env::var("VERNIER_AUTO_OPEN") {
                if let Err(e) = canvas.open(&path) {
                    tracing::error!("open failed: {e}");
                }
                return;
            }
            let dialog = gtk4::FileDialog::builder().title("Open PDF").build();
            let canvas = canvas.clone();
            dialog.open(None::<&gtk4::Window>, gtk4::gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        if let Err(e) = canvas.open(&path.to_string_lossy()) {
                            tracing::error!("open failed: {e}");
                        }
                    }
                }
            });
        });
    }

    let cursor = Rc::new(Cell::new(ScreenPt { x: 0.0, y: 0.0 }));

    // Pointer motion: snap (both tools) + redraw.
    {
        let cb = canvas.clone();
        let cursor = cursor.clone();
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion(move |_, x, y| {
            let sp = ScreenPt { x, y };
            cursor.set(sp);
            {
                let mut st = cb.state.borrow_mut();
                // Feed the chip rects from the last draw into the hover
                // hit-test (hovering a label = hovering its dimension).
                st.set_label_rects(cb.area.label_rects());
                st.on_pointer_move(sp);
            }
            cb.area.queue_draw();
        });
        canvas.area.add_controller(motion);
    }

    // Left click: place a (snapped) dimension / scale point — unless it lands
    // on the hovered dimension's × delete glyph.
    {
        let cb = canvas.clone();
        let toasts = toasts.clone();
        let click = gtk4::GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click.connect_pressed(move |_, _, x, y| {
            if let Some((rx, ry, rw, rh)) = cb.area.close_rect() {
                if x >= rx && x <= rx + rw && y >= ry && y <= ry + rh {
                    delete_hovered(&cb, &toasts);
                    return;
                }
            }
            {
                let mut st = cb.state.borrow_mut();
                let tool = st.active_tool();
                st.on_click(ScreenPt { x, y });
                match tool {
                    Tool::Measure => {
                        if st.dimensions().pending().is_some() {
                            tracing::info!("dimension started");
                        } else if let Some((a, b)) = st.dimensions().committed().last() {
                            tracing::info!("dimension placed: {}", st.format_len(a.distance(b)));
                        }
                    }
                    Tool::SetScale => {
                        tracing::debug!("scale point placed (ready: {})", st.set_scale_tool().is_ready());
                    }
                }
            }
            cb.area.queue_draw();
        });
        canvas.area.add_controller(click);
    }

    // Right click: delete the hovered dimension (undo via toast).
    {
        let cb = canvas.clone();
        let toasts = toasts.clone();
        let rclick = gtk4::GestureClick::new();
        rclick.set_button(gtk4::gdk::BUTTON_SECONDARY);
        rclick.connect_pressed(move |_, _, _, _| {
            delete_hovered(&cb, &toasts);
        });
        canvas.area.add_controller(rclick);
    }

    // Scroll = zoom about the cursor.
    {
        let cb = canvas.clone();
        let cursor = cursor.clone();
        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(move |_, _dx, dy| {
            let factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
            cb.zoom_about(factor, cursor.get());
            glib::Propagation::Stop
        });
        canvas.area.add_controller(scroll);
    }

    // Middle-button drag = pan.
    {
        let last = Rc::new(Cell::new((0.0_f64, 0.0_f64)));
        let drag = gtk4::GestureDrag::new();
        drag.set_button(gtk4::gdk::BUTTON_MIDDLE);
        {
            let last = last.clone();
            drag.connect_drag_begin(move |_, _, _| last.set((0.0, 0.0)));
        }
        {
            let cb = canvas.clone();
            let last = last.clone();
            drag.connect_drag_update(move |_, ox, oy| {
                let (lx, ly) = last.get();
                cb.pan_by(ox - lx, oy - ly);
                last.set((ox, oy));
            });
        }
        canvas.area.add_controller(drag);
    }

    // Page navigation (sidebar footer).
    {
        let cb = canvas.clone();
        page_up.connect_clicked(move |_| cb.prev_page());
    }
    {
        let cb = canvas.clone();
        page_down.connect_clicked(move |_| cb.next_page());
    }

    // ---- Scale popover wiring ----
    // Ratio: "1 : N" -> Scale::from_ratio (uses the chosen display unit).
    {
        let cb = canvas.clone();
        let entry = ratio_entry.clone();
        let dd = unit_dd.clone();
        let pop = scale_pop.clone();
        let apply: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(r) = scale::parse_ratio(entry.text().as_str()) {
                let unit = UNITS[dd.selected() as usize];
                cb.state.borrow_mut().set_scale(Scale::from_ratio(r, unit));
                tracing::info!("scale set from ratio 1:{r} ({})", unit.suffix());
                pop.popdown();
                cb.area.queue_draw();
            } else {
                tracing::warn!("could not parse ratio {:?}", entry.text());
            }
        });
        let f = apply.clone();
        ratio_entry.connect_activate(move |_| f());
        ratio_apply.connect_clicked(move |_| apply());
    }
    // Measured: length + unit dropdown -> Scale::from_measurement on the two points.
    {
        let cb = canvas.clone();
        let entry = len_entry.clone();
        let dd = unit_dd.clone();
        let pop = scale_pop.clone();
        let apply: Rc<dyn Fn()> = Rc::new(move || {
            match entry.text().trim().parse::<f64>() {
                Ok(val) => {
                    let unit = UNITS[dd.selected() as usize];
                    cb.state.borrow_mut().finish_set_scale(val, unit);
                    if cb.state.borrow().scale().is_some() {
                        tracing::info!("scale set from measurement");
                        pop.popdown();
                    } else {
                        tracing::warn!("need two scale points before applying a length");
                    }
                    cb.area.queue_draw();
                }
                Err(_) => tracing::warn!("could not parse length {:?}", entry.text()),
            }
        });
        let f = apply.clone();
        len_entry.connect_activate(move |_| f());
        len_apply.connect_clicked(move |_| apply());
    }
    // "Pick two points" is the explicit entry into measured calibration; the
    // ratio path applies directly without changing tools.
    {
        let cb = canvas.clone();
        pick_btn.connect_clicked(move |_| {
            cb.state.borrow_mut().begin_set_scale();
            cb.area.queue_draw();
        });
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(1100)
        .default_height(800)
        .content(&content)
        .build();

    // Primary-menu actions.
    let clear = gio::SimpleAction::new("clear", None);
    {
        let cb = canvas.clone();
        clear.connect_activate(move |_, _| {
            cb.state.borrow_mut().clear_measure();
            cb.area.queue_draw();
        });
    }
    window.add_action(&clear);

    let about = gio::SimpleAction::new("about", None);
    {
        let win = window.clone();
        about.connect_activate(move |_, _| {
            let dialog = adw::AboutDialog::builder()
                .application_name("Vernier")
                .application_icon("io.github.genneth.Vernier")
                .version(env!("CARGO_PKG_VERSION"))
                .developer_name("genneth")
                .license_type(gtk4::License::Agpl30)
                .comments("Measure and take off quantities from PDF drawings.")
                .build();
            dialog.present(Some(&win));
        });
    }
    window.add_action(&about);

    // Keyboard: Escape cancels; PageUp/Down flip pages; Ctrl +/-/0 zoom.
    {
        let cb = canvas.clone();
        let key = gtk4::EventControllerKey::new();
        key.connect_key_pressed(move |_, keyval, _, modifier| {
            use gtk4::gdk::Key;
            let ctrl = modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            match keyval {
                Key::Escape => {
                    cb.state.borrow_mut().cancel();
                    cb.area.queue_draw();
                }
                Key::Page_Up => cb.prev_page(),
                Key::Page_Down => cb.next_page(),
                Key::_0 if ctrl => cb.fit_page(),
                Key::plus | Key::equal if ctrl => cb.zoom_in(),
                Key::minus if ctrl => cb.zoom_out(),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        window.add_controller(key);
    }
    window.present();

    // "Open with…" / file argument: open the PDF once the window is up.
    if let Some(path) = open_path {
        if let Err(e) = canvas.open(&path) {
            tracing::error!("open failed: {e}");
        }
    }
}

/// The "Set scale" popover: a stated-ratio row and a measured-length row.
#[allow(clippy::type_complexity)]
fn build_scale_popover() -> (
    gtk4::Popover,
    gtk4::Entry,
    gtk4::Button,
    gtk4::Entry,
    gtk4::DropDown,
    gtk4::Button,
    gtk4::Button,
) {
    let pop = gtk4::Popover::new();
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);

    // Ratio row (applies directly).
    let ratio_caption = gtk4::Label::new(Some("Set directly from the drawing scale"));
    ratio_caption.add_css_class("dim-label");
    ratio_caption.set_xalign(0.0);
    let ratio_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    ratio_row.append(&gtk4::Label::new(Some("1 :")));
    let ratio_entry = gtk4::Entry::new();
    ratio_entry.set_placeholder_text(Some("50"));
    ratio_entry.set_max_width_chars(6);
    ratio_entry.update_property(&[gtk4::accessible::Property::Label("Scale ratio")]);
    ratio_row.append(&ratio_entry);
    let ratio_apply = gtk4::Button::with_label("Apply");
    // A button's child label wins name computation via the labelled-by
    // relation (ARIA precedence); clear it so the Label property applies.
    ratio_apply.update_relation(&[gtk4::accessible::Relation::LabelledBy(&[])]);
    ratio_apply.update_property(&[gtk4::accessible::Property::Label("Apply ratio")]);
    ratio_row.append(&ratio_apply);

    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);

    // Measured row (pick two points, then enter the real length).
    let meas_caption = gtk4::Label::new(Some("Or measure a known dimension"));
    meas_caption.add_css_class("dim-label");
    meas_caption.set_xalign(0.0);
    let pick_btn = gtk4::Button::with_label("Pick two points on the drawing");
    pick_btn.update_relation(&[gtk4::accessible::Relation::LabelledBy(&[])]);
    pick_btn.update_property(&[gtk4::accessible::Property::Label("Pick two points")]);
    let meas_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let len_entry = gtk4::Entry::new();
    len_entry.set_placeholder_text(Some("3000"));
    len_entry.set_max_width_chars(8);
    len_entry.update_property(&[gtk4::accessible::Property::Label("Scale length")]);
    let unit_dd = gtk4::DropDown::from_strings(&["mm", "m", "ft", "in"]);
    meas_row.append(&len_entry);
    meas_row.append(&unit_dd);
    let len_apply = gtk4::Button::with_label("Apply");
    len_apply.update_relation(&[gtk4::accessible::Relation::LabelledBy(&[])]);
    len_apply.update_property(&[gtk4::accessible::Property::Label("Apply length")]);
    meas_row.append(&len_apply);

    vbox.append(&ratio_caption);
    vbox.append(&ratio_row);
    vbox.append(&sep);
    vbox.append(&meas_caption);
    vbox.append(&pick_btn);
    vbox.append(&meas_row);
    pop.set_child(Some(&vbox));
    (pop, ratio_entry, ratio_apply, len_entry, unit_dd, len_apply, pick_btn)
}

/// Target display width (px) for sidebar thumbnails.
const THUMB_W: i32 = 180;

/// A sidebar row: a framed page thumbnail above its page number. The returned
/// `Picture` starts empty and is filled once its render lands.
fn make_thumb_row(page: usize) -> (gtk4::ListBoxRow, gtk4::Picture) {
    let pic = gtk4::Picture::new();
    pic.set_size_request(THUMB_W, 224);
    pic.set_content_fit(gtk4::ContentFit::Contain);
    pic.add_css_class("card");
    let label = gtk4::Label::new(Some(&format!("{}", page + 1)));
    label.add_css_class("caption");
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    vbox.set_margin_top(6);
    vbox.set_margin_bottom(6);
    vbox.set_margin_start(6);
    vbox.set_margin_end(6);
    vbox.append(&pic);
    vbox.append(&label);
    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&vbox));
    row.update_property(&[gtk4::accessible::Property::Label(&format!("Page {}", page + 1))]);
    (row, pic)
}

/// The zoom popover: Fit Page / Fit Width / Actual Size, then fixed levels.
fn build_zoom_popover(canvas: &Rc<PdfCanvas>) -> gtk4::Popover {
    let pop = gtk4::Popover::new();
    let vb = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    vb.set_margin_top(6);
    vb.set_margin_bottom(6);
    vb.set_margin_start(6);
    vb.set_margin_end(6);

    let mk = |label: &str| {
        let b = gtk4::Button::with_label(label);
        b.add_css_class("flat");
        if let Some(c) = b.child().and_downcast::<gtk4::Label>() {
            c.set_xalign(0.0);
        }
        b
    };

    let fit_page = mk("Fit Page");
    let fit_width = mk("Fit Width");
    let actual = mk("Actual Size (100%)");
    {
        let cb = canvas.clone();
        let p = pop.clone();
        fit_page.connect_clicked(move |_| {
            cb.fit_page();
            p.popdown();
        });
    }
    {
        let cb = canvas.clone();
        let p = pop.clone();
        fit_width.connect_clicked(move |_| {
            cb.fit_width();
            p.popdown();
        });
    }
    {
        let cb = canvas.clone();
        let p = pop.clone();
        actual.connect_clicked(move |_| {
            cb.zoom_actual();
            p.popdown();
        });
    }
    vb.append(&fit_page);
    vb.append(&fit_width);
    vb.append(&actual);
    vb.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    for pct in [50.0_f64, 75.0, 100.0, 150.0, 200.0] {
        let b = mk(&format!("{pct:.0}%"));
        let cb = canvas.clone();
        let p = pop.clone();
        b.connect_clicked(move |_| {
            cb.set_zoom_percent_centered(pct);
            p.popdown();
        });
        vb.append(&b);
    }

    pop.set_child(Some(&vb));
    pop
}

/// Delete the hovered dimension (if any) and offer undo via a toast.
fn delete_hovered(cb: &Rc<PdfCanvas>, toasts: &adw::ToastOverlay) {
    let (seg, len) = {
        let mut st = cb.state.borrow_mut();
        let Some(idx) = st.hovered_dimension() else { return };
        let Some(seg) = st.delete_dimension(idx) else { return };
        let len = st.format_len(seg.0.distance(&seg.1));
        (seg, len)
    };
    tracing::info!("dimension deleted: {len}");
    let toast = adw::Toast::builder()
        .title(format!("Deleted {len}"))
        .button_label("Undo")
        .build();
    let cb2 = cb.clone();
    let len2 = len.clone();
    toast.connect_button_clicked(move |_| {
        cb2.state.borrow_mut().restore_dimension(seg);
        tracing::info!("dimension restored: {len2}");
        cb2.area.queue_draw();
    });
    toasts.add_toast(toast);
    cb.area.queue_draw();
}
