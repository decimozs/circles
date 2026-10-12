#!/usr/bin/env bash
# Fails a release build when the title model is missing or is not the one that was expected.
set -euo pipefail
cd "$(dirname "$0")/.."
DEST="apps/desktop/src-tauri/resources/models/title-model.gguf"
SHA256="2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2"
if [ ! -f "$DEST" ]; then
  echo "the title model is missing: run pnpm model:fetch" >&2
  exit 1
fi
if [ "$(shasum -a 256 "$DEST" | cut -d' ' -f1)" != "$SHA256" ]; then
  echo "the title model is not the expected file: run pnpm model:fetch" >&2
  exit 1
fi
echo "title model ok"
