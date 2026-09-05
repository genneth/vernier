# Headless GUI testing

How to exercise the real GTK window with nobody at the machine: a headless
Wayland compositor, the accessibility tree for semantic actions, and
screenshots only for pixel checks. This is what CI-style verification and the
Flathub screenshots use. If a person is present, just run the app on the
desktop instead.

## Run a driver

```sh
cargo build
scripts/gui-verify.sh DRIVER.sh [WAIT_SECS]
# the worked example: open → lint → scale → snapped measurement → assertion
scripts/gui-verify.sh scripts/examples/measure-flow.sh 4
```

`gui-verify.sh` starts a private D-Bus session, the AT-SPI registry, a
headless `sway`, a persistent virtual pointer, and the app, then runs your
driver with these in scope:

| In scope | Meaning |
| --- | --- |
| `$WAYLAND_DISPLAY`, `$SWAYSOCK`, `$DBUS_SESSION_BUS_ADDRESS` | The private compositor and buses |
| `$VERNIER_RT` | Runtime dir with `app.log`, `sway.log` and the other logs |
| `click X Y` | Exported function: press and release button 1 at output coordinates |
| `VERNIER_AUTO_OPEN` | Path the **Open** button loads directly, since no file-chooser portal exists here; defaults to the fixture plan |
| `SWAY_CFG` | Compositor layout; default `scripts/sway-headless.cfg` (one 1400×900 output, window tiled to fill it) |

Inside a driver:

```sh
python3 scripts/atspi_tool.py dump vernier                  # the widget tree
python3 scripts/atspi_tool.py lint vernier                  # unlabelled/duplicate names → fail
python3 scripts/atspi_tool.py activate vernier button Open  # invoke by accessible name
python3 scripts/atspi_tool.py settext vernier text "Scale ratio" 50
python3 scripts/atspi_tool.py text vernier label "<name>"   # read a label's text
click 700 400                                               # coordinate click on the canvas
grim out.png                                                # screenshot of the output
grep 'dimension placed' "$VERNIER_RT/app.log"               # assert on the app's log
```

Prefer accessible names for everything that has one; use coordinates only for
the canvas. Run `lint` in every driver so a widget cannot lose its name
unnoticed.

## Produce the Flathub screenshots

```sh
SWAY_CFG=scripts/sway-screenshot.cfg scripts/gui-verify.sh scripts/screenshots.sh 4
```

This uses a 1000×700 output with the window floating at 940×620, so GTK draws
its client-side shadow and rounded corners, keys the flat backdrop out to
transparency, and writes `measure.png`, `pages.png` and `scale.png` into
`packaging/screenshots/`. It fails if the snapped measurement does not read
4374.4 mm, which catches a mis-calibrated click before a wrong screenshot ships.

## Requirements

In the build environment: `sway`, `grim`, `dbus-daemon`, `at-spi2-core`,
`python3-pyatspi`, `wayland-devel` and a C compiler (for the one-time build of
`scripts/proto/vpointer-hold.c`), ImageMagick (`magick`) for the screenshot
driver, and a working GPU or software GL stack.

## Facts the harness depends on

These were each found the hard way. Do not regress them.

- **A headless wlroots seat advertises no pointer.** GTK therefore never binds
  `wl_pointer`, and injected clicks vanish. The harness keeps a virtual pointer
  attached for the whole session (`vpointer-hold`); only then does
  `swaymsg seat seat0 cursor …` deliver events. One-shot tools such as `wlrctl`
  fail here for the same reason.
- **There is no keyboard on the headless seat.** Do not press Escape to close a
  popover; click elsewhere or invoke the widget's action again.
- **`at-spi2-registryd` must be started by hand.** Its D-Bus activation fails
  in a container and the accessibility tree comes up empty.
- **GTK4 reports size-only AT-SPI extents** (position is always 0,0). Anchor
  canvas clicks by calibrating from the fixed layout, as the example drivers
  do, not from `extents`.
- **On a hybrid Intel + NVIDIA machine wlroots may pick the NVIDIA render
  node**, whose EGL does not initialise headless. The harness sets
  `WLR_RENDER_DRM_DEVICE` to the first non-NVIDIA node unless you set it.
- **Every interactive widget needs an accessible name**, set with
  `widget.update_property(&[gtk4::accessible::Property::Label("…")])`
  (`set_widget_name` is CSS only). For a button with a visible label the child
  label wins name computation, so clear the relation first:
  `btn.update_relation(&[gtk4::accessible::Relation::LabelledBy(&[])])`.
