// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::{PageLen, PagePt};
use crate::tools::segment::SegmentBuilder;

/// Stable identity of a placed dimension. Ids are never reused within a
/// `Dimensions`, so a stale id (from hover, or a label rect measured last
/// frame) can only fail to resolve, never resolve to the wrong dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DimId(u64);

/// A committed two-point dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dimension {
    pub id: DimId,
    pub a: PagePt,
    pub b: PagePt,
}

impl Dimension {
    pub fn length(&self) -> PageLen {
        self.a.distance(&self.b)
    }
}

/// Two-point dimensions placed on the page. Built on the shared `SegmentBuilder`
/// so it behaves identically to scale calibration: click once to start, click
/// again to commit a dimension; repeat to place more.
#[derive(Default)]
pub struct Dimensions {
    committed: Vec<Dimension>,
    builder: SegmentBuilder,
    next_id: u64,
}

impl Dimensions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a point. Returns the dimension this click completed, if any.
    /// A second click on the same snapped point is ignored (no zero-length
    /// dimensions) — the first point stays pending.
    pub fn add_point(&mut self, p: PagePt) -> Option<Dimension> {
        let (a, b) = self.builder.click(p)?;
        if a == b {
            self.builder.click(a); // re-arm the pending point
            return None;
        }
        let dim = Dimension {
            id: DimId(self.next_id),
            a,
            b,
        };
        self.next_id += 1;
        self.committed.push(dim);
        Some(dim)
    }

    pub fn pending(&self) -> Option<PagePt> {
        self.builder.pending()
    }

    pub fn committed(&self) -> &[Dimension] {
        &self.committed
    }

    pub fn get(&self, id: DimId) -> Option<&Dimension> {
        self.committed.iter().find(|d| d.id == id)
    }

    /// Remove a committed dimension (returned for undo).
    pub fn remove(&mut self, id: DimId) -> Option<Dimension> {
        let i = self.committed.iter().position(|d| d.id == id)?;
        Some(self.committed.remove(i))
    }

    /// Reinsert a dimension removed earlier (undo). It keeps its id, so it can
    /// only be restored once; restoring one that is already present is a no-op.
    pub fn restore(&mut self, dim: Dimension) {
        if self.get(dim.id).is_none() {
            self.committed.push(dim);
        }
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
    fn ends(d: &Dimension) -> (PagePt, PagePt) {
        (d.a, d.b)
    }

    #[test]
    fn two_clicks_commit_one_dimension() {
        let mut d = Dimensions::new();
        assert!(d.add_point(p(0.0, 0.0)).is_none());
        assert_eq!(d.pending(), Some(p(0.0, 0.0)));
        let dim = d.add_point(p(3.0, 4.0)).unwrap();
        assert_eq!(ends(&dim), (p(0.0, 0.0), p(3.0, 4.0)));
        assert_eq!(dim.length(), PageLen(5.0));
        assert_eq!(d.pending(), None);
        assert_eq!(d.committed(), &[dim]);
    }

    #[test]
    fn same_point_twice_commits_nothing_and_stays_pending() {
        let mut d = Dimensions::new();
        assert!(d.add_point(p(2.0, 3.0)).is_none());
        assert!(d.add_point(p(2.0, 3.0)).is_none());
        assert!(d.committed().is_empty());
        assert_eq!(d.pending(), Some(p(2.0, 3.0)));
        let dim = d.add_point(p(5.0, 3.0)).unwrap();
        assert_eq!(ends(&dim), (p(2.0, 3.0), p(5.0, 3.0)));
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

    #[test]
    fn ids_are_unique_and_survive_remove_restore() {
        let mut d = Dimensions::new();
        d.add_point(p(0.0, 0.0));
        let first = d.add_point(p(1.0, 0.0)).unwrap();
        d.add_point(p(0.0, 5.0));
        let second = d.add_point(p(1.0, 5.0)).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(d.remove(first.id), Some(first));
        assert_eq!(d.remove(first.id), None); // gone
        d.add_point(p(0.0, 9.0));
        let third = d.add_point(p(1.0, 9.0)).unwrap();
        assert_ne!(third.id, first.id); // ids are never reused
        d.restore(first);
        d.restore(first); // idempotent
        assert_eq!(d.committed().len(), 3);
        assert_eq!(d.get(first.id), Some(&first));
    }
}
