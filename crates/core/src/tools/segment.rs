// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::PagePt;

/// Builds a line segment from two clicks. The single source of truth for
/// "click two points to define a segment", shared by every two-point tool
/// (measurement dimensions, scale calibration) so they behave identically:
/// the first click starts a fresh segment, the second completes it.
#[derive(Default)]
pub struct SegmentBuilder {
    start: Option<PagePt>,
}

impl SegmentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a click. Returns the completed `(start, end)` segment on the
    /// second click (resetting, ready for a fresh segment); `None` on the first.
    pub fn click(&mut self, p: PagePt) -> Option<(PagePt, PagePt)> {
        match self.start.take() {
            None => {
                self.start = Some(p);
                None
            }
            Some(a) => Some((a, p)),
        }
    }

    /// The placed-but-not-yet-completed first point, if any (for live preview).
    pub fn pending(&self) -> Option<PagePt> {
        self.start
    }

    pub fn reset(&mut self) {
        self.start = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f64, y: f64) -> PagePt {
        PagePt { x, y }
    }

    #[test]
    fn two_clicks_make_a_segment() {
        let mut s = SegmentBuilder::new();
        assert_eq!(s.click(p(0.0, 0.0)), None);
        assert_eq!(s.pending(), Some(p(0.0, 0.0)));
        assert_eq!(s.click(p(1.0, 1.0)), Some((p(0.0, 0.0), p(1.0, 1.0))));
        assert_eq!(s.pending(), None); // resets after completing
    }

    #[test]
    fn next_segment_starts_fresh_not_from_last_end() {
        // The bug guard: after a completed segment, the next first click must
        // start a NEW segment, not connect to the previous endpoint.
        let mut s = SegmentBuilder::new();
        s.click(p(0.0, 0.0));
        s.click(p(1.0, 1.0)); // completes (0,0)->(1,1)
        assert_eq!(s.click(p(5.0, 5.0)), None); // fresh start, no segment yet
        assert_eq!(s.pending(), Some(p(5.0, 5.0)));
    }

    #[test]
    fn reset_clears_pending() {
        let mut s = SegmentBuilder::new();
        s.click(p(2.0, 2.0));
        s.reset();
        assert_eq!(s.pending(), None);
    }
}
