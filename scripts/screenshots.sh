#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Produce the Flathub screenshots into packaging/screenshots/ from the
# synthetic fixture, on the headless compositor:
#
#   SWAY_CFG=scripts/sway-screenshot.cfg scripts/gui-verify.sh scripts/screenshots.sh 4
#
# The window floats at 940×620 on a 1000×700 output (see sway-screenshot.cfg),
# so GTK's client-side shadow is captured; the flat backdrop is keyed out.
set -u
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${OUT_DIR:-$REPO/packaging/screenshots}"; mkdir -p "$OUT"
TOOL() { python3 "$REPO/scripts/atspi_tool.py" "$@"; }
shot() {  # shot NAME — capture the output and key out the backdrop
    grim "$VERNIER_RT/$1.raw.png"
    magick "$VERNIER_RT/$1.raw.png" -fuzz 1% -transparent '#7f7f7f' "$OUT/$1.png"
    echo "screenshot: $OUT/$1.png"
}

sleep 1
TOOL activate vernier button Open
sleep 7   # page + thumbnails render; sidebar auto-appears; fit-page settles
TOOL lint vernier || { echo "a11y lint FAILED"; exit 1; }

# --- measure.png: 1:50 scale, one snapped wall dimension ---
TOOL activate vernier "toggle button" "Set scale"
sleep 1
TOOL settext vernier text "Scale ratio" "50"
sleep 0.3
TOOL activate vernier button "Apply ratio"
sleep 0.5
click 500 30    # dismiss the popover onto the header (no keyboard on a headless seat)
sleep 0.5

# Window at (30,40) of 940×620; header 47 px; sidebar 220 px -> canvas
# x 250..970 (720 wide), y 87..660 (573 high). Fit Page on A4 595×842 gives
# zoom = min(720/595, 573/842)*0.97 and a centred page; the kitchen wall runs
# page (72,342)->(320,342). Snapping absorbs a few px of error.
PTS=$(python3 <<'PY'
cx, cy, cw, ch = 250, 87, 720, 573
pw, ph = 595, 842
z = min(cw / pw, ch / ph) * 0.97
ox = cx + (cw - pw * z) / 2
oy = cy + (ch - ph * z) / 2
for px, py in ((72, 342), (320, 342)):
    print(round(ox + px * z), round(oy + py * z))
PY
)
read -r X1 Y1 <<<"$(echo "$PTS" | sed -n 1p)"
read -r X2 Y2 <<<"$(echo "$PTS" | sed -n 2p)"
click "$X1" "$Y1"; sleep 0.5
click "$X2" "$Y2"; sleep 0.6
swaymsg "seat seat0 cursor set $((X2 + 60)) $((Y2 + 120))" >/dev/null   # park on the plan, off the label
sleep 0.8
shot measure
if ! grep -q 'dimension placed: 4374.4 mm' "$VERNIER_RT/app.log"; then
    echo "FAIL: the wall did not snap (expected 4374.4 mm); app.log says:"
    grep -E 'scale set|dimension' "$VERNIER_RT/app.log" || true
    exit 1
fi

# --- pages.png: second sheet, thumbnails in the sidebar ---
TOOL activate vernier button "Next page"
sleep 3
swaymsg "seat seat0 cursor set 990 690" >/dev/null   # park off the window
sleep 0.5
shot pages

# --- scale.png: the Set scale popover ---
TOOL activate vernier "toggle button" "Set scale"
sleep 1.2
shot scale
echo OK
