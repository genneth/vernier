# Packaging Vernier as a Flatpak

Vernier builds, installs, and runs as a Flatpak. This directory holds everything needed to
submit to **Flathub**; the remaining submission steps are documented at the bottom.

## Build & install locally

The build uses the GNOME 50 SDK with the `rust-stable` and `llvm22` SDK extensions. Install once:

```sh
flatpak install flathub org.flatpak.Builder \
    org.gnome.Platform//50 \
    org.gnome.Sdk//50 \
    org.freedesktop.Sdk.Extension.rust-stable//25.08 \
    org.freedesktop.Sdk.Extension.llvm22//25.08
```

Then, from the repository root:

```sh
flatpak run org.flatpak.Builder --user --force-clean --install \
    build-dir packaging/io.github.genneth.Vernier.yml
flatpak run io.github.genneth.Vernier
```

The build is **offline** (Flathub forbids network access during builds): every crate is
vendored via `cargo-sources.json`, generated from `Cargo.lock` with
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools)
(`cargo/flatpak-cargo-generator.py`):

```sh
python3 packaging/flatpak-cargo-generator.py Cargo.lock -o packaging/cargo-sources.json
```

Regenerate `cargo-sources.json` whenever `Cargo.lock` changes.

## Lint

```sh
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest packaging/io.github.genneth.Vernier.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo <build-repo>
```

## Files

- `io.github.genneth.Vernier.yml` — manifest (GNOME 50 runtime; offline cargo build)
- `io.github.genneth.Vernier.metainfo.xml` — AppStream metadata
- `io.github.genneth.Vernier.desktop` — desktop entry
- `io.github.genneth.Vernier.svg` — icon (a vernier caliper)
- `cargo-sources.json` — vendored crate sources for the offline build
- `screenshots/measure.png` — referenced by the metainfo (measuring on the synthetic test plan)

## What's left to actually submit to Flathub

1. **Switch the manifest's source** from the local `path: ..` git source to the public URL
   (fixes `module-vernier-source-git-no-url`):
   ```yaml
   sources:
     - type: git
       url: https://github.com/genneth/vernier.git
       tag: v0.1.0
       commit: <sha of that tag>
   ```
2. Re-run the linter until clean.
3. Screenshots are captured on Wayland via `scripts/gui-verify.sh` (see `scripts/examples/measure-flow.sh`).
4. Fork `flathub/flathub`, branch off `new-pr`, add the manifest, open a PR titled
   `Add io.github.genneth.Vernier`, comment `bot, build`. After merge you get a dedicated
   repo that auto-builds on every push (~1–2 h to publish).
