// SPDX-License-Identifier: AGPL-3.0-or-later
use kurbo::{CubicBez, Point as KPoint};

/// A point in PDF page space: MuPDF points, top-left origin, y-down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PagePt {
    pub x: f64,
    pub y: f64,
}

/// A point in widget/screen space: pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPt {
    pub x: f64,
    pub y: f64,
}

impl PagePt {
    pub fn distance(&self, other: &PagePt) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
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

    #[test]
    fn distance_is_euclidean() {
        let a = PagePt { x: 0.0, y: 0.0 };
        let b = PagePt { x: 3.0, y: 4.0 };
        assert_eq!(a.distance(&b), 5.0);
    }

    #[test]
    fn flatten_straight_cubic_hits_endpoint() {
        // A degenerate cubic along a straight line: control points colinear.
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
    fn polyline_exposes_vertices() {
        let pl = Polyline(vec![
            PagePt { x: 0.0, y: 0.0 },
            PagePt { x: 1.0, y: 1.0 },
        ]);
        assert_eq!(pl.vertices().len(), 2);
    }
}
