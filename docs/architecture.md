# Architecture

This page explains how Vernier is put together and why. It is the place to
read before changing the structure of the code. For what the app does from the
user's side, see [controls](controls.md); for how to build and test it, see
[AGENTS.md](../AGENTS.md).

## One idea: a headless core under a thin shell

Vernier is two crates. `crates/core` (`vernier_core`) holds everything that
can be reasoned about without a window: the PDF reader, the geometry, the snap
index, the scale model, the tools and the application state. It has no GTK
dependency and is tested headlessly. `crates/app` (`vernier`) is the GTK4 and
libadwaita shell: it turns events into calls on the core and paints what the
core says.

The split is not for reuse. It exists so that the interesting logic can be
tested by calling functions, and so that every effect (drawing, threads,
dialogs) lives in one crate where it is visible. The core never performs an
effect; it is handed data and returns data.

## Three coordinate spaces, kept apart by types

Every quantity in Vernier lives in one of three spaces:

- **Page space**: PDF points (1/72 inch), origin top-left, y down. All geometry
  and all dimensions are stored here. Types: `PagePt`, `PageLen`, `PageRect`,
  `PageSize`.
- **Screen space**: logical pixels in the canvas widget. Types: `ScreenPt`,
  `ScreenRect`.
- **Real space**: millimetres, metres, feet or inches. Type: `RealLen`, which
  carries its `Unit`.

The `View` maps page to screen (`screen = page · zoom + pan`) and back. The
`Scale` maps page lengths to real lengths (`real = units_per_point · page`).
Nothing else converts between spaces, and because the types differ the compiler
rejects a page radius compared with a screen distance or a bare number passed
where a length is meant.

Snapping runs in page space: the pointer is mapped screen to page, snapped to
the nearest vertex within a radius that is fixed in screen pixels (so it is
divided by the zoom), and the result is mapped back for drawing and through the
scale for the readout.

## Modules of the core

- `pdf`: the only module that touches MuPDF. `MupdfBackend` opens a file,
  reports page sizes, extracts every stroked or filled path as flattened
  polylines in page space (applying each path's transform, so geometry and
  raster agree), and renders a page-space rectangle at a given scale by
  replaying a cached display list with a scissor. It hands out plain data
  (`Rgba`, `Polyline`), never a MuPDF type. MuPDF is AGPL, which is why
  Vernier is AGPL; keeping it in one module is what would make a different
  renderer a bounded change.
- `geometry`: the space newtypes above, polylines, curve flattening (`kurbo`)
  and the point-to-segment distance.
- `snap`: an R-tree (`rstar`) over all vertices; `nearest_vertex(point, radius)`.
- `scale`: `Unit`, `RealLen`, `Scale`, and the parsers for lengths (`"3000 mm"`,
  bare numbers are millimetres) and ratios (`"1:50"`, `"50"`). Both `Scale`
  constructors refuse zero or non-finite inputs, so a `Scale` that exists is
  always usable.
- `view`: `View`, its two mappings, and the pure view arithmetic the shell
  needs (fit page, fit width, zoom about an anchor, pan), including the zoom
  limits.
- `tools`: `SegmentBuilder` is the one primitive, "two clicks make a segment".
  `Dimensions` (measuring) and `SetScaleTool` (calibration) are both built on
  it, which is why they behave identically. Each committed `Dimension` has a
  `DimId` that is never reused, so hover state and label rectangles refer to
  dimensions by identity rather than by position in a list.
- `app`: `AppState`, the single mutable object. It owns the view, the scale,
  the snap index, the active tool and the dimensions, and exposes
  `on_pointer_move`, `on_click`, `cancel` and the rest. The shell reads it to
  draw and writes to it in response to input.

One piece of information flows the wrong way: the core cannot measure text, so
the shell reports the screen rectangle of every dimension's label after each
draw (`set_label_rects`), keyed by `DimId`, and the core uses them in its hover
test. That is the only thing the core learns from the drawing pass.

## The shell

`crates/app/src/ui` has four files with distinct jobs:

- `render.rs`: a worker thread that owns the `MupdfBackend`, which is not
  `Send`. The main thread sends it `Req` values (open, geometry, render,
  preview, thumbnail) and receives `Resp` values. Both are plain enums, so the
  whole protocol is readable in one place. Within a queued batch only the
  newest render request is served, thumbnails wait until the queue is idle,
  and every result carries the document id or page generation it belongs to so
  the receiver can drop stale ones.
- `page_view.rs`: the canvas widget. GTK4 is a retained scene graph, so
  `snapshot()` appends the page as GPU textures placed under the current view
  transform (pan and zoom therefore cost nothing on the CPU), then appends one
  Cairo node for the overlay. It layers white paper, a soft whole-page preview
  and the crisp viewport texture so a pan never shows a hole.
- `overlay.rs`: pure Cairo drawing of dimensions, calibration, labels, the ×
  badge and the snap marker. It returns what it painted (`Painted`) for
  hit-testing.
- `canvas.rs`: `PdfCanvas`, the controller. It owns the view policy (fit
  modes, debounced re-render after zoom or pan, skipping renders when the
  viewport is already covered) and reports everything the window needs as a
  `CanvasEvent`. The window handles those in one `match`.

`mod.rs` builds the window and wires GTK controllers to `AppState` and
`PdfCanvas`. It holds no measuring logic.

### Rendering policy

Only the visible region (plus a margin) is rasterised, at exactly the current
zoom, so crispness never depends on upscaling a bitmap and cost is bounded by
the viewport rather than the page. A render is requested 90 ms after the last
zoom or pan event. MuPDF returns RGBA; the texture is created as
`R8g8b8a8` to match, with no conversion.

## Guarantees under failure

- The core returns `Result` or `Option` for anything that can fail and never
  panics on user input. The fixture tests cover a missing file and an
  out-of-range page.
- Every error from the render thread is a `Resp::Error` with a user-facing
  sentence; the window shows it as a toast and logs the detail. If the render
  thread dies, the next request or the closed response channel raises the same
  toast ("The rendering thread stopped; reopen the file") rather than leaving
  the window silently frozen.
- A page with no vector geometry renders and can be measured; a toast says
  snapping is unavailable.
- Before a scale is set, lengths are shown in points, so a measurement is
  never hidden, only unconverted.
- Results for a page or document that is no longer current are discarded, so a
  slow render can never paint the wrong page.
- Dimensions are per page and per session. There is no persistence, so there
  is nothing to corrupt.

## Testing

Unit tests sit beside the code. Where an invariant can be stated for all
inputs it is a `proptest` property rather than an example: the view round trip,
zooming about an anchor leaves the anchor fixed, the R-tree agrees with a
brute-force nearest vertex, the hover test agrees with brute-force segment
distance, a formatted length parses back, delete then restore is the identity.

`crates/core/tests/pdf_fixture.rs` runs the MuPDF chain on a committed
synthetic plan (`tests/fixtures/plan.pdf`, generated by `make_fixture.py`),
including the check that extracted geometry lands in the rendered page frame.
`VERNIER_TEST_PDF=<file>` adds a smoke test on a real drawing.

`crates/app/tests/packaging_consistency.rs` makes the build fail if the
version in `Cargo.toml` and the newest release in the metainfo disagree, or if
the packaging files name a different app id.

The GTK window is verified without a person present by driving it over the
accessibility tree under a headless compositor; see
[headless GUI testing](headless-gui-testing.md).

## Notes on the `mupdf` crate (0.7)

- `Device::from_native` and `Path::walk` consume their argument and return
  nothing useful, so results are shared through `Rc<RefCell<_>>`.
- Path coordinates are path-local; multiply by the CTM the device callback
  gives you, or the snap geometry is offset from the render. The fixture test
  `geometry_aligns_with_rendered_page_frame` guards this.
- `Document` is `!Send`; hence the owning render thread.
