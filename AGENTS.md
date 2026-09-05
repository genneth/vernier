# AGENTS.md

Working rules for coding agents and new contributors. Vernier measures PDF
drawings with snapping to their vector geometry; the design is in
[`docs/architecture.md`](docs/architecture.md) and the user's view in
[`docs/controls.md`](docs/controls.md). Read the architecture page before
changing structure; this file only tells you how to work.

## Commands

Rust stable with GTK4 and libadwaita development libraries (see the README).

```sh
cargo build
cargo test                       # unit + property tests, PDF fixture tests, packaging consistency
cargo test <name>                # one test
cargo clippy --all-targets       # must be clean
cargo fmt
cargo run -- <file.pdf>          # the GTK window
scripts/gui-verify.sh scripts/examples/measure-flow.sh 4   # headless end-to-end check
```

The fixture is `crates/core/tests/fixtures/plan.pdf` (synthetic, committed;
regenerate with `make_fixture.py` beside it). `VERNIER_TEST_PDF=<file>` adds a
smoke test on a real drawing; do not commit real drawings.

## Invariants the code enforces, and you must keep

- **Coordinate spaces are types.** `PagePt`/`PageLen`/`PageRect` (PDF points),
  `ScreenPt`/`ScreenRect` (pixels), `RealLen` (mm, m, ft, in). Only `View` maps
  page to screen and only `Scale` maps page to real. If you find yourself
  passing a bare `f64` length or `(f64, f64)` point, add or use the type.
- **Only `crates/core/src/pdf` touches MuPDF.** It hands out plain data.
- **The core has no effects.** No GTK, no threads, no I/O in `vernier_core`
  apart from `pdf`. Input events go in, state and data come out.
- **Dimensions are identified by `DimId`**, never by index.
- **Failures are `Result`/`Option` and reach the user.** The render thread
  reports every error as `Resp::Error`; the window toasts it. Do not swallow
  an error into a default and do not `unwrap` on user input.
- **Version in one place.** `[workspace.package].version` in `Cargo.toml`; the
  `packaging_consistency` test fails until the metainfo's newest release agrees.

## How to work

- Write the test first. For anything that holds for all inputs, write a
  `proptest` property (see `view.rs`, `snap.rs`, `app.rs` for the style) rather
  than examples. When fixing a bug, make the failing test, watch it fail, then fix.
- Keep the shell thin. New behaviour goes into `AppState` or a core module with
  tests; `crates/app/src/ui` only wires widgets and draws.
- Every interactive widget gets an accessible name via
  `update_property(&[Property::Label(..)])`; `scripts/atspi_tool.py lint` fails
  otherwise, and the headless harness depends on the names. Details and the
  hard-won compositor facts are in
  [`docs/headless-gui-testing.md`](docs/headless-gui-testing.md).
- Rendering: the canvas presents GPU textures under the view transform plus one
  Cairo overlay node. Never draw the page with per-frame Cairo, and never upscale
  a cached bitmap instead of re-rendering at the new zoom.
- Documentation follows Diátaxis: `docs/tutorial.md` teaches, `docs/controls.md`
  and this file state facts, `docs/architecture.md` explains,
  `packaging/README.md` and `docs/headless-gui-testing.md` give procedures.
  Put new material in the page whose job it is, and do not restate one page in
  another; link.
- Releases: follow [`packaging/README.md`](packaging/README.md). Every version
  gets an annotated tag `vX.Y.Z`.

## Known `mupdf` 0.7 gotchas

`Device::from_native` and `Path::walk` consume their argument; share results
through `Rc<RefCell<_>>`. Apply the CTM to walked path points. `Document` is
`!Send`. MuPDF emits RGBA; match the `GdkMemoryFormat` rather than converting.
