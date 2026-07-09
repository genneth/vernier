// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::geometry::{PagePt, Polyline, ScreenPt};
use crate::scale::{Scale, Unit};
use crate::snap::SnapIndex;
use crate::tools::dimension::Dimensions;
use crate::tools::set_scale::SetScaleTool;
use crate::view::View;

const SNAP_PX: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq)]
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
    set_scale: SetScaleTool,
    index: Option<SnapIndex>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            view: View::default(),
            scale: None,
            tool: Tool::Measure,
            cursor: None,
            dimensions: Dimensions::new(),
            set_scale: SetScaleTool::new(),
            index: None,
        }
    }

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

    pub fn set_geometry(&mut self, polylines: Vec<Polyline>) {
        let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        let mut n = 0usize;
        for pl in &polylines {
            for v in pl.vertices() {
                minx = minx.min(v.x);
                miny = miny.min(v.y);
                maxx = maxx.max(v.x);
                maxy = maxy.max(v.y);
                n += 1;
            }
        }
        tracing::debug!("snap geometry: {n} vertices, bbox [{minx:.0},{miny:.0}]..[{maxx:.0},{maxy:.0}]");
        self.index = Some(SnapIndex::build(&polylines));
    }

    pub fn set_zoom(&mut self, zoom: f64) {
        self.view.zoom = zoom;
    }
    pub fn set_pan(&mut self, pan: ScreenPt) {
        self.view.pan = pan;
    }
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

    /// Drop all placed dimensions (e.g. when changing page).
    pub fn clear_measure(&mut self) {
        self.dimensions.clear();
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

    /// Map a screen point to page space, snapping to a vertex if one is in range.
    pub fn snap(&self, screen: ScreenPt) -> PagePt {
        let page = self.view.screen_to_page(screen);
        if let Some(idx) = &self.index {
            let r = self.view.snap_radius_page(SNAP_PX);
            if let Some((pt, d)) = idx.nearest(page) {
                tracing::trace!(
                    "snap query ({:.0},{:.0}) nearest ({:.0},{:.0}) d={d:.1} r={r:.1} {}",
                    page.x,
                    page.y,
                    pt.x,
                    pt.y,
                    if d <= r { "HIT" } else { "miss" }
                );
                if d <= r {
                    return pt;
                }
            }
        }
        page
    }

    pub fn on_pointer_move(&mut self, screen: ScreenPt) {
        // Snap in both tools so the snap marker always shows on hover.
        self.cursor = Some(self.snap(screen));
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

    /// Set the scale directly (e.g. from a stated 1:N ratio) and return to measuring.
    pub fn set_scale(&mut self, s: Scale) {
        self.scale = Some(s);
        self.tool = Tool::Measure;
    }

    pub fn finish_set_scale(&mut self, real_len: f64, unit: Unit) {
        if let Some(s) = self.set_scale.finish(real_len, unit) {
            self.scale = Some(s);
            self.set_scale.clear();
            self.tool = Tool::Measure;
        }
    }

    /// Format a page-space length in real units if a scale is set, else points.
    pub fn format_len(&self, page_len: f64) -> String {
        match self.scale {
            Some(s) => s.format(page_len),
            None => format!("{page_len:.0} pt"),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::PagePt;

    fn state_with_vertex_at(p: PagePt) -> AppState {
        // Second vertex far away so `p` is unambiguously the nearest.
        let mut s = AppState::new();
        s.set_geometry(vec![Polyline(vec![
            p,
            PagePt {
                x: p.x + 200.0,
                y: p.y + 200.0,
            },
        ])]);
        s
    }

    #[test]
    fn click_snaps_to_nearby_vertex() {
        // zoom 1.0, identity pan: screen == page. Vertex at (100,100); click at (104,100).
        let mut s = state_with_vertex_at(PagePt { x: 100.0, y: 100.0 });
        s.set_tool(Tool::Measure);
        s.on_click(ScreenPt { x: 104.0, y: 100.0 });
        assert_eq!(s.dimensions().pending(), Some(PagePt { x: 100.0, y: 100.0 }));
    }

    #[test]
    fn set_scale_then_measure_reads_real_units() {
        let mut s = AppState::new();
        s.set_tool(Tool::SetScale);
        s.on_click(ScreenPt { x: 0.0, y: 0.0 });
        s.on_click(ScreenPt { x: 100.0, y: 0.0 });
        s.finish_set_scale(3000.0, Unit::Mm);
        assert_eq!(s.active_tool(), Tool::Measure);
        // Measure a 50-pt segment -> 1500 mm.
        s.on_click(ScreenPt { x: 0.0, y: 0.0 });
        s.on_click(ScreenPt { x: 50.0, y: 0.0 });
        let dims = s.dimensions().committed();
        assert_eq!(dims.len(), 1);
        let len = dims[0].0.distance(&dims[0].1);
        assert_eq!(s.format_len(len), "1500.0 mm");
    }

    #[test]
    fn escape_cancels_pending_dimension() {
        let mut s = AppState::new();
        s.set_tool(Tool::Measure);
        s.on_click(ScreenPt { x: 10.0, y: 10.0 }); // start
        assert!(s.dimensions().pending().is_some());
        s.cancel();
        assert!(s.dimensions().pending().is_none());
    }
}
