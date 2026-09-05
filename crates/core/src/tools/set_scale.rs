// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::PagePt;
use crate::scale::{RealLen, Scale};
use crate::tools::segment::SegmentBuilder;

/// Picks two points whose real-world length the user then types, to derive the
/// scale. Built on the shared `SegmentBuilder` (same two-click behaviour as
/// measuring), keeping the completed pair until calibration is applied.
#[derive(Default)]
pub struct SetScaleTool {
    builder: SegmentBuilder,
    pair: Option<(PagePt, PagePt)>,
}

impl SetScaleTool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_point(&mut self, p: PagePt) {
        if let Some(seg) = self.builder.click(p) {
            self.pair = Some(seg);
        }
    }

    /// In-progress first point (for live preview).
    pub fn pending(&self) -> Option<PagePt> {
        self.builder.pending()
    }

    /// The completed calibration pair, once both points are placed.
    pub fn pair(&self) -> Option<(PagePt, PagePt)> {
        self.pair
    }

    pub fn is_ready(&self) -> bool {
        self.pair.is_some()
    }

    /// The scale implied by the pair and the typed real length. `None` until a
    /// pair is placed, or if the pair is degenerate (zero page length).
    pub fn finish(&self, real: RealLen) -> Option<Scale> {
        let (a, b) = self.pair?;
        Scale::from_measurement(a.distance(&b), real)
    }

    pub fn clear(&mut self) {
        self.builder.reset();
        self.pair = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::PageLen;
    use crate::scale::Unit;
    fn p(x: f64, y: f64) -> PagePt {
        PagePt { x, y }
    }

    #[test]
    fn computes_scale_from_two_points() {
        let mut t = SetScaleTool::new();
        t.add_point(p(0.0, 0.0));
        assert!(!t.is_ready());
        t.add_point(p(100.0, 0.0));
        assert!(t.is_ready());
        let s = t.finish(RealLen::new(3000.0, Unit::Mm)).unwrap();
        assert_eq!(s.apply(PageLen(100.0)).value, 3000.0);
    }

    #[test]
    fn not_ready_with_one_point() {
        let mut t = SetScaleTool::new();
        t.add_point(p(0.0, 0.0));
        assert!(!t.is_ready());
        assert!(t.finish(RealLen::new(1.0, Unit::Mm)).is_none());
    }

    #[test]
    fn coincident_points_give_no_scale() {
        let mut t = SetScaleTool::new();
        t.add_point(p(4.0, 4.0));
        t.add_point(p(4.0, 4.0));
        assert!(t.is_ready());
        assert!(t.finish(RealLen::new(1.0, Unit::Mm)).is_none());
    }
}
