// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::{PageLen, PagePt, Polyline};
use rstar::RTree;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SnapKind {
    Vertex,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    pub point: PagePt,
    pub kind: SnapKind,
}

/// Spatial index over every polyline vertex on the page.
pub struct SnapIndex {
    tree: RTree<[f64; 2]>,
}

impl SnapIndex {
    pub fn build(polylines: &[Polyline]) -> SnapIndex {
        let pts: Vec<[f64; 2]> = polylines
            .iter()
            .flat_map(|pl| pl.vertices().iter().map(|v| [v.x, v.y]))
            .collect();
        SnapIndex {
            tree: RTree::bulk_load(pts),
        }
    }

    pub fn len(&self) -> usize {
        self.tree.size()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }

    /// The nearest vertex to `query`, if one lies within `radius`.
    pub fn nearest_vertex(&self, query: PagePt, radius: PageLen) -> Option<Snap> {
        let (point, d) = self.nearest(query)?;
        (d <= radius).then_some(Snap {
            point,
            kind: SnapKind::Vertex,
        })
    }

    /// Nearest vertex regardless of radius, with its distance.
    pub fn nearest(&self, query: PagePt) -> Option<(PagePt, PageLen)> {
        let p = self.tree.nearest_neighbor(&[query.x, query.y])?;
        let pt = PagePt { x: p[0], y: p[1] };
        Some((pt, pt.distance(&query)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn idx() -> SnapIndex {
        SnapIndex::build(&[Polyline(vec![
            PagePt { x: 0.0, y: 0.0 },
            PagePt { x: 100.0, y: 0.0 },
            PagePt { x: 100.0, y: 50.0 },
        ])])
    }

    #[test]
    fn snaps_to_nearby_vertex() {
        let s = idx()
            .nearest_vertex(PagePt { x: 98.0, y: 1.0 }, PageLen(5.0))
            .unwrap();
        assert_eq!(s.point, PagePt { x: 100.0, y: 0.0 });
    }

    #[test]
    fn no_snap_when_out_of_radius() {
        assert!(idx()
            .nearest_vertex(PagePt { x: 50.0, y: 50.0 }, PageLen(5.0))
            .is_none());
    }

    #[test]
    fn empty_index_never_snaps() {
        let e = SnapIndex::build(&[]);
        assert!(e.is_empty());
        assert!(e.nearest(PagePt { x: 0.0, y: 0.0 }).is_none());
    }

    fn pt() -> impl Strategy<Value = PagePt> {
        (-1e3..1e3, -1e3..1e3).prop_map(|(x, y)| PagePt { x, y })
    }

    proptest! {
        /// The R-tree answer equals the brute-force nearest vertex (reference
        /// implementation), and the radius test is exact.
        #[test]
        fn nearest_matches_brute_force(
            pts in prop::collection::vec(pt(), 1..200),
            q in pt(),
            radius in 0.0..500.0,
        ) {
            let index = SnapIndex::build(&[Polyline(pts.clone())]);
            let brute = pts
                .iter()
                .map(|p| p.distance(&q))
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap();
            let (_, d) = index.nearest(q).unwrap();
            prop_assert!((d.0 - brute.0).abs() < 1e-9);
            prop_assert_eq!(index.nearest_vertex(q, PageLen(radius)).is_some(), brute <= PageLen(radius));
        }
    }
}
