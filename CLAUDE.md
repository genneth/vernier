# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Vernier is a native desktop tool for **measuring and taking off quantities from PDF drawings**
(architectural floor plans): render a page, set a real-world scale from one known dimension, then
measure lengths with **CAD-style snapping** onto the drawing's real vector geometry. See
`README.md` for the why, and `docs/architecture.md` for the authoritative design — read it before
making architectural changes.

## Build / test / run

Rust stable + GTK4/libadwaita development libraries (see README "Build" for the package list).

```sh
cargo build
cargo test                # headless core tests, incl. the committed synthetic PDF fixture
cargo test <test_name>    # single test
cargo run                 # GTK window; optionally `cargo run -- <file.pdf>`
```

The core test fixture is `crates/core/tests/fixtures/plan.pdf` (synthetic, committed; regenerate
with `make_fixture.py` next to it). Set `VERNIER_TEST_PDF=<file>` to also smoke-test geometry
extraction against a real CAD-exported drawing.

### Verifying the GUI headlessly (no human needed)

**Wayland-first**: the app runs under a **headless sway** compositor on its real Wayland/GPU
rendering path (no Xvfb, no X11 tools, no cairo fallback). Drive it **semantically via the AT-SPI
accessibility tree** (the desktop analogue of Playwright role/name locators); use `grim`
screenshots **only** for visual checks (canvas pixels); inject coordinate clicks (canvas
measuring) via **sway IPC**.

```sh
# boot compositor + a11y bus + app, run a driver script against it:
scripts/gui-verify.sh DRIVER.sh [WAIT_SECS]
# full worked example (open → lint → scale → snapped measure → assert):
scripts/gui-verify.sh scripts/examples/measure-flow.sh 4
```

Inside a DRIVER.sh (app is up; WAYLAND_DISPLAY/SWAYSOCK/a11y bus in scope):
```sh
python3 scripts/atspi_tool.py dump vernier                 # whole widget tree (debug)
python3 scripts/atspi_tool.py lint vernier                 # unlabelled/duplicate widget audit
python3 scripts/atspi_tool.py activate vernier button Open # invoke by NAME, no coords
python3 scripts/atspi_tool.py settext vernier text "Scale ratio" 50
python3 scripts/atspi_tool.py text vernier label "<name>"  # assert on readout text
click X Y                                                  # exported helper: sway-IPC cursor click
grim OUT.png                                               # visual check; then Read OUT.png
tail "$VERNIER_RT/app.log"                                 # app + compositor logs live in $VERNIER_RT
```

Key facts (hard-won — don't regress):
- **A headless wlroots seat advertises no pointer capability** (`caps: 0`), so GTK never binds
  `wl_pointer` and injected clicks vanish — this is also why one-shot `wlrctl` fails headless.
  The harness holds a persistent virtual pointer (`scripts/proto/vpointer-hold.c`, built on
  demand) for the whole session; only then does `swaymsg seat seat0 cursor …` deliver.
- **No keyboard on the headless seat** — don't use Escape to close popovers; invoke the widget's
  action again or `click` elsewhere to dismiss.
- **`at-spi2-registryd` must be launched manually** (the harness does it): its systemd/D-Bus
  activation fails in containers, leaving the AT-SPI tree empty.
- **GTK4's AT-SPI extents are size-only** (position always 0,0) — anchor canvas clicks by
  calibrating from a `grim` screenshot of the fixed 1400×900 layout, not from `extents`.
- Give every interactive widget an accessible name in gtk4-rs via
  `widget.update_property(&[gtk4::accessible::Property::Label("…")])` (NOT `set_widget_name`,
  which is only CSS). For a **button with a visible label** the child label wins name
  computation (ARIA labelled-by precedence) — clear it first:
  `btn.update_relation(&[gtk4::accessible::Relation::LabelledBy(&[])])`.
- `scripts/atspi_tool.py lint` fails on unlabelled or duplicate interactive names — run it in
  every driver.
- This runs **only for headless verification** (no human present). With a human at the machine,
  just run the app on the real display. Needs: sway, grim, dbus-daemon, at-spi2-core,
  python3-pyatspi, wayland-devel (for the one-time vpointer-hold build).

## Architecture (big picture)

Full detail in `docs/architecture.md`. The short version:

The codebase separates a **pure, headless core** (`crates/core`, unit-testable without GTK) from a
thin GTK shell (`crates/app`). Only the `ui` module needs GTK; everything else is plain Rust and is
where the real logic lives.

**Three coordinate spaces, enforced as distinct newtypes (`PagePt` / `ScreenPt`) so they cannot be
mixed** — this is the load-bearing invariant of the whole app:
- **Page space** — PDF points, top-left origin, y-down. *All geometry is stored here.*
- **Screen space** — widget pixels. `screen = view · page` (view = zoom + pan).
- **Real space** — mm/m/ft. `real = scale · page_length`, where `scale = typed_length / measured_page_length`.

Snapping happens in **page space**: cursor maps `screen→page`, snaps, then results map back
`page→screen` (draw) and `page→real` (readout).

Module responsibilities:
- `pdf` — `PdfBackend` trait (`render_region`, `extract_geometry`) + MuPDF impl. The trait isolates
  MuPDF so it can be swapped for pdfium if licensing ever requires it. **Do not call `mupdf` directly
  outside this module.**
- `geometry` — domain types, the coordinate newtypes, curve→polyline flattening (`kurbo`).
- `snap` — `rstar` R-tree over vertices; `nearest_vertex(page_pt, radius_pt)`, `radius_pt = SNAP_PX / zoom`.
- `scale` — scale model, length parsing (`"3000 mm"`) and formatting.
- `view` — view transform, `screen↔page`, page-texture cache, re-render-on-zoom.
- `tools` — interaction state machines (`SetScaleTool`, `MeasureTool`); pointer+snap in, overlay+readout out.
- `ui` — gtk4/libadwaita window, canvas widget, readout panel.
- `app` — central state + update loop.

**Rendering:** GTK4 is a retained GPU scene graph, so the canvas presents a **`GdkMemoryTexture`**
(GPU-cached; pan = transform, zoom = new texture) plus a small `append_cairo` overlay node — **not**
a per-frame Cairo `DrawingArea` blit (which re-uploads every frame and is slow). For crispness,
**re-render the page at the new zoom matrix; never upscale a cached bitmap.**

**Threading:** MuPDF `render_region`/`extract_geometry` run on a worker thread; results are posted to
the GTK main context. The main thread never blocks on MuPDF.

## Gotchas (proven during development)

- `mupdf` 0.7: `Device::from_native(dev)` and `Path::walk(walker)` **consume by value** and give no
  accessor back — share counters/results via `Rc<RefCell<…>>` or a channel between device and walker.
- MuPDF emits **RGBA**; GTK's native format is **BGRA-premultiplied** — pick the matching
  `GdkMemoryFormat` when wrapping the pixmap buffer (or convert).
- MuPDF is **AGPL**; Vernier ships GPL/AGPL as a consequence. Keep `pdf` the only module touching it.
