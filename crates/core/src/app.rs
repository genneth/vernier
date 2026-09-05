// SPDX-License-Identifier: AGPL-3.0-or-later
//! The headless application state: view, scale, snap index, tools and placed
//! dimensions. Pointer events come in (screen space), snapped points and
//! overlay state go out. No GTK, no MuPDF, no effects.
use crate::geometry::{dist_to_segment, PageLen, PagePt, Polyline, ScreenPt, ScreenRect};
use crate::scale::{RealLen, Scale};
use crate::snap::SnapIndex;
use crate::tools::dimension::{DimId, Dimension, Dimensions};
use crate::tools::set_scale::SetScaleTool;
use crate::view::View;

/// Snap radius in screen px (constant on screen, so it shrinks in page space as you zoom in).
pub const SNAP_PX: f64 = 12.0;
/// Hover hit threshold for committed dimensions, in screen px.
pub const HIT_PX: f64 = 12.0;
/// Margin right of a dimension's label chip that still counts as hovering it —
/// the delete badge appears there, and hover must survive the trip to it.
pub const BADGE_ZONE_PX: f64 = 26.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Measure,
    SetScale,
}

pub struct AppState {
    view: View,
    scale: Option<Scale>,
    tool: Tool,
    /// Snapped position of the pointer (set on every move, in both tools) — drives
    /// the snap marker and the in-progress dimension preview.
    cursor: Option<PagePt>,
    dimensions: Dimensions,
    /// The committed dimension under the pointer, if any.
    hover: Option<DimId>,
    /// Screen-space label-chip rects, fed back by the draw pass (the core cannot
    /// measure text). Hovering the chip counts as hovering its dimension — vital
    /// when the line is vertical and the chip (and delete badge) sit beside it.
    /// Keyed by id, so a rect for a deleted dimension simply never matches.
    label_rects: Vec<(DimId, ScreenRect)>,
    set_scale: SetScaleTool,
    /// `None` until the page's geometry has arrived (or if it has none).
    index: Option<SnapIndex>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            view: View::default(),
            scale: None,
            tool: Tool::Measure,
            cursor: None,
            dimensions: Dimensions::new(),
            hover: None,
            label_rects: Vec::new(),
            set_scale: SetScaleTool::new(),
            index: None,
        }
    }

    // ---- read side -------------------------------------------------------

    pub fn view(&self) -> View {
        self.view
    }
    pub fn scale(&self) -> Option<Scale> {
        self.scale
    }
    pub fn active_tool(&self) -> Tool {
        self.tool
    }
    pub fn cursor(&self) -> Option<PagePt> {
        self.cursor
    }
    pub fn dimensions(&self) -> &Dimensions {
        &self.dimensions
    }
    pub fn set_scale_tool(&self) -> &SetScaleTool {
        &self.set_scale
    }
    pub fn hovered_dimension(&self) -> Option<DimId> {
        self.hover
    }
    /// Number of snappable vertices on the current page (0 until geometry lands).
    pub fn snap_vertex_count(&self) -> usize {
        self.index.as_ref().map_or(0, SnapIndex::len)
    }

    /// Format a page-space length in real units if a scale is set, else points.
    pub fn format_len(&self, len: PageLen) -> String {
        match self.scale {
            Some(s) => s.apply(len).format(),
            None => format!("{:.0} pt", len.0),
        }
    }

    // ---- document / view -------------------------------------------------

    /// Install the page's vector geometry (replacing any previous page's).
    pub fn set_geometry(&mut self, polylines: &[Polyline]) {
        let index = SnapIndex::build(polylines);
        tracing::debug!("snap geometry: {} vertices", index.len());
        self.index = Some(index);
    }

    /// Forget the current page's geometry (a new page is loading).
    pub fn clear_geometry(&mut self) {
        self.index = None;
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    // ---- tools -----------------------------------------------------------

    pub fn set_tool(&mut self, t: Tool) {
        self.tool = t;
    }

    /// Enter set-scale mode with a fresh pair of points, dropping any
    /// half-placed measurement so its preview doesn't bleed into this tool.
    pub fn begin_set_scale(&mut self) {
        self.set_scale.clear();
        self.dimensions.cancel_pending();
        self.tool = Tool::SetScale;
    }

    /// Set the scale directly (e.g. from a stated 1:N ratio) and return to measuring.
    pub fn set_scale(&mut self, s: Scale) {
        self.scale = Some(s);
        self.tool = Tool::Measure;
    }

    /// Apply the typed real length to the two picked calibration points.
    /// Returns whether a scale was set (false if the points aren't both placed,
    /// or coincide).
    pub fn finish_set_scale(&mut self, real: RealLen) -> bool {
        match self.set_scale.finish(real) {
            Some(s) => {
                self.scale = Some(s);
                self.set_scale.clear();
                self.tool = Tool::Measure;
                true
            }
            None => false,
        }
    }

    /// Cancel the in-progress action (Escape): the pending dimension, or the
    /// in-progress scale calibration.
    pub fn cancel(&mut self) {
        match self.tool {
            Tool::Measure => {
                self.dimensions.cancel_pending();
            }
            Tool::SetScale => self.set_scale.clear(),
        }
    }

    /// Drop all placed dimensions (e.g. when changing page).
    pub fn clear_measure(&mut self) {
        self.dimensions.clear();
        self.hover = None;
        self.label_rects.clear();
    }

    /// Delete a committed dimension; returns it so the caller can offer undo.
    pub fn delete_dimension(&mut self, id: DimId) -> Option<Dimension> {
        let dim = self.dimensions.remove(id)?;
        if self.hover == Some(id) {
            self.hover = None;
        }
        Some(dim)
    }

    pub fn restore_dimension(&mut self, dim: Dimension) {
        self.dimensions.restore(dim);
    }

    // ---- pointer ---------------------------------------------------------

    /// Map a screen point to page space, snapping to a vertex if one is in range.
    pub fn snap(&self, screen: ScreenPt) -> PagePt {
        let page = self.view.screen_to_page(screen);
        let radius = self.view.snap_radius_page(SNAP_PX);
        match self
            .index
            .as_ref()
            .and_then(|i| i.nearest_vertex(page, radius))
        {
            Some(snap) => snap.point,
            None => page,
        }
    }

    /// The pointer left the canvas: no snap marker, nothing hovered.
    pub fn on_pointer_leave(&mut self) {
        self.cursor = None;
        self.hover = None;
    }

    pub fn on_pointer_move(&mut self, screen: ScreenPt) {
        // Snap in both tools so the snap marker always shows on hover.
        self.cursor = Some(self.snap(screen));
        self.hover = self.hit_test_dimension(screen);
    }

    pub fn on_click(&mut self, screen: ScreenPt) {
        let p = self.snap(screen);
        match self.tool {
            Tool::Measure => {
                self.dimensions.add_point(p);
            }
            Tool::SetScale => self.set_scale.add_point(p),
        }
    }

    /// Replace the label-chip rects reported by the last draw pass.
    pub fn set_label_rects(&mut self, rects: Vec<(DimId, ScreenRect)>) {
        self.label_rects = rects;
    }

    /// The dimension whose label chip (plus badge margin) contains the cursor,
    /// else the nearest committed dimension within `HIT_PX` (screen space).
    fn hit_test_dimension(&self, screen: ScreenPt) -> Option<DimId> {
        if let Some((id, _)) = self.label_rects.iter().find(|(id, rect)| {
            self.dimensions.get(*id).is_some() && rect.grow_right(BADGE_ZONE_PX).contains(screen)
        }) {
            return Some(*id);
        }
        self.dimensions
            .committed()
            .iter()
            .map(|d| {
                let sa = self.view.page_to_screen(d.a);
                let sb = self.view.page_to_screen(d.b);
                (d.id, dist_to_segment(screen, sa, sb))
            })
            .filter(|(_, d)| *d <= HIT_PX)
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Unit;
    use proptest::prelude::*;

    fn sp(x: f64, y: f64) -> ScreenPt {
        ScreenPt { x, y }
    }

    fn state_with_vertex_at(p: PagePt) -> AppState {
        // Second vertex far away so `p` is unambiguously the nearest.
        let mut s = AppState::new();
        s.set_geometry(&[Polyline(vec![
            p,
            PagePt {
                x: p.x + 200.0,
                y: p.y + 200.0,
            },
        ])]);
        s
    }

    /// Place one dimension by two clicks (identity view: screen == page).
    fn place(s: &mut AppState, a: ScreenPt, b: ScreenPt) -> Dimension {
        s.on_click(a);
        s.on_click(b);
        *s.dimensions().committed().last().unwrap()
    }

    #[test]
    fn click_snaps_to_nearby_vertex() {
        // zoom 1.0, identity pan: screen == page. Vertex at (100,100); click at (104,100).
        let mut s = state_with_vertex_at(PagePt { x: 100.0, y: 100.0 });
        s.on_click(sp(104.0, 100.0));
        assert_eq!(
            s.dimensions().pending(),
            Some(PagePt { x: 100.0, y: 100.0 })
        );
    }

    #[test]
    fn no_geometry_means_no_snap() {
        let mut s = AppState::new();
        assert_eq!(s.snap_vertex_count(), 0);
        s.on_click(sp(104.0, 100.0));
        assert_eq!(
            s.dimensions().pending(),
            Some(PagePt { x: 104.0, y: 100.0 })
        );
    }

    #[test]
    fn set_scale_then_measure_reads_real_units() {
        let mut s = AppState::new();
        s.begin_set_scale();
        s.on_click(sp(0.0, 0.0));
        s.on_click(sp(100.0, 0.0));
        assert!(s.finish_set_scale(RealLen::new(3000.0, Unit::Mm)));
        assert_eq!(s.active_tool(), Tool::Measure);
        // Measure a 50-pt segment -> 1500 mm.
        let d = place(&mut s, sp(0.0, 0.0), sp(50.0, 0.0));
        assert_eq!(s.format_len(d.length()), "1500.0 mm");
    }

    #[test]
    fn finishing_scale_without_two_points_fails_and_stays_in_tool() {
        let mut s = AppState::new();
        s.begin_set_scale();
        s.on_click(sp(0.0, 0.0));
        assert!(!s.finish_set_scale(RealLen::new(3000.0, Unit::Mm)));
        assert_eq!(s.active_tool(), Tool::SetScale);
        assert!(s.scale().is_none());
    }

    #[test]
    fn unscaled_lengths_read_in_points() {
        let mut s = AppState::new();
        let d = place(&mut s, sp(0.0, 0.0), sp(50.0, 0.0));
        assert_eq!(s.format_len(d.length()), "50 pt");
    }

    #[test]
    fn pointer_leave_clears_cursor_and_hover() {
        let mut s = AppState::new();
        place(&mut s, sp(10.0, 10.0), sp(110.0, 10.0));
        s.on_pointer_move(sp(60.0, 12.0));
        assert!(s.cursor().is_some() && s.hovered_dimension().is_some());
        s.on_pointer_leave();
        assert_eq!(s.cursor(), None);
        assert_eq!(s.hovered_dimension(), None);
    }

    #[test]
    fn escape_cancels_pending_dimension() {
        let mut s = AppState::new();
        s.on_click(sp(10.0, 10.0)); // start
        assert!(s.dimensions().pending().is_some());
        s.cancel();
        assert!(s.dimensions().pending().is_none());
    }

    #[test]
    fn delete_and_restore_round_trip() {
        let mut s = AppState::new();
        let d = place(&mut s, sp(10.0, 10.0), sp(110.0, 10.0));
        s.on_pointer_move(sp(60.0, 12.0));
        let id = s.hovered_dimension().unwrap();
        assert_eq!(id, d.id);
        let gone = s.delete_dimension(id).unwrap();
        assert_eq!(gone, d);
        assert_eq!(s.dimensions().committed().len(), 0);
        assert_eq!(s.hovered_dimension(), None); // stale hover cleared
        assert!(s.delete_dimension(id).is_none()); // double delete is safe
        s.restore_dimension(gone);
        assert_eq!(s.dimensions().committed(), &[d]);
    }

    #[test]
    fn hover_hits_dimension_within_threshold() {
        let mut s = AppState::new();
        let d = place(&mut s, sp(10.0, 10.0), sp(110.0, 10.0));
        s.on_pointer_move(sp(60.0, 15.0)); // 5 px off the line
        assert_eq!(s.hovered_dimension(), Some(d.id));
        s.on_pointer_move(sp(60.0, 40.0)); // 30 px off
        assert_eq!(s.hovered_dimension(), None);
    }

    #[test]
    fn hover_prefers_nearest_of_overlapping_dimensions() {
        let mut s = AppState::new();
        let _first = place(&mut s, sp(0.0, 0.0), sp(100.0, 0.0));
        let second = place(&mut s, sp(0.0, 8.0), sp(100.0, 8.0));
        s.on_pointer_move(sp(50.0, 6.0)); // 6 px from #0, 2 px from #1
        assert_eq!(s.hovered_dimension(), Some(second.id));
    }

    #[test]
    fn hover_via_label_rect_works_for_vertical_dimension() {
        let mut s = AppState::new();
        let d = place(&mut s, sp(100.0, 100.0), sp(100.0, 300.0)); // vertical line
                                                                   // The label chip sits beside the line (as the draw pass would report).
        let chip = ScreenRect {
            x: 60.0,
            y: 186.0,
            w: 80.0,
            h: 28.0,
        };
        s.set_label_rects(vec![(d.id, chip)]);
        s.on_pointer_move(sp(70.0, 200.0)); // in chip, 30 px off line
        assert_eq!(s.hovered_dimension(), Some(d.id));
        // The margin right of the chip (where the delete badge appears) counts
        // too, so hover survives the trip from chip to badge.
        s.on_pointer_move(sp(160.0, 200.0));
        assert_eq!(s.hovered_dimension(), Some(d.id));
        s.on_pointer_move(sp(60.0, 260.0)); // outside chip and line
        assert_eq!(s.hovered_dimension(), None);
    }

    #[test]
    fn stale_label_rects_cannot_hover_a_deleted_dimension() {
        let mut s = AppState::new();
        let d = place(&mut s, sp(100.0, 100.0), sp(100.0, 300.0));
        let chip = ScreenRect {
            x: 60.0,
            y: 186.0,
            w: 80.0,
            h: 28.0,
        };
        s.set_label_rects(vec![(d.id, chip)]);
        s.on_pointer_move(sp(70.0, 200.0));
        s.delete_dimension(d.id).unwrap();
        // The draw pass has not run yet, so the rect is still registered.
        s.on_pointer_move(sp(70.0, 200.0));
        assert_eq!(s.hovered_dimension(), None);
    }

    #[test]
    fn hover_threshold_is_screen_space() {
        let mut s = AppState::new();
        let d = place(&mut s, sp(10.0, 10.0), sp(110.0, 10.0));
        // Zoom in 4x: the same page-space offset is now 4x bigger on screen.
        s.set_view(View {
            zoom: 4.0,
            pan: ScreenPt { x: 0.0, y: 0.0 },
        });
        // Page point (60, 12.5) -> screen (240, 50); the line is at screen y 40.
        s.on_pointer_move(sp(240.0, 50.0)); // 10 px off on screen
        assert_eq!(s.hovered_dimension(), Some(d.id));
        s.on_pointer_move(sp(240.0, 56.0)); // 16 px off on screen
        assert_eq!(s.hovered_dimension(), None);
    }

    fn pt() -> impl Strategy<Value = ScreenPt> {
        (0.0..1000.0, 0.0..1000.0).prop_map(|(x, y)| ScreenPt { x, y })
    }

    proptest! {
        /// Hover agrees with the brute-force definition: the hovered dimension
        /// (if any) is within `HIT_PX`, and nothing is hovered only when every
        /// dimension is further than `HIT_PX` (no label rects registered).
        #[test]
        fn hover_matches_brute_force(
            ends in prop::collection::vec((pt(), pt()), 0..8),
            q in pt(),
        ) {
            let mut s = AppState::new();
            for (a, b) in &ends {
                if a != b {
                    place(&mut s, *a, *b);
                }
            }
            s.on_pointer_move(q);
            let dists: Vec<(DimId, f64)> = s
                .dimensions()
                .committed()
                .iter()
                .map(|d| (d.id, dist_to_segment(q, sp(d.a.x, d.a.y), sp(d.b.x, d.b.y))))
                .collect();
            let nearest = dists.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1));
            match s.hovered_dimension() {
                Some(id) => {
                    let d = dists.iter().find(|(i, _)| *i == id).unwrap().1;
                    prop_assert!(d <= HIT_PX);
                    prop_assert!((d - nearest.unwrap().1).abs() < 1e-9);
                }
                None => prop_assert!(nearest.is_none_or(|(_, d)| d > HIT_PX)),
            }
        }

        /// Deleting then restoring any dimension leaves the committed set
        /// unchanged as a set.
        #[test]
        fn delete_restore_is_identity(
            ends in prop::collection::vec((pt(), pt()), 1..8),
            which in 0usize..8,
        ) {
            let mut s = AppState::new();
            for (a, b) in &ends {
                if a != b {
                    place(&mut s, *a, *b);
                }
            }
            let before: Vec<Dimension> = s.dimensions().committed().to_vec();
            if before.is_empty() {
                return Ok(());
            }
            let victim = before[which % before.len()];
            let gone = s.delete_dimension(victim.id).unwrap();
            prop_assert_eq!(s.dimensions().committed().len(), before.len() - 1);
            s.restore_dimension(gone);
            let mut after = s.dimensions().committed().to_vec();
            let mut expect = before.clone();
            after.sort_by_key(|d| d.id);
            expect.sort_by_key(|d| d.id);
            prop_assert_eq!(after, expect);
        }
    }
}
