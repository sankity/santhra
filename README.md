# Santhra

Offline live captions for system audio on Linux, built with PipeWire and sherpa-onnx. Santhra
captures what your speakers play — no virtual microphone, no cloud service — and turns it into
streaming captions in a terminal or a desktop window.

Linux is the only target for now; Windows, macOS, and Android are planned.

## Features

- Real-time partial captions plus the last N finalized lines.
- CLI for terminal use, desktop GUI with an optional floating overlay.
- Everything runs locally: no network calls at runtime.
- Caption state, capture, and inference are separate crates (see `docs/ARCHITECTURE.md`).

## Requirements

- Linux with a running PipeWire session.
- Rust 1.85+ (edition 2024; `rust-toolchain.toml` pins the toolchain used in CI).
- PipeWire development headers, for example `pipewire` on Arch and `libpipewire-0.3-dev`
  plus `pkg-config` on Debian/Ubuntu.
- `libxkbcommon-dev` on Debian/Ubuntu (`libxkbcommon` on Arch) — Rune's desktop binary links
  `-lxkbcommon`, so the development package is needed to build `santhra-desktop`.
- `pactl` (PipeWire/PulseAudio tools) for per-application stream enumeration. Without it the
  application list is silently empty; sink capture still works.
- Internet on the first build only: `sherpa-onnx-sys` downloads prebuilt native libraries and
  the speech model is fetched separately.

## Setup

```bash
git clone https://github.com/sankity/santhra.git
cd santhra
scripts/fetch-model.sh        # ~46 MB, streaming Zipformer English, Apache-2.0
```

## Usage

### CLI

```bash
cargo run -p santhra-cli -- live                              # default output
cargo run -p santhra-cli -- live --target <sink-name>         # a specific sink
cargo run -p santhra-cli -- live --target app:<serial>        # one application stream
cargo run -p santhra-cli -- transcribe path/to/file.wav       # 16 kHz mono WAV
cargo run -p santhra-cli -- transcribe-batch manifest.json    # JSONL batch
```

`--target app:<serial>` captions one PipeWire stream, so two browser tabs can be handled
separately. Find the serial with `pw-dump` or the desktop Audio source list (labelled
`App — tab title`). If the stream disappears before capture starts, Santhra falls back to the
default sink.

Other flags: `--no-partial`, `--max-lines <n>`, `--model-dir <path>`, `--threads <n>`,
`--language <id>`, `--model <id>`, `--buffer-ms <n>`. Supported recognition languages: English,
Mandarin Chinese, Spanish, Hindi, Indonesian, French, German, Japanese, Portuguese, Russian.
Language IDs are passed to the selected model; automatic language detection is not offered yet.

Model management:

```bash
cargo run -p santhra-cli -- models list [--language <id>] [--json]
cargo run -p santhra-cli -- models download <id>
cargo run -p santhra-cli -- models verify <id>
cargo run -p santhra-cli -- models root
cargo run -p santhra-cli -- live --language indonesia --model whisper-tiny-id
```

Downloads come from official sherpa-onnx Hugging Face repositories into
`~/.local/share/santhra/models/<id>/` (override with `SANTHRA_MODELS_DIR`) and are verified
against SHA256 + size from the bundled catalog. The desktop Settings → Model page offers the
same manager with progress and cancellation.

### Desktop

```bash
cargo run -p santhra-desktop
```

Settings live in `~/.config/santhra/config.toml`; delete it to reset. An unsupported saved
language falls back to English and an incompatible/stale model id is repaired to the default
for the selected language. Fonts come from fontconfig (`~/.local/share/fonts` after `fc-cache -f`).

### Floating overlay

The overlay is an undecorated, translucent Rune window. The compositor decides which controls
it honours:

| Feature | Wayland | X11 |
|---|---|---|
| Floating window | with a compositor window rule | yes |
| Drag to move | yes (`xdg_toplevel.move`) | yes |
| Edge/corner drag-resize | no | no |
| Transparent background | yes | yes |
| Click-through | yes | yes |
| Always-on-top | no (`set_window_level` is a no-op) | yes |
| Restore position | no | yes |

Width is set in Settings and applied when the overlay opens; height follows caption content.
Width is not applied live (drag-resize is ignored in overlays and this compositor ignores
client-requested Wayland resizes) — reopen the overlay after changing it. The pinned Rune rev
does expose programmatic `OverlayHandle::resize`; the migration to `rune::Overlay` is a recorded
decision in `docs/ARCHITECTURE.md`, not yet implemented.

Scrolling compositors tile new windows; add a rule to float the overlay:

```toml
[[window_rule]]
match.title = "^Santhra captions$"
default_floating = true
```

Captions are bottom-anchored: the oldest lines are clipped when the text is too tall. The
overlay keeps updating when the main window is minimized or unfocused. Dragging works only
while click-through is off. Verified on Umbriel/Wayland; the Settings page warns about the two
Wayland gaps when `WAYLAND_DISPLAY` is set.

## Models and licenses

| Model | Source | License | Size | Files |
|---|---|---|---:|---|
| Streaming Zipformer English 20M INT8 | [sherpa-onnx releases](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models) (HF mirror `csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17`) | Apache-2.0 | ~46 MB | `encoder-epoch-99-avg-1.int8.onnx`, `decoder-epoch-99-avg-1.onnx`, `joiner-epoch-99-avg-1.int8.onnx`, `tokens.txt` |
| Offline Paraformer Chinese INT8 | [HF](https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2023-03-28) | MIT | ~223 MB | `model.int8.onnx`, `tokens.txt` |
| Streaming Paraformer Chinese INT8 | [HF](https://huggingface.co/csukuangfj/sherpa-onnx-streaming-paraformer-zh) | Apache-2.0 | ~237 MB | `encoder.int8.onnx`, `decoder.int8.onnx`, `tokens.txt` |
| Whisper Tiny multilingual INT8 | [HF](https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny) | MIT | ~103 MB | `tiny-encoder.int8.onnx`, `tiny-decoder.int8.onnx`, `tiny-tokens.txt` |
| Whisper Base multilingual INT8 | [HF](https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base) | MIT | ~161 MB | `base-encoder.int8.onnx`, `base-decoder.int8.onnx`, `base-tokens.txt` |
| Whisper Small multilingual INT8 | [HF](https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small) | MIT | ~375 MB | `small-encoder.int8.onnx`, `small-decoder.int8.onnx`, `small-tokens.txt` |
| Whisper Tiny Indonesian fp32 | upstream [maleo-ai/whisper-tiny-id](https://huggingface.co/maleo-ai/whisper-tiny-id), export [sankity/sherpa-onnx-whisper-tiny-id](https://huggingface.co/sankity/sherpa-onnx-whisper-tiny-id) | Apache-2.0 | ~153 MB | `tiny-id-encoder.onnx`, `tiny-id-decoder.onnx`, `tiny-id-tokens.txt` |
| Nemotron 3.5 ASR 0.6B streaming INT8 | upstream [nvidia/nemotron-3.5-asr-streaming-0.6b](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b), export [csukuangfj2](https://huggingface.co/csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11) | OpenMDW-1.1 | ~682 MB | `encoder.int8.onnx`, `decoder.int8.onnx`, `joiner.int8.onnx`, `tokens.txt` |
| FLEURS test split (evaluation audio) | [google/fleurs](https://huggingface.co/datasets/google/fleurs) | CC-BY-4.0 | — | per-language test archives, cached locally |

Whisper Tiny/Base/Small cover all supported languages; `whisper-tiny-id` is Indonesian-only
(fp32; the int8 export measured worse); Nemotron covers 39 locales but not Indonesian.
Offline Whisper/Paraformer models buffer about 2.5 s and emit finals only. Licences and source
cards must be re-verified before public redistribution.

## Testing and CI

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
python3 -m unittest scripts/test_asr_eval.py       # 41 offline evaluator tests
bash scripts/test-eval.sh                          # level 0 + 1 ASR checks, no network
bash scripts/live-smoke.sh                         # plays a sample WAV through the default sink
```

GitHub Actions runs the Rust checks plus the Python unit tests on `ubuntu-latest`
(Linux only — see `docs/ARCHITECTURE.md`, Decisions).

## ASR evaluation

`scripts/test-asr-language.py` measures the catalog on Google FLEURS read speech. Every sample
goes through the production `santhra-cli transcribe-batch` path, which loads a model once per
batch and uses the same `speech-sherpa` code as the live session. Python 3.10+, standard library
only.

| Stage | Samples per model/language | Purpose |
|---|---|---|
| `smoke` | 3 | model loads, produces text, no crash (compatibility check, not a ranking) |
| `screening` | 25 | quick CER/WER/RTF shortlisting |
| `accuracy` | 150 | repeatable run with a fixed manifest |
| `streaming` | — | not implemented yet |
| `compare` | — | accuracy / efficiency / live-caption views from a finished run |

```bash
cargo build -p santhra-cli
python3 scripts/test-asr-language.py smoke --language english --samples 3
python3 scripts/test-asr-language.py smoke --all-languages --all-models --samples 3
python3 scripts/test-asr-language.py screening --language indonesia --all-models --samples 25
python3 scripts/test-asr-language.py accuracy --language indonesia --model whisper-small-multilingual --samples 150
python3 scripts/test-asr-language.py compare --latest
```

Other options: `--seed`, `--manifest`, `--output-dir`, `--cache-dir`, `--force-download`,
`--timeout`, `--threads`, `--provider`, `--chunk-ms`, `--buffer-ms`, `--no-partial`,
`--bucket-short`, `--bucket-medium`, `--max-cer`, `--min-success-rate`. Only catalog models
listed for a language are evaluated; downloads and dataset fetches are opt-in
(`--download-missing-models`, `--yes-download-large`). Models run one batch at a time on CPU.

FLEURS is pinned to revision `4683b04af03d2d9549064c7d72060a9a94bb6046` (CC-BY-4.0).
Archives can be hundreds of MB; the first run needs `--yes-download-large`. Cache layout:

```text
~/.cache/santhra/fleurs/<config>/{test.tsv,test.tar.gz,manifest.json,audio/<sample>.wav}
```

Selection is deterministic (`sha256("<seed>:<sample_id>")`) and duration-bucketed
(`<5 s`, `5–10 s`, `>10 s`), with duplicates dropped and the actual distribution recorded in
`manifest.json`. Every model in a language shares the same manifest. Cache is reused only when
revision, config, seed, bucket settings and every SHA256 still match; `--force-download`
overrides it and `--manifest <path>` runs with no download at all.

Results land in `evaluation/runs/<timestamp>-<stage>/` (`config.json`, `environment.json`,
`manifest.json`, `results.jsonl`, `summary.json`, `failures.json`, transcripts, batch JSONL) and
`evaluation/reports/*.csv`. Run ids are timestamps and existing runs are never overwritten.

Metrics: CER/WER (edits / reference units; WER is `n/a` for Chinese and Japanese), Exact Match,
SER, success rate, RTF, model load time, peak RSS, model size. Text is normalized with NFKC +
casefold + Unicode punctuation/whitespace removal; raw text is preserved next to normalized
text. Failed inferences are reported as failures, never as empty successful transcripts; a
model below `--min-success-rate` (default 0.9) is marked `recommendable: false`.

Limitations: FLEURS is read speech, not music or lyrics; offline models are evaluated through
the ~2.5 s buffering live path, not a full-file decode; `--timeout` is a batch wall-clock limit;
peak RSS is Linux-only and process-wide; no bootstrap confidence intervals yet. Streaming
metrics (`first_partial_ms`, `partial_count`, `final_caption_ms`) are already emitted by the
batch runner but the `streaming` stage is not implemented.

## Documentation

- `README.md` — this file: usage, models, evaluation, licenses.
- `docs/ARCHITECTURE.md` — crate boundaries, dependencies, contracts, decisions.

## License

GPL-3.0-or-later. Model licences are listed above; sherpa-onnx is Apache-2.0.
