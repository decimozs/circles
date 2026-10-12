#!/usr/bin/env bash
# Fetches the small model that names chats, and checks it. It is packed into the app, so nothing
# has to be installed or downloaded on the person's Mac.
set -euo pipefail
cd "$(dirname "$0")/.."
DEST="apps/desktop/src-tauri/resources/models/title-model.gguf"
URL="https://huggingface.co/bartowski/SmolLM2-360M-Instruct-GGUF/resolve/main/SmolLM2-360M-Instruct-Q4_K_M.gguf"
SHA256="2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2"
mkdir -p "$(dirname "$DEST")"
if [ -f "$DEST" ] && [ "$(shasum -a 256 "$DEST" | cut -d' ' -f1)" = "$SHA256" ]; then
  echo "title model is in place"
  exit 0
fi
echo "fetching the title model (about 270 MB)"
curl -L --fail -o "$DEST.part" "$URL"
GOT="$(shasum -a 256 "$DEST.part" | cut -d' ' -f1)"
if [ "$GOT" != "$SHA256" ]; then
  rm -f "$DEST.part"
  echo "the downloaded model does not match the expected checksum" >&2
  exit 1
fi
mv "$DEST.part" "$DEST"
echo "title model fetched"
