#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Pure-Wayland headless GUI verification for Vernier — no Xvfb, no X11 tools.
#
#   headless sway (WLR_BACKENDS=headless) -> the app renders on its real
#                                            Wayland/GPU path, not a fallback
#   grim                     -> pixel screenshots (wlr-screencopy)
#   AT-SPI (atspi_tool.py)   -> semantic drive (widget actions) + tree audit
#   swaymsg seat cursor      -> coordinate pointer input for canvas clicks
#                               (compositor-internal injection; the wlr
#                               virtual-pointer protocol used by wlrctl is
#                               known-broken on the headless backend)
#
#   scripts/gui-verify.sh DRIVER.sh [WAIT_SECS]
#
# The driver runs with the app already up and these in scope:
#   $WAYLAND_DISPLAY, $SWAYSOCK, $DBUS_SESSION_BUS_ADDRESS (app + a11y bus)
#   $VERNIER_APP, $VERNIER_RT (log dir: app.log, sway.log, ...)
#   click X Y   — exported helper: press+release button1 at output coords
#   python3 scripts/atspi_tool.py dump|activate|settext|text|extents vernier ...
#   grim OUT.png
#
# The accessibility bus is bootstrapped WITHOUT systemd/gnome-session (D-Bus
# activation of org.a11y.atspi.Registry fails in a toolbox): launch
# at-spi-bus-launcher, then at-spi2-registryd, on a private session bus.
#
# Prereqs: sway grim at-spi2-core python3-pyatspi dbus-daemon mesa/GPU stack.
set -u

DRIVER="${1:?usage: gui-verify.sh DRIVER.sh [WAIT_SECS]}"
WAIT="${2:-3}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${APP:-$REPO/target/debug/vernier}"
[ -x "$APP" ] || { echo "binary not found: $APP (cargo build first)"; exit 1; }

RT="$(mktemp -d /tmp/vernier-wlrun.XXXXXX)"
export XDG_RUNTIME_DIR="$RT" VERNIER_RT="$RT" VERNIER_APP="$APP"
chmod 700 "$RT"
BUS="unix:path=$RT/bus"
PIDS=()
cleanup() { kill "${PIDS[@]}" 2>/dev/null; wait 2>/dev/null; }
trap cleanup EXIT

dbus-daemon --session --address="$BUS" --nofork --nopidfile >"$RT/bus.log" 2>&1 & PIDS+=($!)
sleep 0.6
export DBUS_SESSION_BUS_ADDRESS="$BUS"

# Accessibility bus without systemd: launcher owns org.a11y.Bus and spawns the
# a11y bus; registryd then owns org.a11y.atspi.Registry there.
/usr/libexec/at-spi-bus-launcher >"$RT/a11y-launch.log" 2>&1 & PIDS+=($!)
sleep 1
/usr/libexec/at-spi2-registryd >"$RT/registryd.log" 2>&1 & PIDS+=($!)
sleep 1

env -u WAYLAND_DISPLAY -u DISPLAY \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=gles2 \
    sway -c "$REPO/scripts/sway-headless.cfg" >"$RT/sway.log" 2>&1 & PIDS+=($!)
sleep 2.5
WAYLAND_DISPLAY="$(cd "$RT" && ls | grep -m1 -E '^wayland-[0-9]+$')"
export WAYLAND_DISPLAY
[ -n "$WAYLAND_DISPLAY" ] || { echo "sway made no wayland socket"; tail -15 "$RT/sway.log"; exit 1; }
SWAYSOCK="$(ls "$RT"/sway-ipc.*.sock 2>/dev/null | head -1)"
export SWAYSOCK

# A headless seat advertises no pointer capability, so clients never bind
# wl_pointer and injected events are dropped. Hold a persistent virtual
# pointer (built on demand from the vendored protocol XML) for the session.
HOLD="$REPO/target/gui-verify/vpointer-hold"
if [ ! -x "$HOLD" ]; then
    mkdir -p "$REPO/target/gui-verify"
    ( cd "$REPO/target/gui-verify" &&
      wayland-scanner client-header \
          "$REPO/scripts/proto/wlr-virtual-pointer-unstable-v1.xml" \
          wlr-virtual-pointer-unstable-v1-client.h &&
      wayland-scanner private-code \
          "$REPO/scripts/proto/wlr-virtual-pointer-unstable-v1.xml" \
          wlr-virtual-pointer-unstable-v1.c &&
      cc -O2 -I. -o vpointer-hold "$REPO/scripts/proto/vpointer-hold.c" \
          wlr-virtual-pointer-unstable-v1.c -lwayland-client
    ) || { echo "vpointer-hold build failed"; exit 1; }
fi
"$HOLD" >"$RT/vpointer.log" 2>&1 & PIDS+=($!)
sleep 0.5

# Coordinate pointer input via sway IPC (works on the headless backend).
click() {
    swaymsg "seat seat0 cursor set $1 $2" >/dev/null
    sleep 0.15
    swaymsg "seat seat0 cursor press button1" >/dev/null
    sleep 0.1
    swaymsg "seat seat0 cursor release button1" >/dev/null
}
export -f click

# Env seam: the Open button loads this path directly instead of a file dialog.
export VERNIER_AUTO_OPEN="${VERNIER_AUTO_OPEN:-$REPO/crates/core/tests/fixtures/plan.pdf}"
GDK_BACKEND=wayland GTK_A11Y=atspi "$APP" >"$RT/app.log" 2>&1 & PIDS+=($!)
sleep "$WAIT"

bash "$DRIVER"
rc=$?
grep -q panicked "$RT/app.log" && { echo "!! app panicked"; tail "$RT/app.log"; rc=1; }
exit $rc
