// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::PagePt;
use crate::tools::segment::SegmentBuilder;

/// Two-point dimensions placed on the page. Built on the shared `SegmentBuilder`
/// so it behaves identically to scale calibration: click once to start, click
/// again to commit a dimension; repeat to place more.
#[derive(Default)]
pub struct Dimensions {
    committed: Vec<(PagePt, PagePt)>,
    builder: SegmentBuilder,
}

impl Dimensions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a point. Returns `true` if this click completed (committed) a dimension.
    pub fn add_point(&mut self, p: PagePt) -> bool {
        if let Some(seg) = self.builder.click(p) {
            self.committed.push(seg);
            true
        } else {
            false
        }
    }

    pub fn pending(&self) -> Option<PagePt> {
        self.builder.pending()
    }

    pub fn committed(&self) -> &[(PagePt, PagePt)] {
        &self.committed
    }

    /// Cancel the in-progress dimension (keeps committed ones). Returns whether
    /// there was a pending point.
    pub fn cancel_pending(&mut self) -> bool {
        let had = self.builder.pending().is_some();
        self.builder.reset();
        had
    }

    pub fn clear(&mut self) {
        self.committed.clear();
        self.builder.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f64, y: f64) -> PagePt {
        PagePt { x, y }
    }

    #[test]
    fn two_clicks_commit_one_dimension() {
        let mut d = Dimensions::new();
        assert!(!d.add_point(p(0.0, 0.0)));
        assert_eq!(d.pending(), Some(p(0.0, 0.0)));
        assert!(d.add_point(p(3.0, 4.0)));
        assert_eq!(d.pending(), None);
        assert_eq!(d.committed(), &[(p(0.0, 0.0), p(3.0, 4.0))]);
    }

    #[test]
    fn cancel_drops_pending_but_keeps_committed() {
        let mut d = Dimensions::new();
        d.add_point(p(0.0, 0.0));
        d.add_point(p(1.0, 0.0));
        d.add_point(p(5.0, 5.0));
        assert!(d.cancel_pending());
        assert_eq!(d.pending(), None);
        assert_eq!(d.committed().len(), 1);
        assert!(!d.cancel_pending());
    }
}
