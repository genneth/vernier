// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::{PagePt, Polyline};
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

pub struct SnapIndex {
    tree: RTree<[f64; 2]>,
}

impl SnapIndex {
    pub fn build(polylines: &[Polyline]) -> SnapIndex {
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for pl in polylines {
            for v in pl.vertices() {
                pts.push([v.x, v.y]);
            }
        }
        SnapIndex {
            tree: RTree::bulk_load(pts),
        }
    }

    pub fn nearest_vertex(&self, query: PagePt, radius: f64) -> Option<Snap> {
        let q = [query.x, query.y];
        let nearest = self.tree.nearest_neighbor(&q)?;
        let dx = nearest[0] - query.x;
        let dy = nearest[1] - query.y;
        if (dx * dx + dy * dy).sqrt() <= radius {
            Some(Snap {
                point: PagePt {
                    x: nearest[0],
                    y: nearest[1],
                },
                kind: SnapKind::Vertex,
            })
        } else {
            None
        }
    }

    /// Nearest vertex regardless of radius, with its distance — for diagnostics.
    pub fn nearest(&self, query: PagePt) -> Option<(PagePt, f64)> {
        let q = [query.x, query.y];
        self.tree.nearest_neighbor(&q).map(|p| {
            let dx = p[0] - query.x;
            let dy = p[1] - query.y;
            (PagePt { x: p[0], y: p[1] }, (dx * dx + dy * dy).sqrt())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .nearest_vertex(PagePt { x: 98.0, y: 1.0 }, 5.0)
            .unwrap();
        assert_eq!(s.point, PagePt { x: 100.0, y: 0.0 });
    }

    #[test]
    fn no_snap_when_out_of_radius() {
        assert!(idx()
            .nearest_vertex(PagePt { x: 50.0, y: 50.0 }, 5.0)
            .is_none());
    }
}
