# Packaging and releasing

How to build Vernier as a Flatpak, cut a release, and publish it on Flathub.
This directory holds the manifest, the AppStream metainfo, the desktop entry,
the icon, the vendored crate list and the screenshots.

| File | Purpose |
| --- | --- |
| `io.github.genneth.Vernier.yml` | Flatpak manifest (GNOME 51 runtime, offline cargo build). Its git source is this working tree |
| `io.github.genneth.Vernier.metainfo.xml` | AppStream metadata: description, screenshots, **release history** |
| `io.github.genneth.Vernier.desktop` | Desktop entry |
| `io.github.genneth.Vernier.svg` | Icon (a vernier caliper) |
| `cargo-sources.json` | Every crate in `Cargo.lock` as a Flatpak source, so the build needs no network |
| `screenshots/*.png` | The store screenshots, produced by `scripts/screenshots.sh` |
| `make-submission.sh` | Rewrites the manifest for a tagged release, for Flathub |

## Build and install locally

The runtimes and the builder live in the **system** Flatpak installation; the
app itself installs at **user** scope from a local repository. Once:

```sh
sudo flatpak install flathub org.flatpak.Builder org.gnome.Platform//51 org.gnome.Sdk//51 \
    org.freedesktop.Sdk.Extension.rust-stable//26.08 org.freedesktop.Sdk.Extension.llvm22//26.08
```

Then, from the repository root, after committing (the manifest builds the
committed `main`, not the working tree):

```sh
flatpak run --system org.flatpak.Builder --user --force-clean --sandbox \
    --repo="$REPO_DIR" build-dir packaging/io.github.genneth.Vernier.yml
flatpak build-update-repo "$REPO_DIR"
flatpak install --user <remote-for-REPO_DIR> io.github.genneth.Vernier   # first time
flatpak update  --user io.github.genneth.Vernier                          # afterwards
```

`REPO_DIR` is whatever local OSTree repository you serve your own builds from.

## Lint

Flathub's linter must pass on the manifest, the metainfo and the built repo.
The appstream check reports the screenshot URLs as unreachable until the
release tag they point at exists on GitHub; that is expected before tagging.

```sh
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest packaging/io.github.genneth.Vernier.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder appstream packaging/io.github.genneth.Vernier.metainfo.xml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo "$REPO_DIR"
```

On the local manifest the manifest check reports `module-vernier-source-git-no-url`,
because the source is a path. The submission manifest (below) has a URL and passes.

## Regenerate the vendored crate list

Whenever `Cargo.lock` changes:

```sh
curl -sLo scratch/flatpak-cargo-generator.py \
    https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
uv run scratch/flatpak-cargo-generator.py Cargo.lock -o packaging/cargo-sources.json
```

The generator declares its own dependencies (PEP 723), so `uv run` needs no
setup. Commit the result with the lock file.

## Cut a release

The version lives in one place, `[workspace.package]` in `Cargo.toml`; the
test `packaging_consistency` fails until the metainfo's newest `<release>`
matches it.

1. Bump `version` in `Cargo.toml`. Run `cargo build` so `Cargo.lock` follows.
2. Add a `<release>` at the top of the metainfo's `<releases>` with today's
   date and two or three sentences on what changed for the user. Point the
   screenshot URLs at the new tag.
3. If `Cargo.lock` changed, regenerate `cargo-sources.json`.
4. Retake the screenshots if the UI changed:
   `SWAY_CFG=scripts/sway-screenshot.cfg scripts/gui-verify.sh scripts/screenshots.sh 4`
5. `cargo test`, `cargo clippy --all-targets`, and the local Flatpak build and
   lint above.
6. Commit, then tag and push:
   ```sh
   git tag -a v0.2.0 -m "Vernier 0.2.0"
   git push origin main v0.2.0
   ```
7. Produce the submission files:
   ```sh
   packaging/make-submission.sh v0.2.0
   ```
   This writes `scratch/flathub-submission/io.github.genneth.Vernier.yml`
   (git source = public URL + tag + commit) and `cargo-sources.json`.

## Publish on Flathub

**First submission.** Flathub takes apps through one pull request against the
`new-pr` branch of `flathub/flathub`, then gives the app its own repository.

1. Fork `flathub/flathub` on GitHub with "copy the master branch only"
   **unchecked**, then:
   ```sh
   git clone --branch=new-pr git@github.com:<you>/flathub.git
   cd flathub && git checkout -b io.github.genneth.Vernier new-pr
   cp ../vernier/scratch/flathub-submission/* .
   git add io.github.genneth.Vernier.yml cargo-sources.json
   git commit -m "Add io.github.genneth.Vernier"
   git push -u origin io.github.genneth.Vernier
   ```
2. Open a pull request against **`new-pr`** (never `master`) titled
   `Add io.github.genneth.Vernier`. The PR template asks for a short video of
   the app running as the Flatpak on Linux; `scripts/record-flatpak-demo.sh`
   produces one (WebM, well under GitHub's 10 MB limit) on the headless
   compositor. Write the description yourself: Flathub
   requires that AI tools do not open or write submission PRs or replies, and
   that any AI-generated code or packaging is disclosed with its extent.
3. Answer reviewer comments; when they are resolved comment `bot, build` for a
   test build. Do not close the PR or merge `master` into it.
4. After the merge Flathub creates `flathub/io.github.genneth.Vernier` and
   invites you. Accept within a week and have GitHub two-factor authentication
   on. The first build publishes within a couple of hours. Sign in to the
   Flathub developer portal with the GitHub account that owns
   `genneth/vernier` to mark the app verified.

**Updates.** Never resubmit. Cut a release as above, run `make-submission.sh`,
copy the two files into a branch of `flathub/io.github.genneth.Vernier`, and
open a pull request there. The bot builds it; check the test build works,
then merge, which publishes. Keep the runtime current (Flathub requires the
latest runtime at submission and frowns on end-of-life ones) and respond to
issues in that repository.

## Requirements this packaging meets

Kept here so a change does not silently break one.

- App id `io.github.genneth.Vernier`: four components, `io.github.` prefix,
  repository `github.com/genneth/vernier` reachable.
- Build is offline; all sources are the tagged git tree plus crates.io
  archives with checksums.
- Permissions: Wayland, fallback X11 with `--share=ipc`, `--device=dri`. Files
  come through the portal.
- Metainfo: id, name, summary (10–35 chars), developer, description, launchable,
  branding colours, homepage/bugtracker/vcs/help URLs, OARS rating, `requires`
  for keyboard, pointing and a 768 px display, three captioned screenshots at
  1000×700 with window decorations, and a dated `<release>` per version.
- Screenshot URLs reference a tag, never a branch.
- Licence file installed to `/app/share/licenses/io.github.genneth.Vernier`.
- Every version has an annotated git tag `vX.Y.Z` on `main`.
