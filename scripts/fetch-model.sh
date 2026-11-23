#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
DEST="${1:-models}"
NAME=sherpa-onnx-streaming-zipformer-en-20M-2023-02-17
[ -f "$DEST/$NAME/tokens.txt" ] && { echo "model already present: $DEST/$NAME"; exit 0; }
mkdir -p "$DEST"
curl -fL --retry 3 -o "$DEST/$NAME.tar.bz2" \
  "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/$NAME.tar.bz2"
tar xjf "$DEST/$NAME.tar.bz2" -C "$DEST"
rm "$DEST/$NAME.tar.bz2"
echo "model ready: $DEST/$NAME"
