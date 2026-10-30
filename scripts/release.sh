#!/usr/bin/env bash
# Builds the .dmg. Universal (arm64 + x86_64) when both Rust targets are installed, else this Mac's.
# Usage: scripts/release.sh
set -euo pipefail
cd "$(dirname "$0")/.."

scripts/test-all.sh

TARGET=""
if command -v rustup >/dev/null \
  && rustup target list --installed | grep -q x86_64-apple-darwin \
  && rustup target list --installed | grep -q aarch64-apple-darwin; then
  TARGET="--target universal-apple-darwin"
else
  echo "Both macOS Rust targets are not installed (needs rustup): building for this Mac only."
  echo "For a universal build: rustup target add aarch64-apple-darwin x86_64-apple-darwin"
fi

# shellcheck disable=SC2086
pnpm tauri build --bundles dmg $TARGET

DMG=$(find target -name '*.dmg' -print0 | xargs -0 ls -t | head -1)
echo
echo "Built: $DMG"
shasum -a 256 "$DMG"
echo "Signing: set APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID before running to sign and notarize."
