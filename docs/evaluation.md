# ASR evaluation

`scripts/test-asr-language.py` measures Santhra's sherpa-onnx models on Google FLEURS
read speech. It never implements its own inference path: every sample goes through
`santhra-cli transcribe-batch`, which loads the model once per batch and calls the same
`speech-sherpa` code as `santhra-cli transcribe` and the live caption session.

Evaluation is staged so expensive runs happen only after cheap checks:

| Stage | Samples per model/language | Purpose |
|---|---|---|
| `smoke` | 3 | model loads, inference returns text, no crash |
| `screening` | 25 | quick CER/WER/RTF screening, shortlist candidates |
| `accuracy` | 150 | repeatable accuracy run with fixed manifest |
| `streaming` | — | not implemented (see below) |
| `compare` | — | report three views from a finished run |

Smoke results are a compatibility check, not a statistically meaningful ranking.

## Install and build

```bash
cargo build -p santhra-cli          # batch runner
python3 -m unittest scripts/test_asr_eval.py
```

The evaluator uses only the Python standard library (no numpy/pandas). Python 3.10+.

## Models

Only models listed by `santhra-cli models list --json --language <id>` are evaluated.
The catalog, language compatibility and SHA256 checks live in
`crates/santhra-models/src/lib.rs`; the evaluator never invents model ids or URLs.

```bash
target/debug/santhra-cli models list --language indonesia
target/debug/santhra-cli models download whisper-small-multilingual
```

Downloads are **opt-in**: a run skips uninstalled models with an explicit reason unless
`--download-missing-models` is passed. Models are cached under
`~/.local/share/santhra/models/<id>/` (override with `SANTHRA_MODELS_DIR`).

At the time of writing the catalog has six models. Whisper Tiny/Base/Small cover every
supported language; `zipformer-en-20m` is English-only and the two Paraformer checkpoints
are Chinese-only.

## Dataset

FLEURS is pinned to revision `4683b04af03d2d9549064c7d72060a9a94bb6046`
(CC-BY-4.0). Per-language test archives can be hundreds of MB each, so the first run
needs `--yes-download-large`. Everything is cached under
`~/.cache/santhra/fleurs/<config>/`:

```text
<config>/
├── test.tsv
├── test.tar.gz           # the pinned archive, kept for re-selection
├── manifest.json         # revision, seed, samples, durations, SHA256
└── audio/<sample>.wav    # only the selected samples
```

Selection is deterministic and duration-aware:

- all FLEURS sample ids are ranked by `sha256("<seed>:<sample_id>")`, which is stable
  across Python versions and platforms;
- candidates are bucketed by duration (short `<5 s`, medium `5–10 s`, long `>10 s`) and
  filled round-robin so every bucket is represented;
- a bucket with too few samples is used up and the actual distribution is recorded in
  `manifest.json`; nothing is fabricated;
- duplicate sample ids are dropped;
- every model compared for a language gets the same manifest.

Cached audio is reused only when the dataset revision, split, config, seed, bucket
settings and SHA256 of every file still match. `--force-download` ignores the cache.
`--dataset-revision` overrides the pinned revision. To feed a prepared manifest without
downloading anything, use `--manifest <path>` (it must include `language`).

## Running

```bash
# compatibility check across all languages and every compatible installed model
python3 scripts/test-asr-language.py smoke --all-languages --all-models --samples 3

# screening for one language, all compatible models
python3 scripts/test-asr-language.py screening --language indonesia --all-models --samples 25

# deeper accuracy run for one model
python3 scripts/test-asr-language.py accuracy --language indonesia \
  --model whisper-small-multilingual --samples 150

# compare the newest run
python3 scripts/test-asr-language.py compare --latest
```

Other options: `--seed`, `--manifest`, `--output-dir` (default `evaluation/`),
`--cache-dir`, `--force-download`, `--timeout`, `--threads`, `--provider`, `--chunk-ms`,
`--buffer-ms`, `--no-partial`, `--bucket-short`, `--bucket-medium`, `--max-cer`,
`--min-success-rate`.

Models are never executed in parallel; one batch process runs at a time on CPU.

### Offline runs

After the FLEURS archive and the models are cached, a run needs no network:

```bash
python3 scripts/test-asr-language.py smoke --language english --samples 3
```

Any attempt to download prints the URL to stderr. If the cache is complete, nothing is
fetched and the run works with networking disabled.

## Results

```text
evaluation/
├── runs/<run-id>/
│   ├── config.json        # effective arguments, stage, policy
│   ├── environment.json   # OS, kernel, CPU, RAM, rustc, git commit, sherpa-onnx version
│   ├── manifest.json      # single-language runs; also manifests/<config>.json
│   ├── results.jsonl      # one row per model/language/sample
│   ├── summary.json       # aggregate metrics per pair + policy
│   ├── failures.json      # every non-success status with its error
│   ├── transcripts/<model>/<language>/<sample>.txt
│   └── batch-<language>-<model>.jsonl
└── reports/model-comparison.csv, language-comparison.csv
```

Run ids are timestamps; existing runs are never overwritten.

`results.jsonl` keeps raw text plus normalized text, per-sample CER/WER, timings and the
status (`success`, `empty_transcript`, `timeout`, `audio_error`, `model_error`,
`process_error`). A failed inference is never turned into a normal empty transcript.

`summary.json` reports corpus-level CER/WER (total edits / total reference units) next to
mean per-sample scores, plus success rate, exact-match rate, SER, RTF, model load time,
peak RSS and model size.

## Metrics and normalization

- **CER** = (substitutions + deletions + insertions) / reference characters.
- **WER** = the same over whitespace tokens. Reported only for space-segmented
  languages. Chinese and Japanese are `n/a` because there is no agreed word tokenizer;
  their primary metric is CER. WER values are never compared across languages.
- **Exact Match**: normalized hypothesis equals normalized reference.
- **SER**: at least one edit error.
- **Success rate**: successful, non-empty inferences / attempted samples.
- **RTF**: inference time / audio duration (<1 means faster than real time, which alone
  does not prove responsive live captions).

Normalization (applied to both reference and hypothesis):

1. Unicode NFKC;
2. casefold;
3. drop Unicode punctuation (`P*`) and whitespace.

Unicode marks (`M*`) and symbols are kept, so Indic scripts, Japanese kana/kanji and
accented Latin stay distinct. Raw text is always preserved alongside the normalized text.

Failure policy: accuracy is computed over successful samples only, success rate is
reported separately, and a model is marked `recommendable: false` below
`--min-success-rate` (default `0.9`). The policy is written into every report. There is
no single opaque score: `compare` prints accuracy, efficiency and live-caption views
separately.

## Performance measurement

Model load time (`load_ms`, one value per batch) is measured with the monotonic clock
around recognizer creation. Per-sample inference time covers chunk feeding plus the final
decode. Peak RSS comes from `/proc/self/status` (`VmHWM`) and is `null` when unavailable.
The batch runner is not warmed up; the first sample includes any lazy initialisation.

## Streaming evaluation

Not implemented yet (spec phase F). `streaming` prints `not applicable` for offline
models and `TODO` for the streaming ones (`zipformer-en-20m`, `paraformer-zh-online`).
`transcribe-batch` already emits `first_partial_ms`, `partial_count` and
`final_caption_ms`, so the stage can consume them without a protocol change.

## Known limitations

- FLEURS is read speech. It does not measure music, singing or lyric transcription.
- Offline models (Whisper, Paraformer offline) are evaluated through the production
  live path, which buffers about 2.5 s (`--buffer-ms`). That is what users experience,
  but it is not a full-file offline decode.
- `--timeout` is a batch wall-clock limit. A hung single inference cannot be interrupted
  in-process; the Python runner also hard-kills the batch after `timeout + 60` seconds.
- Peak RSS is Linux-only and process-wide.
- Per-language model coverage differs by design; the report shows only compatible models.
- No bootstrap confidence intervals yet.

## Testing the evaluator

```bash
bash scripts/test-eval.sh        # level 0 + 1, no network
python3 -m unittest scripts/test_asr_eval.py
```

Covers CER/WER edit cases, empty inputs, Unicode/Indic normalization, CJK behavior, corpus
vs mean aggregation, deterministic sampling and bucket distribution, duplicate rejection,
manifest serialization, revision/hash cache validation, WAV parsing, archive scanning,
JSONL protocol parsing and failure-status handling. No model or network is needed.
