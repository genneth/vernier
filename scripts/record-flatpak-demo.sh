#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Record the showcase video Flathub's submission PR template asks for, of the
# INSTALLED FLATPAK running on the headless compositor:
#
#   SWAY_CFG=scripts/sway-screenshot.cfg APP=/bin/true scripts/gui-verify.sh scripts/record-flatpak-demo.sh 1
#   # then, on the host (ffmpeg-free has VP9 but no working H.264):
#   cd scratch/video && ls frames | sort | sed s/.png// | awk 'NR>1{printf "duration %.4f\n",($1-p)/1e9} {printf "file '"'"'frames/%s.png'"'"'\n",$1;p=$1}' > list.txt
#   ffmpeg -f concat -safe 0 -i list.txt -vsync vfr -vf format=yuv420p -c:v libvpx-vp9 -b:v 0 -crf 33 demo.webm
#
# The app is launched on the host (flatpak-spawn --host) against this
# compositor's Wayland socket by absolute path, on the host session bus so
# portals work; AT-SPI therefore goes through the host a11y bus too. Frames
# land in scratch/video/frames with nanosecond timestamps for exact pacing.
# Host-specific: assumes uid 1000 and a toolbox that shares /tmp with the host.
set -u
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOSTBUS=unix:path=/run/user/1000/bus
FIX="$REPO/crates/core/tests/fixtures"
OUT="${OUT_DIR:-$REPO/scratch/video}"; rm -rf "$OUT/frames"; mkdir -p "$OUT/frames"
TOOL() { DBUS_SESSION_BUS_ADDRESS=$HOSTBUS python3 "$REPO/scripts/atspi_tool.py" "$@"; }
hover() { swaymsg "seat seat0 cursor set $1 $2" >/dev/null; }
rclick() { swaymsg "seat seat0 cursor press button3" >/dev/null; sleep 0.1; swaymsg "seat seat0 cursor release button3" >/dev/null; }
glide() { # glide X0 Y0 X1 Y1 STEPS
    for ((k=0; k<=$5; k++)); do
        hover $(( $1 + ($3-$1)*k/$5 )) $(( $2 + ($4-$2)*k/$5 )); sleep 0.04
    done
}
stop() { touch "$OUT/stop"; wait "$CAP" 2>/dev/null; kill "$FP" 2>/dev/null; }

rm -f "$OUT/stop"
( while [ ! -f "$OUT/stop" ]; do grim -c "$OUT/frames/$(date +%s%N).png"; sleep 0.05; done ) & CAP=$!

DBUS_SESSION_BUS_ADDRESS=$HOSTBUS flatpak-spawn --host \
    --env=XDG_RUNTIME_DIR=/run/user/1000 --env=DBUS_SESSION_BUS_ADDRESS=$HOSTBUS \
    --env=WAYLAND_DISPLAY="$VERNIER_RT/$WAYLAND_DISPLAY" --env=GDK_BACKEND=wayland --env=GTK_A11Y=atspi \
    --env=VERNIER_AUTO_OPEN="$FIX/plan.pdf" \
    flatpak run --user --filesystem="$FIX:ro" io.github.genneth.Vernier >"$OUT/flatpak-app.log" 2>&1 &
FP=$!
sleep 4
swaymsg -t get_tree | grep -o '"app_id": "[^"]*"' | sort -u
if ! TOOL activate vernier button Open; then echo "no window"; stop; tail "$OUT/flatpak-app.log"; exit 1; fi
sleep 6

# Set scale 1:50 from the sheet's ratio.
TOOL activate vernier "toggle button" "Set scale"; sleep 1.5
TOOL settext vernier text "Scale ratio" "50"; sleep 1.0
TOOL activate vernier button "Apply ratio"; sleep 0.8
click 500 30; sleep 0.8

# Layout as in scripts/screenshots.sh: canvas 250..970 x 87..660, Fit Page on A4.
read -r X1 X2 Y <<<"$(python3 - <<'PY'
cx, cy, cw, ch = 250, 87, 720, 573
pw, ph = 595, 842
z = min(cw / pw, ch / ph) * 0.97
ox = cx + (cw - pw * z) / 2; oy = cy + (ch - ph * z) / 2
print(round(ox + 72 * z), round(ox + 320 * z), round(oy + 342 * z))
PY
)"
# Glide onto the wall's left end (snap marker jumps on), click, glide to the right end, click.
glide 520 520 $((X1 - 6)) $((Y + 5)) 20; sleep 0.6
click "$X1" "$Y"; sleep 0.5
glide "$X1" "$Y" $((X2 + 5)) $((Y - 4)) 30; sleep 0.6
click "$X2" "$Y"; sleep 1.2
# Move off, then zoom in twice to show crisp re-rendering, and back to Fit Page.
glide "$X2" "$Y" 700 500 10; sleep 0.8
TOOL activate vernier button "Zoom in"; sleep 1.2
TOOL activate vernier button "Zoom in"; sleep 1.5
TOOL activate vernier "toggle button" "Zoom level"; sleep 1.0
TOOL activate vernier button "Fit Page"; sleep 1.5
# Hover the dimension, delete it with a right click, then undo from the toast.
glide 700 500 $(( (X1 + X2) / 2 - 60 )) $((Y + 30)) 12
glide $(( (X1 + X2) / 2 - 60 )) $((Y + 30)) $(( (X1 + X2) / 2 - 60 )) "$Y" 8; sleep 0.8
rclick; sleep 1.5
TOOL activate vernier button "Undo"; sleep 1.5
# Second sheet.
TOOL activate vernier button "Next page"; sleep 2.5
stop
grep -E 'scale set|dimension' "$OUT/flatpak-app.log"
echo "frames: $(ls "$OUT/frames" | wc -l)"
