#!/usr/bin/env bash
# Plays a speech WAV through the default sink and checks that `santhra-cli live`
# captions it. Needs a running PipeWire session; not usable in CI.
set -euo pipefail
cd "$(dirname "$0")/.."

MODEL=${SANTHRA_MODEL_DIR:-models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17}
WAV=${1:-$MODEL/test_wavs/0.wav}
OUT=$(mktemp)

cargo build -q -p santhra-cli
target/debug/santhra-cli live --no-partial >"$OUT" 2>&1 &
CAP=$!
trap 'kill "$CAP" 2>/dev/null || true' EXIT

sleep 2
pw-play "$WAV"
sleep 2
kill "$CAP" 2>/dev/null || true
wait "$CAP" 2>/dev/null || true
trap - EXIT

if ! grep -qiE "lamps|yellow|consequence|punished|child|heaven" "$OUT"; then
  echo "FAIL: no caption captured"
  cat "$OUT"
  exit 1
fi
echo "PASS: captured $(grep -c '\[final\]' "$OUT") final segment(s)"
