// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::{PagePt, ScreenPt};

/// Maps page space <-> screen space. screen = page * zoom + pan.
/// `pan` is the screen-space position of page origin (0,0).
#[derive(Debug, Clone, Copy)]
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
    pub fn snap_radius_page(&self, screen_px: f64) -> f64 {
        screen_px / self.zoom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_both_spaces() {
        let v = View {
            zoom: 2.0,
            pan: ScreenPt { x: 30.0, y: -10.0 },
        };
        let p = PagePt { x: 12.5, y: 7.0 };
        let back = v.screen_to_page(v.page_to_screen(p));
        assert!((back.x - p.x).abs() < 1e-9 && (back.y - p.y).abs() < 1e-9);
    }

    #[test]
    fn snap_radius_shrinks_with_zoom() {
        let v = View {
            zoom: 4.0,
            pan: ScreenPt { x: 0.0, y: 0.0 },
        };
        assert_eq!(v.snap_radius_page(12.0), 3.0);
    }
}
