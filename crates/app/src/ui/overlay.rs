// SPDX-License-Identifier: AGPL-3.0-or-later
//! The measurement / calibration overlay, drawn with Cairo in screen space on
//! top of the page texture. Pure drawing: it reads `AppState` and reports the
//! screen rects it painted (label chips, delete badge) for hit-testing.
use gtk4::pango;
use gtk4::prelude::*;
use vernier_core::app::{AppState, Tool};
use vernier_core::geometry::{ScreenPt, ScreenRect};
use vernier_core::tools::dimension::DimId;
use vernier_core::view::View;

// Colourblind-safe overlay palette (IBM), distinguished also by shape/style.
const COL_DIM: (f64, f64, f64) = (0.392, 0.561, 1.0); // #648FFF — measurements
const COL_DIM_HOT: (f64, f64, f64) = (0.55, 0.69, 1.0); // hovered measurement
const COL_SCALE: (f64, f64, f64) = (0.863, 0.149, 0.498); // #DC267F — scale calibration
const COL_SNAP: (f64, f64, f64) = (0.996, 0.380, 0.0); // #FE6100 — snap marker

/// What the overlay painted that the pointer can hit.
pub struct Painted {
    /// Every committed dimension's label chip.
    pub label_rects: Vec<(DimId, ScreenRect)>,
    /// The × delete badge beside the hovered dimension's label, if any.
    pub close_rect: Option<ScreenRect>,
}

pub fn draw(cr: &gtk4::cairo::Context, widget: &gtk4::Widget, app: &AppState) -> Painted {
    let v = app.view();
    let dims = app.dimensions();
    let hovered = app.hovered_dimension();
    let mut painted = Painted {
        label_rects: Vec::with_capacity(dims.committed().len()),
        close_rect: None,
    };
    for d in dims.committed() {
        let sa = v.page_to_screen(d.a);
        let sb = v.page_to_screen(d.b);
        let hot = hovered == Some(d.id);
        let col = if hot { COL_DIM_HOT } else { COL_DIM };
        halo_line(cr, sa, sb, col, if hot { 4.0 } else { 2.5 }, false);
        dot(cr, sa, col);
        dot(cr, sb, col);
        // The chip is identical hovered or not, so nothing shifts; the delete
        // badge appears OUTSIDE it, in the empty space off its right edge.
        let chip = pill(
            widget,
            cr,
            ScreenPt {
                x: (sa.x + sb.x) / 2.0,
                y: (sa.y + sb.y) / 2.0,
            },
            &app.format_len(d.length()),
        );
        painted.label_rects.push((d.id, chip));
        if hot {
            painted.close_rect = Some(close_badge(
                cr,
                ScreenPt {
                    x: chip.x + chip.w + 12.0,
                    y: chip.y + chip.h / 2.0,
                },
            ));
        }
    }

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
                    ScreenPt {
                        x: sc.x + 16.0,
                        y: sc.y - 16.0,
                    },
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
        snap_marker(cr, &v, c);
    }
    painted
}

fn snap_marker(cr: &gtk4::cairo::Context, v: &View, c: vernier_core::geometry::PagePt) {
    let s = v.page_to_screen(c);
    cr.rectangle(s.x - 5.0, s.y - 5.0, 10.0, 10.0);
    cr.set_line_width(4.0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    let _ = cr.stroke_preserve();
    cr.set_line_width(2.0);
    cr.set_source_rgb(COL_SNAP.0, COL_SNAP.1, COL_SNAP.2);
    let _ = cr.stroke();
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

/// A small round × delete badge centred at `c`. Returns its hit rect, padded
/// for clickability.
fn close_badge(cr: &gtk4::cairo::Context, c: ScreenPt) -> ScreenRect {
    let r = 9.0;
    cr.arc(c.x, c.y, r, 0.0, std::f64::consts::TAU);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.82);
    let _ = cr.fill();
    cr.arc(c.x, c.y, r, 0.0, std::f64::consts::TAU);
    cr.set_line_width(1.5);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
    let _ = cr.stroke();
    let k = 3.5;
    cr.set_line_width(2.0);
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(c.x - k, c.y - k);
    cr.line_to(c.x + k, c.y + k);
    cr.move_to(c.x + k, c.y - k);
    cr.line_to(c.x - k, c.y + k);
    let _ = cr.stroke();
    ScreenRect {
        x: c.x - 11.0,
        y: c.y - 11.0,
        w: 22.0,
        h: 22.0,
    }
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

/// A rounded dark pill with white text, centred at `c`. Uses the widget's own
/// font via Pango — system family + size, honouring text-scaling — in bold.
/// Returns the chip's screen rect.
fn pill(widget: &gtk4::Widget, cr: &gtk4::cairo::Context, c: ScreenPt, text: &str) -> ScreenRect {
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
    let x = c.x - w / 2.0;
    let y = c.y - h / 2.0;
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
    cr.move_to(c.x - tw / 2.0, c.y - th / 2.0);
    pangocairo::functions::show_layout(cr, &layout);
    ScreenRect { x, y, w, h }
}
