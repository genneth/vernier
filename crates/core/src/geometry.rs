// SPDX-License-Identifier: AGPL-3.0-or-later
//! Domain geometry and the coordinate-space newtypes.
//!
//! Two spaces, kept apart by the type system: **page** (PDF points, top-left
//! origin, y-down; everything is stored here) and **screen** (widget pixels).
//! Lengths and rectangles carry their space too, so a page-space radius can
//! never be compared with a screen-space distance by accident.
use kurbo::{CubicBez, Point as KPoint};

/// A point in PDF page space: points (1/72"), top-left origin, y-down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PagePt {
    pub x: f64,
    pub y: f64,
}

/// A length in page space (PDF points).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct PageLen(pub f64);

/// An axis-aligned rectangle in page space, `x0 <= x1`, `y0 <= y1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// A page's size in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub w: f64,
    pub h: f64,
}

/// A point in widget/screen space: logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPt {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned rectangle in screen space, by origin and size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl PagePt {
    pub fn distance(&self, other: &PagePt) -> PageLen {
        PageLen(((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt())
    }
}

impl PageRect {
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }
    /// Clip to the page `[0, size]`.
    pub fn clamp_to(&self, size: PageSize) -> PageRect {
        PageRect {
            x0: self.x0.max(0.0),
            y0: self.y0.max(0.0),
            x1: self.x1.min(size.w),
            y1: self.y1.min(size.h),
        }
    }
}

impl ScreenPt {
    pub fn distance(&self, other: &ScreenPt) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

impl ScreenRect {
    pub fn contains(&self, p: ScreenPt) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
    /// The same rectangle widened by `dx` on the right edge.
    pub fn grow_right(&self, dx: f64) -> ScreenRect {
        ScreenRect {
            w: self.w + dx,
            ..*self
        }
    }
}

/// Distance from `p` to the segment `a`–`b`, all in screen space.
pub fn dist_to_segment(p: ScreenPt, a: ScreenPt, b: ScreenPt) -> f64 {
    let (vx, vy) = (b.x - a.x, b.y - a.y);
    let (wx, wy) = (p.x - a.x, p.y - a.y);
    let len2 = vx * vx + vy * vy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        ((wx * vx + wy * vy) / len2).clamp(0.0, 1.0)
    };
    let (dx, dy) = (wx - t * vx, wy - t * vy);
    (dx * dx + dy * dy).sqrt()
}

/// A flattened sequence of page-space points (a subpath rendered as line segments).
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline(pub Vec<PagePt>);

impl Polyline {
    pub fn vertices(&self) -> &[PagePt] {
        &self.0
    }
}

/// Flatten one cubic bezier into line points using kurbo. Returns points AFTER p0
/// (the caller already holds p0), ending exactly at p3.
pub fn flatten_cubic(p0: PagePt, p1: PagePt, p2: PagePt, p3: PagePt, tol: f64) -> Vec<PagePt> {
    let bez = CubicBez::new(
        KPoint::new(p0.x, p0.y),
        KPoint::new(p1.x, p1.y),
        KPoint::new(p2.x, p2.y),
        KPoint::new(p3.x, p3.y),
    );
    let mut out = Vec::new();
    kurbo::flatten(
        [
            kurbo::PathEl::MoveTo(bez.p0),
            kurbo::PathEl::CurveTo(bez.p1, bez.p2, bez.p3),
        ],
        tol,
        |el| {
            if let kurbo::PathEl::LineTo(pt) = el {
                out.push(PagePt { x: pt.x, y: pt.y });
            }
        },
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn distance_is_euclidean() {
        let a = PagePt { x: 0.0, y: 0.0 };
        let b = PagePt { x: 3.0, y: 4.0 };
        assert_eq!(a.distance(&b), PageLen(5.0));
    }

    #[test]
    fn flatten_straight_cubic_hits_endpoint() {
        let pts = flatten_cubic(
            PagePt { x: 0.0, y: 0.0 },
            PagePt { x: 1.0, y: 0.0 },
            PagePt { x: 2.0, y: 0.0 },
            PagePt { x: 3.0, y: 0.0 },
            0.1,
        );
        let last = pts.last().unwrap();
        assert!((last.x - 3.0).abs() < 1e-6 && last.y.abs() < 1e-6);
    }

    #[test]
    fn screen_rect_contains_is_inclusive() {
        let r = ScreenRect {
            x: 10.0,
            y: 10.0,
            w: 20.0,
            h: 5.0,
        };
        assert!(r.contains(ScreenPt { x: 10.0, y: 10.0 }));
        assert!(r.contains(ScreenPt { x: 30.0, y: 15.0 }));
        assert!(!r.contains(ScreenPt { x: 30.1, y: 15.0 }));
        assert!(r.grow_right(5.0).contains(ScreenPt { x: 34.0, y: 12.0 }));
    }

    fn coord() -> impl Strategy<Value = f64> {
        -1e4..1e4
    }

    proptest! {
        /// Flattening any cubic ends exactly at its end point.
        #[test]
        fn flatten_ends_at_p3(
            (x0, y0, x1, y1, x2, y2, x3, y3) in (coord(), coord(), coord(), coord(), coord(), coord(), coord(), coord())
        ) {
            let pts = flatten_cubic(
                PagePt { x: x0, y: y0 },
                PagePt { x: x1, y: y1 },
                PagePt { x: x2, y: y2 },
                PagePt { x: x3, y: y3 },
                0.3,
            );
            let last = pts.last().expect("at least one point");
            prop_assert!((last.x - x3).abs() < 1e-6 && (last.y - y3).abs() < 1e-6);
        }

        /// Segment distance is never below the distance to the nearest
        /// endpoint's foot and never above the distance to either endpoint.
        #[test]
        fn segment_distance_is_bounded_by_endpoints(
            (px, py, ax, ay, bx, by) in (coord(), coord(), coord(), coord(), coord(), coord())
        ) {
            let p = ScreenPt { x: px, y: py };
            let a = ScreenPt { x: ax, y: ay };
            let b = ScreenPt { x: bx, y: by };
            let d = dist_to_segment(p, a, b);
            prop_assert!(d <= p.distance(&a) + 1e-9);
            prop_assert!(d <= p.distance(&b) + 1e-9);
            prop_assert!(d >= 0.0);
        }
    }
}
