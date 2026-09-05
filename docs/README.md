# Vernier documentation

Vernier measures lengths on PDF drawings, snapping to the drawing's own vector
geometry. The pages here are organised by what you are trying to do.

## Using Vernier

- **[Measure your first drawing](tutorial.md)** — a ten-minute walk-through on the
  sample plan that ships with the source: open, set the scale, measure, tidy up.
- **[Controls and readouts](controls.md)** — every mouse and keyboard control, the
  two ways of setting a scale, the length syntax, and what the readouts mean.

## Understanding Vernier

- **[Architecture](architecture.md)** — why the app is split into a headless core
  and a GTK shell, the three coordinate spaces, rendering, threading, and what the
  app guarantees when things fail.

## Developing Vernier

- **[Headless GUI testing](headless-gui-testing.md)** — driving the real GTK window
  under a headless Wayland compositor, without a person at the machine.
- **[Packaging and releasing](../packaging/README.md)** — the Flatpak build, how to
  cut a release, and how to submit or update it on Flathub.
- **[AGENTS.md](../AGENTS.md)** — the working rules for coding agents and new
  contributors: build commands, invariants, and where things live.
