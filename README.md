# Vernier

A native Linux desktop tool for **measuring and taking off quantities from PDF drawings**
(architectural floor plans and other CAD-exported PDFs): render a page, **set a real-world scale**
from one known dimension, then measure lengths with **CAD-style snapping** — the cursor locks onto
the drawing's real vector geometry instead of eyeballed pixels.

![Measuring on a floor plan](packaging/screenshots/measure.png)

**Name:** after the **vernier scale** (Pierre Vernier, 1631) — the little sliding scale that lets a
ruler read *precisely*. Precision measurement is the whole point.

## Why

PDF viewers and editors do measurement badly: their measure tools have **no snapping** and no fluid
"set scale → measure" loop, so every takeoff is eyeballed pixel-clicking. CAD-exported PDFs carry
the real line geometry inside the file — Vernier extracts it, indexes it, and snaps to it, so a
measurement means the same thing it meant in the CAD model.

## Features

- Open a PDF; page navigation with a thumbnail sidebar.
- Smooth pan/zoom, crisp at any magnification (pages re-render at the current zoom — never an
  upscaled bitmap).
- **Set scale** from a known dimension: click its two endpoints (snapped), type the real length
  (`3000 mm`, `3 m`, `10 ft`), or enter a drawing ratio like `1:50` directly.
- **Measure** two-point dimensions with vertex/endpoint snapping, on-page labels, and a live
  readout in real units.
- Written as a pure, headless, unit-tested Rust core with a thin GTK4/libadwaita shell —
  see [docs/architecture.md](docs/architecture.md).

## Build

Rust stable plus the GTK stack development libraries are required:
GTK4 ≥ 4.14, libadwaita ≥ 1.5, and (for MuPDF's build) clang, fontconfig and freetype headers.
On Fedora: `gtk4-devel libadwaita-devel clang-devel fontconfig-devel freetype-devel cmake`.

```sh
cargo build --release
cargo run --release            # optionally: vernier <file.pdf>
cargo test                     # headless core tests, incl. a synthetic PDF fixture
```

Flatpak packaging (the intended distribution route) lives in [`packaging/`](packaging/README.md).

## Status

MVP: render → scale → snap-measure works end to end on real multi-thousand-segment floor plans.
Planned next: areas/perimeters, counts, midpoint/intersection/perpendicular snaps, ortho lock,
categories, CSV/annotated-PDF export, annotation persistence.

## License

**AGPL-3.0-or-later** (a consequence of rendering with [MuPDF](https://mupdf.com/), which is AGPL).
The MuPDF dependency is isolated behind a single `PdfBackend` trait, so a differently-licensed
backend could be swapped in without touching the rest of the app.
