# Measure your first drawing

In this tutorial you will open a small floor plan, tell Vernier its scale, and
measure a wall on it. By the end you will have used every part of the
measuring loop, and you will know what the orange square and the blue lines mean.

You need Vernier installed and the sample plan from the source tree,
`crates/core/tests/fixtures/plan.pdf`. It is a synthetic two-page A4 drawing at
1:50, so you can check every number you get against the numbers here. Any
CAD-exported PDF will behave the same way; only the numbers will differ.

## 1. Open the plan

Start Vernier and click **Open** in the top-left. Pick `plan.pdf`.

The first page fills the window. Because the file has two pages, a sidebar
with a thumbnail of each appears on the left. The title bar reads
`plan.pdf` and `Page 1 of 2`.

Move the pointer over the drawing. An **orange square** follows it, and jumps
onto line ends and corners as you get close to them. That square is the snap
marker: it shows where a click would land. Vernier has read the plan's vector
geometry and is snapping the pointer to its vertices.

## 2. Set the scale from the plot ratio

The drawing's title block says *1:50 at A4*. Tell Vernier so.

Click **Set scale** at the top-right. A small panel opens. In the row that
reads `1 :`, type `50` and press **Apply** (or Enter). The panel closes.

Nothing visible changes yet; the scale only affects how lengths are reported.

## 3. Measure a wall

Find the long horizontal wall that separates the two upper rooms from the rest
of the plan, roughly a third of the way down the page. Move the pointer to its
left end until the orange square locks onto the corner, and click once.

A blue line now stretches from that corner to the pointer, with a live length
beside it. Move to the wall's right end, wait for the square to lock on, and
click again.

The line becomes a fixed **dimension** with a dark label in its middle reading
**4374.4 mm**. That is exactly right: in the drawing the wall is 248 points
long, and at 1:50 one point is 17.64 mm.

Place two or three more dimensions on other walls. Each one takes two clicks,
and each starts fresh, so you never accidentally chain them.

## 4. Change the units

Open **Set scale** again. Change the unit menu in the lower row from `mm` to
`m`, type `50` in the ratio row once more and press **Apply**.

Every label now reads in metres: the wall says **4.4 m**. The unit menu is
shared by both ways of setting the scale.

## 5. Set the scale from a known dimension instead

Suppose the sheet had no printed ratio, but you knew that wall was 4.4 metres.

Open **Set scale** and click **Pick two points on the drawing**. The panel
stays open. Click the wall's two ends, as before; the calibration line is
drawn dashed and magenta so you cannot confuse it with a measurement. Type
`4400` in the lower row, make sure the unit menu says `mm`, and press
**Apply**.

The scale is now derived from your two clicks. Because you clicked the same
corners the drawing was built from, the other dimensions still read the same.

## 6. Tidy up

Hover over any dimension. It brightens, and a small **×** appears beside its
label. Click the × (or right-click anywhere on the line) to delete it. A toast
offers **Undo** for a few seconds.

To remove them all, open the menu (the ☰ button) and choose **Clear
Measurements**. Changing page also clears them: dimensions belong to the page
they were drawn on and are not kept between sessions.

## 7. Look around

Roll the scroll wheel to zoom about the pointer, and drag with the middle
button to pan. The page stays crisp at any zoom, because it is re-rendered
rather than enlarged. **Fit Page** in the zoom menu, or Ctrl+0, brings the
whole page back.

Press Page Down, or click the second thumbnail, to see the first floor.

## What you have learned

You opened a PDF, set its scale two different ways, measured with snapping,
changed units, and deleted a dimension. Everything else in Vernier is a
variation on these moves; the [controls reference](controls.md) lists them all.
