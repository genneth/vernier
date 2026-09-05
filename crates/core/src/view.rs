// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::{PageLen, PagePt, PageRect, PageSize, ScreenPt};

/// Zoom bounds shared by every zoom entry point (fit modes, wheel, presets).
pub const ZOOM_MIN: f64 = 0.05;
pub const ZOOM_MAX: f64 = 40.0;

/// Maps page space <-> screen space. `screen = page * zoom + pan`.
/// `pan` is the screen-space position of page origin (0,0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub zoom: f64,
    pub pan: ScreenPt,
}

impl Default for View {
    fn default() -> Self {
        View {
            zoom: 1.0,
            pan: ScreenPt { x: 0.0, y: 0.0 },
        }
    }
}

impl View {
    pub fn page_to_screen(&self, p: PagePt) -> ScreenPt {
        ScreenPt {
            x: p.x * self.zoom + self.pan.x,
            y: p.y * self.zoom + self.pan.y,
        }
    }

    pub fn screen_to_page(&self, s: ScreenPt) -> PagePt {
        PagePt {
            x: (s.x - self.pan.x) / self.zoom,
            y: (s.y - self.pan.y) / self.zoom,
        }
    }

    /// A screen-pixel radius expressed in page units at the current zoom.
    pub fn snap_radius_page(&self, screen_px: f64) -> PageLen {
        PageLen(screen_px / self.zoom)
    }

    /// The page-space rectangle visible in a viewport of `w`×`h` pixels.
    pub fn visible_page_rect(&self, w: f64, h: f64) -> PageRect {
        let tl = self.screen_to_page(ScreenPt { x: 0.0, y: 0.0 });
        let br = self.screen_to_page(ScreenPt { x: w, y: h });
        PageRect {
            x0: tl.x,
            y0: tl.y,
            x1: br.x,
            y1: br.y,
        }
    }

    /// Fit the whole page into a `w`×`h` viewport, centred, with a little margin.
    pub fn fit_page(size: PageSize, w: f64, h: f64) -> View {
        let zoom = ((w / size.w).min(h / size.h) * 0.97).clamp(ZOOM_MIN, ZOOM_MAX);
        View {
            zoom,
            pan: ScreenPt {
                x: (w - size.w * zoom) / 2.0,
                y: (h - size.h * zoom) / 2.0,
            },
        }
    }

    /// The zoom that fits the page width into a viewport `w` pixels wide.
    pub fn fit_width_zoom(size: PageSize, w: f64) -> f64 {
        ((w / size.w) * 0.99).clamp(ZOOM_MIN, ZOOM_MAX)
    }

    /// The view at `zoom` (clamped) that keeps the page point under `anchor`
    /// stationary on screen.
    pub fn zoomed_about(&self, zoom: f64, anchor: ScreenPt) -> View {
        let new_zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let k = new_zoom / self.zoom;
        View {
            zoom: new_zoom,
            pan: ScreenPt {
                x: anchor.x - k * (anchor.x - self.pan.x),
                y: anchor.y - k * (anchor.y - self.pan.y),
            },
        }
    }

    pub fn panned_by(&self, dx: f64, dy: f64) -> View {
        View {
            zoom: self.zoom,
            pan: ScreenPt {
                x: self.pan.x + dx,
                y: self.pan.y + dy,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn snap_radius_shrinks_with_zoom() {
        let v = View {
            zoom: 4.0,
            pan: ScreenPt { x: 0.0, y: 0.0 },
        };
        assert_eq!(v.snap_radius_page(12.0), PageLen(3.0));
    }

    #[test]
    fn fit_page_centres_the_page() {
        let v = View::fit_page(PageSize { w: 100.0, h: 200.0 }, 1000.0, 1000.0);
        let c = v.page_to_screen(PagePt { x: 50.0, y: 100.0 });
        assert!((c.x - 500.0).abs() < 1e-9 && (c.y - 500.0).abs() < 1e-9);
    }

    fn view() -> impl Strategy<Value = View> {
        (ZOOM_MIN..ZOOM_MAX, -1e4..1e4, -1e4..1e4).prop_map(|(zoom, x, y)| View {
            zoom,
            pan: ScreenPt { x, y },
        })
    }

    proptest! {
        #[test]
        fn round_trips_through_both_spaces(v in view(), x in -1e4..1e4, y in -1e4..1e4) {
            let p = PagePt { x, y };
            let back = v.screen_to_page(v.page_to_screen(p));
            prop_assert!((back.x - p.x).abs() < 1e-6 && (back.y - p.y).abs() < 1e-6);
        }

        #[test]
        fn zooming_about_an_anchor_keeps_it_fixed(
            v in view(), zoom in ZOOM_MIN..ZOOM_MAX, ax in 0.0..2000.0, ay in 0.0..2000.0
        ) {
            let anchor = ScreenPt { x: ax, y: ay };
            let before = v.screen_to_page(anchor);
            let after = v.zoomed_about(zoom, anchor).screen_to_page(anchor);
            prop_assert!((before.x - after.x).abs() < 1e-6 && (before.y - after.y).abs() < 1e-6);
        }

        #[test]
        fn fit_page_keeps_the_whole_page_visible(
            pw in 10.0..5000.0, ph in 10.0..5000.0, w in 100.0..4000.0, h in 100.0..4000.0
        ) {
            let v = View::fit_page(PageSize { w: pw, h: ph }, w, h);
            let tl = v.page_to_screen(PagePt { x: 0.0, y: 0.0 });
            let br = v.page_to_screen(PagePt { x: pw, y: ph });
            // Either fully inside, or the zoom clamp made that impossible.
            let clamped = v.zoom == ZOOM_MIN || v.zoom == ZOOM_MAX;
            prop_assert!(clamped || (tl.x >= -1e-6 && tl.y >= -1e-6 && br.x <= w + 1e-6 && br.y <= h + 1e-6));
        }
    }
}
