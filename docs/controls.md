# Controls and readouts

A complete list of Vernier's controls, the scale-entry syntax, and what the
on-screen readouts mean. For a guided introduction see the
[tutorial](tutorial.md).

## Mouse

| Action | Effect |
| --- | --- |
| Move | Shows the snap marker (orange square) at the point a click would use |
| Left click | Places a point: the first click of a dimension or calibration, or the second click that completes it |
| Left click on a dimension's × | Deletes that dimension |
| Right click on a dimension | Deletes that dimension |
| Scroll wheel | Zooms about the pointer, in steps of 10 % |
| Middle-button drag | Pans |
| Click a thumbnail | Shows that page |

Dimension hover: a dimension counts as hovered when the pointer is within
12 px of its line or over its label (and the space just right of the label,
where the × appears). Of several candidates the nearest line wins.

## Keyboard

| Keys | Effect |
| --- | --- |
| Escape | Cancels the half-placed dimension, or the half-placed calibration |
| Page Up / Page Down | Previous / next page |
| Ctrl + `+` (or Ctrl + `=`) | Zoom in by 25 % |
| Ctrl + `−` | Zoom out by 25 % |
| Ctrl + `0` | Fit page |

The same table is in the app under **Menu → Keyboard Shortcuts**.

## Header bar

| Control | Effect |
| --- | --- |
| Sidebar toggle | Shows or hides the page thumbnails. The sidebar opens by itself for files with more than one page |
| Open | Chooses a PDF. Vernier also opens a file given on the command line or via "Open with" |
| `−` / zoom level / `+` | Zoom out, choose a zoom, zoom in |
| Set scale | Opens the scale panel (see below) |
| ☰ | Clear Measurements, Keyboard Shortcuts, About |

## Zoom levels

The zoom menu offers three modes and five fixed levels.

- **Fit Page** and **Fit Width** are modes: they re-fit when the window is
  resized or the page changes. Fit Width keeps your vertical position.
- **Actual Size (100 %)** and the percentages are relative to physical size:
  at 100 % one PDF point is 1/72 inch on the monitor, using the size the
  monitor reports. On a monitor that reports no size, 96 dpi is assumed.
- Any manual zoom (wheel, buttons, a percentage) leaves the fit modes.

Zoom is limited to the range 5 %–4000 % of one screen pixel per point.

## Snapping

Snapping locks the pointer to the nearest **vertex** of the page's vector
geometry when one is within 12 screen pixels. Vertices are the ends and corners
of every stroked or filled path, with curves flattened to short segments.
There is no midpoint, intersection or perpendicular snapping yet.

A page with no vector geometry (a scanned drawing, or a PDF made of images)
renders normally and can still be measured by eye. A message says so when the
page loads, and the snap marker then simply follows the pointer.

## The scale panel

The scale converts page lengths (PDF points) to real lengths. It applies to
every page of the document until you change it, and is not saved.

**Ratio row.** Type the drawing's plot ratio as the number after `1 :`, so
`50` for a 1:50 drawing. This assumes the PDF is at true plot size (an A1
drawing exported as an A1 PDF). If the PDF was plotted to a different sheet
size, use the measured method instead. The unit menu in the lower row chooses
the readout unit.

**Measured row.** Click **Pick two points on the drawing**, click the two ends
of a dimension you know, type its real length, choose its unit, and press
**Apply**. The two picked points are drawn as a dashed magenta line. If the
points coincide, or only one is placed, Vernier says so and waits.

The unit menu offers `mm`, `m`, `ft` and `in`.

## Readouts

- A completed dimension shows its length in a dark label at its midpoint, to
  one decimal place, in the chosen unit: `4374.4 mm`.
- Before a scale is set, lengths are shown in PDF points: `248 pt`.
- The pending dimension shows its live length beside the pointer.

## Command line

```
vernier [FILE.pdf]
```

Logging goes to stderr and is controlled by `RUST_LOG`, for example
`RUST_LOG=vernier=debug`. The default level is `info`.
