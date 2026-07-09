#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Example gui-verify driver: the full measure loop on the synthetic fixture.
#   open -> a11y lint -> set a 1:50 ratio scale -> click a wall's two
#   endpoints (snapped) -> screenshot + assert the readout.
#
#   scripts/gui-verify.sh scripts/examples/measure-flow.sh 4
#
# Demonstrates every harness capability: AT-SPI actions/settext by accessible
# name, coordinate clicks via the exported `click` helper, grim screenshots,
# and app.log assertions. Outputs to $OUT_DIR (default /tmp/vernier-verify).
set -u
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${OUT_DIR:-/tmp/vernier-verify}"; mkdir -p "$OUT"
TOOL() { python3 "$REPO/scripts/atspi_tool.py" "$@"; }

sleep 1
TOOL activate vernier button Open        # VERNIER_AUTO_OPEN seam -> fixture
sleep 7   # page + thumbnails render; sidebar auto-appears; fit-page settles

TOOL lint vernier || echo "a11y lint FAILED (see above)"

TOOL activate vernier "toggle button" "Set scale"
sleep 1
TOOL settext vernier text "Scale ratio" "50"
sleep 0.3
TOOL activate vernier button "Apply ratio"
sleep 0.5
click 700 25    # dismiss the popover (no keyboard on a headless seat)
sleep 0.5

# Click the kitchen wall's endpoints: page-space (72,342)->(320,342).
# GTK4's AT-SPI extents are size-only (position always 0,0), so screen
# coords are calibrated from a grim shot of this fixed 1400x900 layout:
# with the sidebar open the page rect is x 517..1103, y 60..888.
PTS=$(python3 <<'EOF'
ox, oy, s = 517, 60, 586 / 595
for px, py in ((72, 342), (320, 342)):
    print(round(ox + px * s), round(oy + py * s))
EOF
)
read -r X1 Y1 <<<"$(echo "$PTS" | sed -n 1p)"
read -r X2 Y2 <<<"$(echo "$PTS" | sed -n 2p)"
click "$X1" "$Y1"; sleep 0.5
click "$X2" "$Y2"; sleep 0.6
swaymsg "seat seat0 cursor set 700 25" >/dev/null   # park off the drawing
sleep 0.8

grim "$OUT/measure.png"
echo "screenshot: $OUT/measure.png"

# The wall is exactly 248 pt; at 1:50 that must read 4374.4 mm.
if grep -q 'dimension placed: 4374.4 mm' "$VERNIER_RT/app.log"; then
    echo "OK: snapped measurement reads 4374.4 mm"
else
    echo "FAIL: expected 4374.4 mm; app.log says:"
    grep -E 'scale set|dimension' "$VERNIER_RT/app.log" || true
    exit 1
fi
