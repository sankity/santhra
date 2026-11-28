#!/usr/bin/env bash
# Level 0 + 1 ASR evaluation checks. No network, no FLEURS download.
# Level 0: pure logic (unit tests, syntax, rust workspace).
# Level 1: real inference over the model's own test WAVs via --manifest.
#         Skipped cleanly when the local zipformer-en-20m model is absent.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

FIXTURE="${SANTHRA_EVAL_FIXTURE:-models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/test_wavs}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/santhra-eval-test.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

step() { printf '\n=== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

step "level 0: evaluator unit tests"
python3 -m unittest scripts/test_asr_eval.py

step "level 0: python syntax"
python3 -m py_compile scripts/test-asr-language.py scripts/asr_metrics.py scripts/test_asr_eval.py

step "level 0: rust workspace tests"
cargo test --all --quiet

if [[ ! -f "$FIXTURE/trans.txt" ]]; then
  step "level 1: skipped (no local model)"
  echo "expected the model at $FIXTURE"
  echo "fetch it with scripts/fetch-model.sh, or point SANTHRA_MODELS_DIR at an install"
  exit 0
fi

step "level 1: build santhra-cli"
cargo build -p santhra-cli

step "level 1: build manifests from the model's test wavs"
python3 - "$FIXTURE" "$WORK" <<'PY'
import json, pathlib, sys

fixture, work = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
refs = dict(line.split(" ", 1) for line in (fixture / "trans.txt").read_text().splitlines())
samples = [
    {
        "id": name,
        "audio_path": str((fixture / name).resolve()),
        "reference": text,
        "duration_seconds": 3.0,
    }
    for name, text in refs.items()
    if name != "8k.wav"
]


def write_manifest(name: str, rows: list[dict]) -> None:
    (work / name).write_text(
        json.dumps(
            {"dataset": "local", "language": "english", "config": "en_us", "seed": 42, "samples": rows}
        )
    )


(work / "broken.wav").write_bytes(b"not a wav")
write_manifest("english.json", samples)
write_manifest(
    "broken.json",
    [
        {
            "id": "broken.wav",
            "audio_path": str((work / "broken.wav").resolve()),
            "reference": "hello",
            "duration_seconds": 1.0,
        }
    ],
)
PY

step "level 1: smoke run (zipformer-en-20m)"
python3 scripts/test-asr-language.py smoke \
  --manifest "$WORK/english.json" --model zipformer-en-20m --output-dir "$WORK/eval"

RUN="${WORK}/eval/runs/$(ls "$WORK/eval/runs" | head -1)"
[[ -f "$RUN/summary.json" ]] || fail "no summary.json under $RUN"
META="$(cat "$RUN"/batch-*.jsonl | grep -c '"type":"meta"')"
[[ "$META" == "1" ]] || fail "expected one model load per batch, saw $META meta records"
echo "OK: model loaded once for the whole batch"

python3 - "$RUN" <<'PY' || fail "batch segments missing timing"
import glob, json, sys
records = [json.loads(line) for path in glob.glob(sys.argv[1] + "/batch-*.jsonl") for line in open(path)]
samples = [record for record in records if record.get("type") == "sample"]
assert samples, "no sample records"
for sample in samples:
    assert "segments" in sample, "sample has no segments key"
    for segment in sample["segments"]:
        assert segment["start_ms"] >= 0, "negative start_ms"
        assert segment["end_ms"] is None or segment["end_ms"] >= segment["start_ms"], "bad end_ms"
print("OK: segments carry timing")
PY

step "level 1: failure path (broken audio -> audio_error, exit 1)"
set +e
python3 scripts/test-asr-language.py smoke \
  --manifest "$WORK/broken.json" --model zipformer-en-20m --output-dir "$WORK/eval-broken"
CODE=$?
set -e
[[ "$CODE" == "1" ]] || fail "expected exit 1 on a broken sample, got $CODE"
grep -q '"status": "audio_error"' "$WORK"/eval-broken/runs/*/failures.json \
  || fail "broken sample was not reported as audio_error"
echo "OK: failure reported, never scored as an empty transcript"

step "all checks passed"
