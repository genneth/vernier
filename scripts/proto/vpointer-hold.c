// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Persistent virtual-pointer "capability holder" for headless-sway GUI
// verification (scripts/gui-verify.sh).
//
// A headless wlroots seat advertises NO pointer capability, so Wayland
// clients (GTK) never bind wl_pointer and every injected pointer event is
// dropped — this is why one-shot injectors (wlrctl) fail on the headless
// backend: their virtual pointer exists only for the injector's lifetime,
// and the capability flaps away before the app can bind.
//
// This program creates one zwlr_virtual_pointer_v1 and then just stays
// connected, so seat0 advertises pointer for the whole session and
// compositor-side injection (`swaymsg seat seat0 cursor ...`) is delivered.
//
// Built on demand by gui-verify.sh:
//   wayland-scanner client-header proto.xml vp.h
//   wayland-scanner private-code  proto.xml vp.c
//   cc -o vpointer-hold vpointer-hold.c vp.c -lwayland-client

#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <wayland-client.h>

#include "wlr-virtual-pointer-unstable-v1-client.h"

static struct zwlr_virtual_pointer_manager_v1 *mgr;

static void on_global(void *data, struct wl_registry *reg, uint32_t name,
                      const char *iface, uint32_t version) {
    (void)data;
    if (strcmp(iface, zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
        mgr = wl_registry_bind(reg, name,
                               &zwlr_virtual_pointer_manager_v1_interface, 1);
    }
}

static void on_global_remove(void *data, struct wl_registry *reg,
                             uint32_t name) {
    (void)data; (void)reg; (void)name;
}

static const struct wl_registry_listener listener = {on_global,
                                                     on_global_remove};

int main(void) {
    struct wl_display *dpy = wl_display_connect(NULL);
    if (!dpy) {
        fprintf(stderr, "vpointer-hold: cannot connect to wayland display\n");
        return 1;
    }
    struct wl_registry *reg = wl_display_get_registry(dpy);
    wl_registry_add_listener(reg, &listener, NULL);
    wl_display_roundtrip(dpy);
    if (!mgr) {
        fprintf(stderr,
                "vpointer-hold: no zwlr_virtual_pointer_manager_v1 global\n");
        return 1;
    }
    zwlr_virtual_pointer_manager_v1_create_virtual_pointer(mgr, NULL);
    wl_display_roundtrip(dpy);
    fprintf(stderr, "vpointer-hold: virtual pointer up, holding\n");
    // Hold the connection (and the seat's pointer capability) until killed.
    for (;;) {
        if (wl_display_dispatch(dpy) < 0)
            break;
    }
    return 0;
}
