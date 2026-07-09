# Vernier architecture

Vernier separates a **pure, headless core** (`crates/core`, unit-testable without GTK) from a thin
GTK4/libadwaita shell (`crates/app`). Only the `ui` layer touches GTK; everything else is plain
Rust, and that is where the real logic lives.

## Coordinate spaces (the crux)

Three spaces, enforced as distinct newtypes (`PagePt`, `ScreenPt`) so they cannot be mixed — this
is the load-bearing invariant of the whole app:

1. **Page space** — PDF points, top-left origin, y-down. **All geometry is stored here.**
2. **Screen space** — widget pixels. `screen = view · page`, where `view` = zoom scale + pan offset.
3. **Real space** — mm / m / ft. `real = scale · page_length`, where
   `scale = typed_length / measured_page_length` (real units per point).

Snapping runs in **page space**. Flow per cursor sample: `screen → page` → snap → `page → screen`
(draw) and `page → real` (readout).

## Modules

- **`pdf`** — `PdfBackend` trait (`render_region`, `extract_geometry`) + the MuPDF implementation.
  The trait isolates MuPDF (AGPL) so it could be swapped for a differently-licensed backend such as
  pdfium — a one-module change. **No other module may call `mupdf` directly.**
- **`geometry`** — domain types, the coordinate newtypes, curve→polyline flattening (`kurbo`);
  produces the vertex set for snapping.
- **`snap`** — `rstar` R-tree over vertices; `nearest_vertex(page_pt, radius_pt)`, where
  `radius_pt = SNAP_PX / zoom` so the snap radius is constant in screen pixels.
- **`scale`** — scale model; parses lengths (`"3000 mm"`, bare numbers default to mm, inches/feet)
  and ratios (`1:50`); formats readouts.
- **`view`** — view transform, `screen ↔ page` mapping, re-render-on-zoom policy (never upscale a
  cached bitmap — crispness).
- **`tools`** — interaction state machines (`SetScaleTool`, `MeasureTool`, shared
  `SegmentBuilder`): pointer events + snap results in, overlay geometry + readout state out.
- **`app`** — central headless state (document, page, view, scale, active tool, measurements) +
  update loop; the GTK shell is a thin projection of this state.
- **`ui`** (in `crates/app`) — gtk4/libadwaita window, canvas widget, thumbnail sidebar, readout.

## Data flow

```
open file ──▶ pdf.load
                │
   ┌────────────┴───────────────┐
   ▼                            ▼
extract_geometry            render_region(page, view.matrix)
   │ flatten (kurbo)            │
   ▼                            ▼
snap.build (rstar R-tree)   GdkMemoryTexture ──▶ canvas

pointer-move: screen→page ─▶ snap.nearest_vertex ─▶ tool.preview ─▶ readout
click:        tool consumes snapped point (add vertex / set scale endpoint)
pan:          translate cached texture (no re-render)
zoom:         recompute view; re-render region async ─▶ new texture
```

## Rendering

GTK4 is a retained GPU scene graph, so the canvas presents a **`GdkMemoryTexture`** (GPU-cached;
pan = transform, zoom = new texture) plus a small `append_cairo` overlay node — **not** a per-frame
Cairo `DrawingArea` blit, which re-uploads every frame. Only the visible viewport region is
rasterised, bounding zoom cost; a coarse full-page preview covers exposure during pans, and
per-page display lists are cached.

## Threading

MuPDF `render_region` / `extract_geometry` run on a worker thread; results are posted to the GTK
main context. The main thread never blocks on MuPDF, so pan/zoom stay smooth. Extraction is
one-shot per page; render-on-zoom is the recurring async job, debounced during active scroll.

## Error handling

- `Result` through the core; `ui` surfaces failures as dialogs/toasts — never panics.
- A page with no vector geometry (e.g. a scan) still renders, with a non-blocking notice that
  snapping is unavailable.
- Measuring before a scale is set shows page-points with a "set scale" prompt rather than blocking.

## Testing

- The core modules are unit-tested headless.
- `crates/core/tests/pdf_fixture.rs` exercises the full MuPDF chain against a **committed
  synthetic vector fixture** (`tests/fixtures/plan.pdf`, regenerable via `make_fixture.py`);
  set `VERNIER_TEST_PDF=<file>` to also smoke-test extraction on a real drawing.
- The GTK shell can be driven headlessly over the AT-SPI accessibility tree, on its real Wayland
  rendering path — see [`scripts/gui-verify.sh`](../scripts/gui-verify.sh) (headless sway + grim +
  `scripts/atspi_tool.py`; worked example in `scripts/examples/measure-flow.sh`).

## Proven `mupdf` 0.7 crate notes

- `Device::from_native(dev)` and `Path::walk(walker)` **consume by value** and give no accessor
  back — share counters/results via `Rc<RefCell<…>>` or a channel between device and walker.
- Apply the path **CTM** when extracting geometry, so geometry lands in the same `[0,0,pw,ph]`
  frame as the render — else snapping is offset (regression-tested in the fixture tests).
- MuPDF emits **RGBA**; GTK's native format is BGRA-premultiplied — pick the matching
  `GdkMemoryFormat` when wrapping the pixmap buffer (or convert).
