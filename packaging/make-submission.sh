#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Produce the two files Flathub needs from a release tag:
#
#   packaging/make-submission.sh v0.2.0 [OUT_DIR]
#
# OUT_DIR (default scratch/flathub-submission) receives the manifest with its
# git source rewritten from this working tree to the public URL, the tag, and
# the tag's commit, plus the matching cargo-sources.json. Copy both into a
# branch of your flathub/flathub fork (first submission) or of the
# flathub/io.github.genneth.Vernier repo (updates).
set -euo pipefail
TAG="${1:?usage: make-submission.sh TAG [OUT_DIR]}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${2:-$REPO/scratch/flathub-submission}"
ID=io.github.genneth.Vernier
URL=https://github.com/genneth/vernier.git

cd "$REPO"
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || { echo "no such tag: $TAG"; exit 1; }
COMMIT="$(git rev-list -n1 "$TAG")"   # the commit an annotated tag points at
if ! git ls-remote --tags origin "refs/tags/$TAG" | grep -q .; then
    echo "warning: $TAG is not on origin yet; Flathub cannot fetch it until you push it" >&2
fi
if ! git diff --quiet HEAD -- Cargo.lock packaging/cargo-sources.json; then
    echo "Cargo.lock or cargo-sources.json has uncommitted changes" >&2; exit 1
fi

mkdir -p "$OUT"
sed -e "s|^        path: \.\.$|        url: $URL|" \
    -e "s|^        branch: main$|        tag: $TAG\n        commit: $COMMIT|" \
    -e '/^# Flatpak manifest for local builds/,/^# public URL, tag and commit/d' \
    "packaging/$ID.yml" > "$OUT/$ID.yml"
cp packaging/cargo-sources.json "$OUT/cargo-sources.json"

grep -q "commit: $COMMIT" "$OUT/$ID.yml" || { echo "manifest rewrite failed"; exit 1; }
echo "wrote $OUT/$ID.yml (tag $TAG, commit $COMMIT) and $OUT/cargo-sources.json"
echo "lint:  flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest $OUT/$ID.yml"
