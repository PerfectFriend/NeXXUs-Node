#!/usr/bin/env bash
# H-12: verify a NeXXus GitHub Release against the committed minisign public key.
# Usage: ./scripts/verify-release.sh <release-tag> [asset-dir]
# Requires: gh (authenticated), minisign, curl.
set -euo pipefail
TAG="${1:?usage: verify-release.sh <tag> [dir]}"
DIR="${2:-/tmp/nexxus-release-$TAG}"
KEY="$(dirname "$0")/../keys/nexxus-release-minisign.pub"
mkdir -p "$DIR" && cd "$DIR"
gh release download "$TAG" --pattern 'SHA256SUMS*' --repo PerfectFriend/NeXXuS-Node-Hermes-Rust
for f in SHA256SUMS SHA256SUMS.minisig; do
  [ -f "$f" ] || { echo "MISSING: $f"; exit 1; }
done
MINISIGN_PASSWORD='' minisign -V -p "$KEY" -m SHA256SUMS -x SHA256SUMS.minisig
echo "signature OK; downloading assets listed in SHA256SUMS..."
gh release download "$TAG" --pattern '*' --repo PerfectFriend/NeXXuS-Node-Hermes-Rust -D "$DIR" || true
sha256sum -c SHA256SUMS
echo "ALL ASSETS VERIFIED against $TAG"
