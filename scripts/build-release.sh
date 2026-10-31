#!/usr/bin/env bash
# Builds the signed Brainmatter.app and DMG, with the stdio bridge inside.
#
# The bridge is added through a separate config because tauri-build insists on
# every external binary existing at compile time, which would break `cargo test`
# and `tauri dev` on a fresh checkout.
#
# Signs with the self-signed identity from create-signing-cert.sh, so the Touch ID
# key keeps working across builds. There is no notarization (no Apple Developer
# account): on another Mac, Gatekeeper asks to confirm the first start.
#
# Usage: scripts/build-release.sh [app|dmg]   (default: dmg, which includes app)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${1:-dmg}"
IDENTITY="${APPLE_SIGNING_IDENTITY:-Brainmatter Dev}"

if ! security find-identity -p codesigning | grep -q "\"$IDENTITY\""; then
  echo "No signing identity \"$IDENTITY\". Run scripts/create-signing-cert.sh first." >&2
  exit 1
fi

cd "$ROOT"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
cargo build --release -p bm-cli
mkdir -p app/src-tauri/binaries
cp target/release/brainmatter-mcp "app/src-tauri/binaries/brainmatter-mcp-$TRIPLE"

cd app
APPLE_SIGNING_IDENTITY="$IDENTITY" bun run tauri build \
  --bundles "$( [ "$TARGET" = app ] && echo app || echo app,dmg )" \
  --config src-tauri/tauri.bundle.conf.json

echo
echo "Built:"
find "$ROOT/target/release/bundle" -maxdepth 2 \( -name '*.app' -o -name '*.dmg' \) -print
