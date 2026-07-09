#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""AT-SPI introspection + driving for the Vernier GUI.

Runs inside the a11y-enabled headless env (see scripts/gui-atspi.sh). Commands:

  dump <app>                       print the accessibility tree (role, name, text, extents)
  lint <app>                       flag unlabelled/duplicate interactive widgets (exit 1 if any)
  activate <app> <role> <name>     invoke a widget's default action (e.g. click a button) — no coords
  settext <app> <role> <name> <t>  focus an entry and set its text contents — no coords
  text <app> <role> <name>         print a widget's text / accessible name (for assertions)
  extents <app> <role> <name>      print "x y w h" desktop-coord bounding box (anchor canvas clicks)

`role` is the AT-SPI role name, e.g. "push button", "label", "image", "toggle button".
"""
import sys
import pyatspi


def find_app(name):
    desktop = pyatspi.Registry.getDesktop(0)
    for app in desktop:
        if app is not None and name.lower() in (app.name or "").lower():
            return app
    return None


def role_of(a):
    return a.getRoleName()


def text_of(a):
    try:
        t = a.queryText()
        return t.getText(0, t.characterCount)
    except NotImplementedError:
        return None


def extents_of(a, coord=pyatspi.DESKTOP_COORDS):
    try:
        e = a.queryComponent().getExtents(coord)
        return (e.x, e.y, e.width, e.height)
    except NotImplementedError:
        return None


def dump(a, depth=0):
    line = "  " * depth + f"[{role_of(a)}] name={a.name!r}"
    t = text_of(a)
    if t:
        line += f" text={t!r}"
    ex = extents_of(a)
    if ex:
        line += f" ext={ex}"
    states = [pyatspi.stateToString(s) for s in a.getState().getStates()]
    if "showing" in states:
        line += " <showing>"
    print(line)
    for c in a:
        if c is not None:
            dump(c, depth + 1)


def find(a, rolename, name):
    return pyatspi.findDescendant(
        a, lambda x: role_of(x) == rolename and (x.name or "") == name
    )


INTERACTIVE = {"push button", "button", "toggle button", "menu item",
               "check box", "radio button", "text", "combo box", "slider"}


def lint(a):
    """A11y lint: interactive widgets must have unique, non-empty accessible
    names (else drivers and assistive tech can't address them)."""
    seen, warnings = {}, []

    def walk(o):
        try:
            role = role_of(o)
            if role in INTERACTIVE:
                name = (o.name or "").strip()
                if not name:
                    warnings.append(f"unlabelled [{role}]")
                else:
                    key = (role, name)
                    if key in seen:
                        warnings.append(f"duplicate [{role}] name={name!r}")
                    seen[key] = True
            for c in o:
                if c is not None:
                    walk(c)
        except Exception:
            pass

    walk(a)
    return warnings


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    cmd, appname = sys.argv[1], sys.argv[2]
    app = find_app(appname)
    if app is None:
        sys.exit(f"app matching {appname!r} not found in AT-SPI tree")

    if cmd == "dump":
        dump(app)
        return

    if cmd == "lint":
        warns = lint(app)
        for w in warns:
            print(f"a11y-lint: {w}")
        sys.exit(1 if warns else 0)

    rolename, name = sys.argv[3], sys.argv[4]
    w = find(app, rolename, name)
    if w is None:
        sys.exit(f"widget [{rolename}] name={name!r} not found")

    if cmd == "activate":
        action = w.queryAction()
        action.doAction(0)
        print(f"activated [{rolename}] {name!r}")
    elif cmd == "settext":
        # settext <app> <role> <name> <text...> — focus + set an entry's text (no coords).
        new = " ".join(sys.argv[5:])
        try:
            w.queryComponent().grabFocus()
        except Exception:
            pass
        w.queryEditableText().setTextContents(new)
        print(f"set [{rolename}] {name!r} = {new!r}")
    elif cmd == "focus":
        w.queryComponent().grabFocus()
        print(f"focused [{rolename}] {name!r}")
    elif cmd == "text":
        print(text_of(w) if text_of(w) is not None else (w.name or ""))
    elif cmd == "extents":
        x, y, ww, hh = extents_of(w)
        print(f"{x} {y} {ww} {hh}")
    else:
        sys.exit(f"unknown command {cmd!r}")


if __name__ == "__main__":
    main()
